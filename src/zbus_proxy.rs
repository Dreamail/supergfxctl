//! # DBus interface proxy for: `org.asuslinux.Gfx`
//!
//! This code was generated by `zbus-xmlgen` `1.0.0` from DBus introspection data.
//! Source: `Interface '/org/supergfxctl/Gfx' from service 'org.asuslinux.Daemon' on system bus`.
//!
//! You may prefer to adapt it, instead of using it verbatim.
//!
//! More information can be found in the
//! [Writing a client proxy](https://zeenix.pages.freedesktop.org/zbus/client.html)
//! section of the zbus documentation.
//!
//! This DBus object implements
//! [standard DBus interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//!
//! * [`zbus::fdo::PropertiesProxy`]
//! * [`zbus::fdo::IntrospectableProxy`]
//! * [`zbus::fdo::PeerProxy`]
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

use std::sync::mpsc::Sender;

use zbus::{dbus_proxy, Connection, Message, Result};

use crate::{
    pci_device::{GfxMode, GfxPower, GfxRequiredUserAction},
    DBUS_IFACE_PATH,
};

#[dbus_proxy(interface = "org.supergfxctl.Daemon")]
trait Daemon {
    /// Version method
    fn version(&self) -> zbus::Result<String>;

    /// Power method
    fn power(&self) -> zbus::Result<GfxPower>;

    /// SetMode method
    fn set_mode(&self, mode: &GfxMode) -> zbus::Result<GfxRequiredUserAction>;

    /// Get the `String` name of the pending mode change if any
    fn pending_mode(&self) -> zbus::Result<GfxMode>;

    /// Get the `String` name of the pending required user action if any
    fn pending_user_action(&self) -> zbus::Result<GfxRequiredUserAction>;

    /// Mode method
    fn mode(&self) -> zbus::Result<GfxMode>;

    fn supported(&self) -> zbus::Result<Vec<GfxMode>>;

    /// Vendor method
    fn vendor(&self) -> zbus::Result<String>;

    /// NotifyAction signal
    #[dbus_proxy(signal)]
    fn notify_action(&self, action: GfxRequiredUserAction) -> zbus::Result<()>;

    /// NotifyGfx signal
    #[dbus_proxy(signal)]
    fn notify_gfx(&self, mode: GfxMode) -> zbus::Result<()>;
}

pub struct GfxProxy<'a>(pub DaemonProxy<'a>);

impl<'a> GfxProxy<'a> {
    #[inline]
    pub fn new(conn: &Connection) -> Result<Self> {
        let proxy = DaemonProxy::new_for(conn, "org.supergfxctl.Daemon", DBUS_IFACE_PATH)?;
        Ok(GfxProxy(proxy))
    }

    #[inline]
    pub fn new_for(conn: &Connection, destination: &'a str, path: &'a str) -> Result<Self> {
        let proxy = DaemonProxy::new_for(conn, destination, path)?;
        Ok(GfxProxy(proxy))
    }

    #[inline]
    pub fn new_for_owned(conn: Connection, destination: String, path: String) -> Result<Self> {
        let proxy = DaemonProxy::new_for_owned(conn, destination, path)?;
        Ok(GfxProxy(proxy))
    }

    #[inline]
    pub fn proxy(&self) -> &DaemonProxy<'a> {
        &self.0
    }

    #[inline]
    pub fn get_version(&self) -> Result<String> {
        self.0.version()
    }

    #[inline]
    pub fn get_vendor(&self) -> Result<String> {
        self.0.vendor()
    }

    #[inline]
    pub fn get_pwr(&self) -> Result<GfxPower> {
        self.0.power()
    }

    #[inline]
    pub fn get_mode(&self) -> Result<GfxMode> {
        self.0.mode()
    }

    #[inline]
    pub fn get_supported_modes(&self) -> Result<Vec<GfxMode>> {
        self.0.supported()
    }

    #[inline]
    pub fn pending_mode(&self) -> Result<GfxMode> {
        self.0.pending_mode()
    }

    #[inline]
    pub fn pending_user_action(&self) -> Result<GfxRequiredUserAction> {
        self.0.pending_user_action()
    }

    #[inline]
    pub fn write_mode(&self, mode: &GfxMode) -> Result<GfxRequiredUserAction> {
        self.0.set_mode(mode)
    }

    #[inline]
    pub fn connect_notify_action(
        &self,
        send: Sender<GfxRequiredUserAction>,
    ) -> zbus::fdo::Result<()> {
        self.0.connect_notify_action(move |data| {
            send.send(data)
                .map_err(|err| zbus::fdo::Error::Failed(err.to_string()))?;
            Ok(())
        })
    }

    #[inline]
    pub fn connect_notify_gfx(&self, send: Sender<GfxMode>) -> zbus::fdo::Result<()> {
        self.0.connect_notify_gfx(move |data| {
            send.send(data)
                .map_err(|err| zbus::fdo::Error::Failed(err.to_string()))?;
            Ok(())
        })
    }

    #[inline]
    pub fn next_signal(&self) -> Result<Option<Message>> {
        self.0.next_signal()
    }
}
