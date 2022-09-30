use ::zbus::dbus_interface;
use log::{error, info, warn};
use zbus::SignalContext;
use zvariant::ObjectPath;

use crate::{
    config::GfxConfigDbus,
    pci_device::{GfxMode, GfxPower, GfxRequiredUserAction},
    DBUS_IFACE_PATH, VERSION,
};

use super::controller::CtrlGraphics;

#[dbus_interface(name = "org.supergfxctl.Daemon")]
impl CtrlGraphics {
    /// Get supergfxd version
    fn version(&self) -> zbus::fdo::Result<String> {
        Ok(VERSION.to_string())
    }

    /// Get the current graphics mode:
    /// enum GfxMode {
    ///     Hybrid,
    ///     Integrated,
    ///     Compute,
    ///     Vfio,
    ///     Egpu,
    ///     None,
    /// }
    async fn mode(&self) -> zbus::fdo::Result<GfxMode> {
        let config = self.config.lock().await;
        self.get_gfx_mode(&config).map_err(|err| {
            error!("{}", err);
            zbus::fdo::Error::Failed(format!("GFX fail: {}", err))
        })
    }

    /// Get list of supported modes
    async fn supported(&self) -> zbus::fdo::Result<Vec<GfxMode>> {
        Ok(self.get_supported_modes().await)
    }

    /// Get the vendor name of the dGPU
    async fn vendor(&self) -> zbus::fdo::Result<String> {
        Ok(<&str>::from(self.get_gfx_vendor().await).to_string())
    }

    /// Get the current power status:
    /// enum GfxPower {
    ///     Active,
    ///     Suspended,
    ///     Off,
    ///     AsusDisabled,
    ///     Unknown,
    /// }
    async fn power(&self) -> zbus::fdo::Result<GfxPower> {
        let dgpu = self.dgpu.lock().await;
        return dgpu.get_runtime_status().map_err(|err| {
            error!("{}", err);
            zbus::fdo::Error::Failed(format!("GFX fail: {}", err))
        });
    }

    /// Set the graphics mode:
    /// enum GfxMode {
    ///     Hybrid,
    ///     Integrated,
    ///     Compute,
    ///     Vfio,
    ///     Egpu,
    ///     None,
    /// }
    ///
    /// Returns action required:
    /// enum GfxRequiredUserAction {
    ///     Logout,
    ///     Integrated,
    ///     AsusGpuMuxDisable,
    ///     None,
    /// }
    async fn set_mode(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        mode: GfxMode,
    ) -> zbus::fdo::Result<GfxRequiredUserAction> {
        info!("Switching gfx mode to {}", <&str>::from(mode));
        let msg = self.set_gfx_mode(mode).await.map_err(|err| {
            error!("{}", err);
            zbus::fdo::Error::Failed(format!("GFX fail: {}", err))
        })?;

        Self::notify_action(&ctxt, &msg)
            .await
            .unwrap_or_else(|err| warn!("{}", err));

        Self::notify_gfx(&ctxt, &mode)
            .await
            .unwrap_or_else(|err| warn!("{}", err));

        Ok(msg)
    }

    /// Get the `String` name of the pending mode change if any
    async fn pending_mode(&self) -> zbus::fdo::Result<GfxMode> {
        Ok(self.get_pending_mode().await)
    }

    /// Get the `String` name of the pending required user action if any
    async fn pending_user_action(&self) -> zbus::fdo::Result<GfxRequiredUserAction> {
        Ok(self.get_pending_user_action().await)
    }

    /// Get the base config, args in order are:
    /// pub mode: GfxMode,
    /// vfio_enable: bool,
    /// vfio_save: bool,
    /// compute_save: bool,
    /// always_reboot: bool,
    /// no_logind: bool,
    /// logout_timeout_s: u64,
    async fn config(&self) -> zbus::fdo::Result<GfxConfigDbus> {
        let cfg = self.config.lock().await;
        let cfg = GfxConfigDbus::from(&*cfg);
        Ok(cfg)
    }

    /// Set the base config, args in order are:
    /// pub mode: GfxMode,
    /// vfio_enable: bool,
    /// vfio_save: bool,
    /// compute_save: bool,
    /// always_reboot: bool,
    /// no_logind: bool,
    /// logout_timeout_s: u64,
    async fn set_config(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        config: GfxConfigDbus,
    ) -> zbus::fdo::Result<()> {
        let do_mode_change;
        let mode;

        {
            let mut cfg = self.config.lock().await;

            do_mode_change = cfg.mode == config.mode;
            mode = cfg.mode;

            cfg.vfio_enable = config.vfio_enable;
            cfg.vfio_save = config.vfio_save;
            cfg.compute_save = config.compute_save;
            cfg.always_reboot = config.always_reboot;
            cfg.no_logind = config.no_logind;
            cfg.logout_timeout_s = config.logout_timeout_s;
        }

        if do_mode_change {
            self.set_mode(ctxt, mode).await.ok();
        }

        Ok(())
    }

    /// Recieve a notification if the graphics mode changes and to which mode
    #[dbus_interface(signal)]
    async fn notify_gfx(signal_ctxt: &SignalContext<'_>, vendor: &GfxMode) -> zbus::Result<()> {}

    /// Recieve a notification on required action if mode changes
    #[dbus_interface(signal)]
    async fn notify_action(
        signal_ctxt: &SignalContext<'_>,
        action: &GfxRequiredUserAction,
    ) -> zbus::Result<()> {
    }
}

impl CtrlGraphics {
    pub async fn add_to_server(self, server: &mut zbus::ObjectServer) {
        server
            .at(&ObjectPath::from_str_unchecked(DBUS_IFACE_PATH), self)
            .await
            .map_err(|err| {
                warn!("CtrlGraphics: add_to_server {}", err);
                err
            })
            .ok();
    }
}
