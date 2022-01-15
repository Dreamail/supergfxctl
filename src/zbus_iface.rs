use ::zbus::dbus_interface;
use log::{error, info, warn};
use zvariant::ObjectPath;

use crate::{
    gfx_vendors::{GfxMode, GfxPower, GfxRequiredUserAction},
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
        self.dgpu().get_runtime_status().map_err(|err| {
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

    #[dbus_interface(signal)]
    fn notify_gfx(&self, vendor: &GfxMode) -> zbus::Result<()> {}

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
