#![allow(clippy::type_complexity)]

//! # DBus interface proxy for: `org.supergfxctl.Daemon`
//!
//! This code was generated by `zbus-xmlgen` `3.0.0` from DBus introspection data.
//! Source: `Interface '/org/supergfxctl/Gfx' from service 'org.supergfxctl.Daemon' on system bus`.
//!
//! You may prefer to adapt it, instead of using it verbatim.
//!
//! More information can be found in the
//! [Writing a client proxy](https://dbus.pages.freedesktop.org/zbus/client.html)
//! section of the zbus documentation.
//!
//! This DBus object implements
//! [standard DBus interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//!
//! * [`zbus::fdo::IntrospectableProxy`]
//! * [`zbus::fdo::PropertiesProxy`]
//! * [`zbus::fdo::PeerProxy`]
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

use zbus::proxy;

use crate::{
    actions::UserActionRequired,
    pci_device::{GfxMode, GfxPower},
};

#[proxy(
    interface = "org.supergfxctl.Daemon",
    default_path = "/org/supergfxctl/Gfx"
)]
trait Daemon {
    /// Version method
    fn version(&self) -> zbus::Result<String>;

    /// Get the base config, args in order are:
    /// pub mode: GfxMode,
    /// vfio_enable: bool,
    /// vfio_save: bool,
    /// compute_save: bool,
    /// always_reboot: bool,
    /// no_logind: bool,
    /// logout_timeout_s: u64,
    fn config(&self) -> zbus::Result<(u32, bool, bool, bool, bool, bool, u64, bool)>;

    /// Set the base config, args in order are:
    /// pub mode: GfxMode,
    /// vfio_enable: bool,
    /// vfio_save: bool,
    /// compute_save: bool,
    /// always_reboot: bool,
    /// no_logind: bool,
    /// logout_timeout_s: u64,
    fn set_config(
        &self,
        config: &(u32, bool, bool, bool, bool, bool, u64, bool),
    ) -> zbus::Result<()>;

    /// Get the current power status
    fn power(&self) -> zbus::Result<GfxPower>;

    /// Set the graphics mode. Returns action required.
    fn set_mode(&self, mode: &GfxMode) -> zbus::Result<UserActionRequired>;

    /// Get the `String` name of the pending mode change if any
    fn pending_mode(&self) -> zbus::Result<GfxMode>;

    /// Get the `String` name of the pending required user action if any
    fn pending_user_action(&self) -> zbus::Result<UserActionRequired>;

    /// Get the current graphics mode
    fn mode(&self) -> zbus::Result<GfxMode>;

    /// Get list of supported modes
    fn supported(&self) -> zbus::Result<Vec<GfxMode>>;

    /// Get the vendor name of the dGPU
    fn vendor(&self) -> zbus::Result<String>;

    /// Be notified when the dgpu status changes
    #[zbus(signal)]
    fn notify_gfx_status(&self, status: GfxPower) -> zbus::Result<()>;

    /// NotifyAction signal
    #[zbus(signal)]
    fn notify_action(&self, action: UserActionRequired) -> zbus::Result<()>;

    /// NotifyGfx signal
    #[zbus(signal)]
    fn notify_gfx(&self, mode: GfxMode) -> zbus::Result<()>;
}
