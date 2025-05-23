use ::zbus::interface;
use log::{error, info, warn};
use zbus::{object_server::SignalEmitter, zvariant::ObjectPath};

use crate::{
    actions::UserActionRequired,
    config::GfxConfigDbus,
    pci_device::{GfxMode, GfxPower},
    special_asus::{asus_gpu_mux_mode, AsusGpuMuxMode},
    DBUS_IFACE_PATH, VERSION,
};

use super::controller::CtrlGraphics;

#[interface(name = "org.supergfxctl.Daemon")]
impl CtrlGraphics {
    /// Get supergfxd version
    fn version(&self) -> zbus::fdo::Result<String> {
        Ok(VERSION.to_string())
    }

    /// Get the current graphics mode:
    /// ```rust
    /// enum GfxMode {
    ///     Hybrid,
    ///     Integrated,
    ///     NvidiaNoModeset,
    ///     Vfio,
    ///     AsusEgpu,
    ///     AsusMuxDgpu,
    ///     None,
    /// }
    /// # use supergfxctl::pci_device;
    /// # assert_eq!(pci_device::GfxMode::None as u8, 6);
    /// # assert_eq!(pci_device::GfxMode::Hybrid as u8, GfxMode::Hybrid as u8);
    /// # assert_eq!(pci_device::GfxMode::Integrated as u8, GfxMode::Integrated as u8);
    /// # assert_eq!(pci_device::GfxMode::NvidiaNoModeset  as u8, GfxMode::NvidiaNoModeset as u8);
    /// # assert_eq!(pci_device::GfxMode::Vfio as u8, GfxMode::Vfio as u8);
    /// # assert_eq!(pci_device::GfxMode::AsusEgpu as u8, GfxMode::AsusEgpu as u8);
    /// # assert_eq!(pci_device::GfxMode::AsusMuxDgpu as u8, GfxMode::AsusMuxDgpu as u8);
    /// # assert_eq!(pci_device::GfxMode::None as u8, GfxMode::None as u8);
    /// ```
    async fn mode(&self) -> zbus::fdo::Result<GfxMode> {
        if let Ok(state) = asus_gpu_mux_mode() {
            if state == AsusGpuMuxMode::Discreet {
                return Ok(GfxMode::AsusMuxDgpu);
            }
        }
        let config = self.config.lock().await;
        self.get_gfx_mode(&config).map_err(|err| {
            error!("{}", err);
            zbus::fdo::Error::Failed(format!("GFX fail: {}", err))
        })
    }

    /// Get list of supported modes
    async fn supported(&self) -> zbus::fdo::Result<Vec<GfxMode>> {
        if let Ok(state) = asus_gpu_mux_mode() {
            if state == AsusGpuMuxMode::Discreet {
                return Ok(vec![GfxMode::AsusMuxDgpu]);
            }
        }
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
        if let Ok(state) = asus_gpu_mux_mode() {
            if state == AsusGpuMuxMode::Discreet {
                return Ok(GfxPower::AsusMuxDiscreet);
            }
        }
        let dgpu = self.dgpu.lock().await;
        dgpu.get_runtime_status().map_err(|err| {
            error!("{}", err);
            zbus::fdo::Error::Failed(format!("GFX fail: {}", err))
        })
    }

    /// Set the graphics mode:
    /// ```rust
    /// enum GfxMode {
    ///     Hybrid,
    ///     Integrated,
    ///     NvidiaNoModeset,
    ///     Vfio,
    ///     AsusEgpu,
    ///     AsusMuxDgpu,
    ///     None,
    /// }
    /// # use supergfxctl::pci_device;
    /// # assert_eq!(pci_device::GfxMode::None as u8, 6);
    /// # assert_eq!(pci_device::GfxMode::Hybrid as u8, GfxMode::Hybrid as u8);
    /// # assert_eq!(pci_device::GfxMode::Integrated as u8, GfxMode::Integrated as u8);
    /// # assert_eq!(pci_device::GfxMode::NvidiaNoModeset  as u8, GfxMode::NvidiaNoModeset as u8);
    /// # assert_eq!(pci_device::GfxMode::Vfio as u8, GfxMode::Vfio as u8);
    /// # assert_eq!(pci_device::GfxMode::AsusEgpu as u8, GfxMode::AsusEgpu as u8);
    /// # assert_eq!(pci_device::GfxMode::AsusMuxDgpu as u8, GfxMode::AsusMuxDgpu as u8);
    /// # assert_eq!(pci_device::GfxMode::None as u8, GfxMode::None as u8);
    /// ```
    ///
    /// Returns action required:
    /// ```rust
    /// enum UserActionRequired {
    ///     Logout,
    ///     Reboot,
    ///     SwitchToIntegrated,
    ///     AsusEgpuDisable,
    ///     Nothing,
    /// }
    /// # use supergfxctl::actions;
    /// # assert_eq!(actions::UserActionRequired::Nothing as u8, 4);
    /// # assert_eq!(actions::UserActionRequired::Logout as u8, UserActionRequired::Logout as u8);
    /// # assert_eq!(actions::UserActionRequired::Reboot as u8, UserActionRequired::Reboot as u8);
    /// # assert_eq!(actions::UserActionRequired::SwitchToIntegrated as u8, UserActionRequired::SwitchToIntegrated as u8);
    /// # assert_eq!(actions::UserActionRequired::AsusEgpuDisable as u8, UserActionRequired::AsusEgpuDisable as u8);
    /// # assert_eq!(actions::UserActionRequired::Nothing as u8, UserActionRequired::Nothing as u8);
    /// ```
    async fn set_mode(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalEmitter<'_>,
        mode: GfxMode,
    ) -> zbus::fdo::Result<UserActionRequired> {
        info!("Switching gfx mode to {mode}");
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
    async fn pending_user_action(&self) -> zbus::fdo::Result<UserActionRequired> {
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
        #[zbus(signal_context)] ctxt: SignalEmitter<'_>,
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
            cfg.always_reboot = config.always_reboot;
            cfg.no_logind = config.no_logind;
            cfg.logout_timeout_s = config.logout_timeout_s;
        }

        if do_mode_change {
            self.set_mode(ctxt, mode).await.ok();
        }

        Ok(())
    }

    /// Be notified when the dgpu status changes:
    /// enum GfxPower {
    ///     Active,
    ///     Suspended,
    ///     Off,
    ///     AsusDisabled,
    ///     AsusMuxDiscreet,
    ///     Unknown,
    /// }
    #[zbus(signal)]
    pub async fn notify_gfx_status(
        signal_ctxt: &SignalEmitter<'_>,
        status: &GfxPower,
    ) -> zbus::Result<()> {
    }

    /// Recieve a notification if the graphics mode changes and to which mode
    #[zbus(signal)]
    async fn notify_gfx(signal_ctxt: &SignalEmitter<'_>, vendor: &GfxMode) -> zbus::Result<()> {}

    /// Recieve a notification on required action if mode changes
    #[zbus(signal)]
    async fn notify_action(
        signal_ctxt: &SignalEmitter<'_>,
        action: &UserActionRequired,
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
