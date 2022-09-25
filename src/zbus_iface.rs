use ::zbus::dbus_interface;
use log::{error, info, warn};
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
    /// enum {
    ///     Hybrid,
    ///     Dedicated,
    ///     Integrated,
    ///     Compute,
    ///     Vfio,
    ///     Egpu,
    /// }
    fn mode(&self) -> zbus::fdo::Result<GfxMode> {
        self.get_gfx_mode().map_err(|err| {
            error!("{}", err);
            zbus::fdo::Error::Failed(format!("GFX fail: {}", err))
        })
    }

    /// Get list of supported modes
    fn supported(&self) -> zbus::fdo::Result<Vec<GfxMode>> {
        Ok(self.get_supported_modes())
    }

    /// Get the vendor nae of the dGPU
    fn vendor(&self) -> zbus::fdo::Result<String> {
        Ok(<&str>::from(self.get_gfx_vendor()).to_string())
    }

    /// Get the current power status:
    /// enum {
    ///     Active,
    ///     Suspended,
    ///     Off,
    ///     Unknown,
    /// }
    fn power(&self) -> zbus::fdo::Result<GfxPower> {
        self.dgpu.get_runtime_status().map_err(|err| {
            error!("{}", err);
            zbus::fdo::Error::Failed(format!("GFX fail: {}", err))
        })
    }

    /// Set the graphics mode:
    /// enum {
    ///     Hybrid,
    ///     Dedicated,
    ///     Integrated,
    ///     Compute,
    ///     Vfio,
    ///     Egpu,
    /// }
    ///
    /// Returns action required:
    /// enum {
    ///     Logout,
    ///     Reboot,
    ///     Integrated,
    ///     None,
    /// }
    fn set_mode(&mut self, mode: GfxMode) -> zbus::fdo::Result<GfxRequiredUserAction> {
        info!("Switching gfx mode to {}", <&str>::from(mode));
        let msg = self.set_gfx_mode(mode).map_err(|err| {
            error!("{}", err);
            zbus::fdo::Error::Failed(format!("GFX fail: {}", err))
        })?;

        self.notify_action(&msg)
            .unwrap_or_else(|err| warn!("{}", err));

        self.notify_gfx(&mode)
            .unwrap_or_else(|err| warn!("{}", err));

        Ok(msg)
    }

    /// Get the `String` name of the pending mode change if any
    fn pending_mode(&self) -> zbus::fdo::Result<GfxMode> {
        Ok(self.get_pending_mode())
    }

    /// Get the `String` name of the pending required user action if any
    fn pending_user_action(&self) -> zbus::fdo::Result<GfxRequiredUserAction> {
        Ok(self.get_pending_user_action())
    }

    /// Get the base config, args in order are:
    /// pub mode: GfxMode,
    /// vfio_enable: bool,
    /// vfio_save: bool,
    /// compute_save: bool,
    /// always_reboot: bool,
    /// no_logind: bool,
    /// logout_timeout_s: u64,
    fn config(&self) -> zbus::fdo::Result<GfxConfigDbus> {
        let cfg = self
            .config
            .try_lock()
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
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
    fn set_config(&mut self, config: GfxConfigDbus) -> zbus::fdo::Result<()> {
        let do_mode_change;
        let mode;

        {
            let mut cfg = self
                .config
                .try_lock()
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

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
            self.set_mode(mode).ok();
        }

        Ok(())
    }

    /// Recieve a notification if the graphics mode changes and to which mode
    #[dbus_interface(signal)]
    fn notify_gfx(&self, vendor: &GfxMode) -> zbus::Result<()> {}

    /// Recieve a notification on required action if mode changes
    #[dbus_interface(signal)]
    fn notify_action(&self, action: &GfxRequiredUserAction) -> zbus::Result<()> {}
}

impl CtrlGraphics {
    pub fn add_to_server(self, server: &mut zbus::ObjectServer) {
        server
            .at(&ObjectPath::from_str_unchecked(DBUS_IFACE_PATH), self)
            .map_err(|err| {
                warn!("CtrlGraphics: add_to_server {}", err);
                err
            })
            .ok();
    }
}
