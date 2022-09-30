use log::{debug, info, warn};
use std::fs;
use std::io::{Read, Write};
use std::str::FromStr;
use std::{fs::write, path::PathBuf};

use crate::error::GfxError;
use crate::special_asus::asus_dgpu_exists;
use crate::{do_driver_action, NVIDIA_DRIVERS};

use serde_derive::{Deserialize, Serialize};
use zvariant_derive::Type;

const PCI_BUS_PATH: &str = "/sys/bus/pci";

#[derive(Debug, Type, PartialEq, Eq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxPower {
    Active,
    Suspended,
    Off,
    AsusDisabled,
    Unknown,
}

impl FromStr for GfxPower {
    type Err = GfxError;

    fn from_str(s: &str) -> Result<Self, GfxError> {
        match s.to_lowercase().trim() {
            "active" => Ok(GfxPower::Active),
            "suspended" => Ok(GfxPower::Suspended),
            "off" => Ok(GfxPower::Off),
            _ => Ok(GfxPower::Unknown),
        }
    }
}

impl From<&GfxPower> for &str {
    fn from(gfx: &GfxPower) -> &'static str {
        match gfx {
            GfxPower::Active => "active",
            GfxPower::Suspended => "suspended",
            GfxPower::Off => "off",
            GfxPower::AsusDisabled => "dgpu_disabled",
            GfxPower::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Type, PartialEq, Eq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
    AsusDgpuDisabled,
}

impl From<u16> for GfxVendor {
    fn from(vendor: u16) -> Self {
        match vendor {
            0x1002 => GfxVendor::Amd,
            0x10DE => GfxVendor::Nvidia,
            0x8086 => GfxVendor::Intel,
            _ => GfxVendor::Unknown,
        }
    }
}

impl From<&str> for GfxVendor {
    fn from(vendor: &str) -> Self {
        match vendor {
            "0x1002" => GfxVendor::Amd,
            "0x10DE" => GfxVendor::Nvidia,
            "0x8086" => GfxVendor::Intel,
            "1002" => GfxVendor::Amd,
            "10DE" => GfxVendor::Nvidia,
            "8086" => GfxVendor::Intel,
            _ => GfxVendor::Unknown,
        }
    }
}

impl From<GfxVendor> for &str {
    fn from(vendor: GfxVendor) -> Self {
        match vendor {
            GfxVendor::Nvidia => "Nvidia",
            GfxVendor::Amd => "AMD",
            GfxVendor::Intel => "Intel",
            GfxVendor::Unknown => "Unknown",
            GfxVendor::AsusDgpuDisabled => "ASUS dGPU disabled",
        }
    }
}

impl From<&GfxVendor> for &str {
    fn from(vendor: &GfxVendor) -> Self {
        <&str>::from(*vendor)
    }
}

#[derive(Debug, Type, PartialEq, Eq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxMode {
    Hybrid,
    Integrated,
    Compute,
    Vfio,
    Egpu,
    None,
}

impl FromStr for GfxMode {
    type Err = GfxError;

    fn from_str(s: &str) -> Result<Self, GfxError> {
        match s.to_lowercase().trim() {
            "hybrid" => Ok(GfxMode::Hybrid),
            "integrated" => Ok(GfxMode::Integrated),
            "compute" => Ok(GfxMode::Compute),
            "vfio" => Ok(GfxMode::Vfio),
            "egpu" => Ok(GfxMode::Egpu),
            _ => Err(GfxError::ParseVendor),
        }
    }
}

impl From<GfxMode> for &str {
    fn from(gfx: GfxMode) -> &'static str {
        match gfx {
            GfxMode::Hybrid => "hybrid",
            GfxMode::Integrated => "integrated",
            GfxMode::Compute => "compute",
            GfxMode::Vfio => "vfio",
            GfxMode::Egpu => "egpu",
            GfxMode::None => "none",
        }
    }
}

impl From<&GfxMode> for &str {
    fn from(gfx: &GfxMode) -> &'static str {
        (*gfx).into()
    }
}

#[derive(Debug, Type, PartialEq, Eq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxRequiredUserAction {
    Logout,
    Integrated,
    AsusGpuMuxDisable,
    None,
}

impl From<GfxRequiredUserAction> for &str {
    fn from(gfx: GfxRequiredUserAction) -> &'static str {
        match gfx {
            GfxRequiredUserAction::Logout => "logout",
            GfxRequiredUserAction::Integrated => "switch to integrated first",
            GfxRequiredUserAction::None => "none",
            GfxRequiredUserAction::AsusGpuMuxDisable => "The GPU MUX is in Discreet mode, supergfx can not change modes until the MUX is changed back to Optimus mode.",
        }
    }
}

impl From<&GfxRequiredUserAction> for &str {
    fn from(gfx: &GfxRequiredUserAction) -> &'static str {
        (*gfx).into()
    }
}

/// Will rescan the device tree, which adds all removed devices back
pub fn rescan_pci_bus() -> Result<(), GfxError> {
    let path = PathBuf::from(PCI_BUS_PATH).join("rescan");
    write(&path, "1").map_err(|e| GfxError::from_io(e, path))
}

#[derive(Clone, Debug)]
pub struct Device {
    /// Concrete path to the device control
    path: PathBuf,
    vendor: GfxVendor,
    is_dgpu: bool,
    /// System name given by kerne, e.g `0000:01:00.0`
    name: String,
    /// Vendor:Device, typically used only for VFIO setup
    pci_id: String,
}

impl Device {
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn vendor(&self) -> GfxVendor {
        self.vendor
    }

    pub fn is_dgpu(&self) -> bool {
        self.is_dgpu
    }

    pub fn pci_id(&self) -> &str {
        &self.pci_id
    }

    pub fn find() -> Result<Vec<Self>, GfxError> {
        let mut devices = Vec::new();
        let mut parent = String::new();

        let mut enumerator = udev::Enumerator::new().map_err(|err| {
            warn!("{}", err);
            GfxError::Udev("enumerator failed".into(), err)
        })?;

        enumerator.match_subsystem("pci").map_err(|err| {
            warn!("{}", err);
            GfxError::Udev("match_subsystem failed".into(), err)
        })?;

        for device in enumerator.scan_devices().map_err(|err| {
            warn!("{}", err);
            GfxError::Udev("scan_devices failed".into(), err)
        })? {
            let sysname = device.sysname().to_string_lossy();
            debug!("Looking at PCI device {:?}", sysname);
            if let Some(id) = device.property_value("PCI_ID") {
                if let Some(class) = device.property_value("PCI_CLASS") {
                    let id = id.to_string_lossy();
                    let class = class.to_string_lossy();
                    // Match only      Nvidia or AMD
                    if id.starts_with("10DE") || id.starts_with("1002") {
                        if let Some(vendor) = id.split(':').next() {
                            // DGPU CHECK
                            // Assumes that the enumeration is always in order, so things on the same bus after the dGPU
                            // are attached. Look at parent system name to match
                            if let Some(boot_vga) = device.attribute_value("boot_vga") {
                                if class.starts_with("30") && boot_vga == "0" {
                                    info!("Found dgpu {id} at {:?}", device.sysname());
                                    parent = device
                                        .sysname()
                                        .to_string_lossy()
                                        .trim_end_matches(char::is_numeric)
                                        .trim_end_matches('.')
                                        .to_string();
                                    devices.push(Self {
                                        path: PathBuf::from(device.syspath()),
                                        vendor: vendor.into(),
                                        is_dgpu: true,
                                        name: sysname.to_string(),
                                        pci_id: id.to_string(),
                                    });
                                }
                            }
                            // Add next devices only if on same parent as dGPU
                            else if sysname.contains(&parent) {
                                info!("Found additional device {id} at {:?}", device.sysname());
                                devices.push(Self {
                                    path: PathBuf::from(device.syspath()),
                                    vendor: vendor.into(),
                                    is_dgpu: false,
                                    name: sysname.to_string(),
                                    pci_id: id.to_string(),
                                });
                            }
                        }
                    }
                }
            }
            if !parent.is_empty() && !sysname.contains(&parent) {
                break;
            }
        }

        if devices.is_empty() {
            return Err(GfxError::DgpuNotFound);
        }
        Ok(devices)
    }

    /// Read a file underneath the sys object
    fn read_file(path: PathBuf) -> Result<String, GfxError> {
        let path = path.canonicalize()?;
        let mut data = String::new();
        let mut file = fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(|e| GfxError::from_io(e, path.clone()))?;
        debug!("read_file: {file:?}");
        file.read_to_string(&mut data)
            .map_err(|e| GfxError::from_io(e, path))?;

        Ok(data)
    }

    /// Write a file underneath the sys object
    fn write_file(path: PathBuf, data: &[u8]) -> Result<(), GfxError> {
        let path = path.canonicalize()?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| GfxError::from_io(e, path.clone()))?;
        debug!("write_file: {file:?}");
        file.write_all(data.as_ref())
            .map_err(|e| GfxError::from_io(e, path))?;

        Ok(())
    }

    pub fn set_runtime_pm(&self, state: RuntimePowerManagement) -> Result<(), GfxError> {
        let mut path = self.path.clone();
        path.push("power");
        path.push("control");
        debug!("set_runtime_pm: {path:?}");
        Self::write_file(path, <&str>::from(state).as_bytes())
    }

    pub fn get_runtime_status(&self) -> Result<GfxPower, GfxError> {
        let mut path = self.path.clone();
        path.push("power");
        path.push("runtime_status");
        debug!("get_runtime_status: {path:?}");
        match Self::read_file(path) {
            Ok(inner) => GfxPower::from_str(inner.as_str()),
            Err(_) => Ok(GfxPower::Off),
        }
    }

    pub fn driver(&self) -> std::io::Result<PathBuf> {
        fs::canonicalize(self.path.join("driver"))
    }

    pub fn unbind(&self) -> Result<(), GfxError> {
        let mut path = self.driver()?;
        path.push("unbind");
        Self::write_file(path, self.name.as_bytes())
    }

    pub fn remove(&self) -> Result<(), GfxError> {
        let mut path = self.path.clone();
        path.push("remove");
        Self::write_file(path, "1".as_bytes())
    }
}

/// Control whether a device uses, or does not use, runtime power management.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RuntimePowerManagement {
    Auto,
    On,
    Off,
}

impl From<RuntimePowerManagement> for &'static str {
    fn from(pm: RuntimePowerManagement) -> &'static str {
        match pm {
            RuntimePowerManagement::Auto => "auto",
            RuntimePowerManagement::On => "on",
            RuntimePowerManagement::Off => "off",
        }
    }
}

impl From<&str> for RuntimePowerManagement {
    fn from(pm: &str) -> RuntimePowerManagement {
        match pm {
            "auto" => RuntimePowerManagement::Auto,
            "on" => RuntimePowerManagement::On,
            "off" => RuntimePowerManagement::Off,
            _ => RuntimePowerManagement::On,
        }
    }
}

/// Collection of all graphics devices. Functions intend to work on the device
/// determined to be the discreet GPU only.
#[derive(Clone)]
pub struct DiscreetGpu {
    vendor: GfxVendor,
    dgpu_index: usize,
    devices: Vec<Device>,
}

impl DiscreetGpu {
    pub fn new() -> Result<DiscreetGpu, GfxError> {
        // first need to check asus specific paths

        info!("Rescanning PCI bus");
        rescan_pci_bus()?;

        if let Ok(device) = Device::find() {
            let mut vendor = GfxVendor::Unknown;
            let mut dgpu_index = 0;
            for dev in device.iter().enumerate() {
                if dev.1.is_dgpu() {
                    dgpu_index = dev.0;
                    vendor = dev.1.vendor();
                }
            }
            Ok(Self {
                vendor,
                dgpu_index,
                devices: device,
            })
        } else {
            let mut vendor = GfxVendor::Unknown;
            if asus_dgpu_exists() {
                warn!("ASUS dGPU appears to be disabled");
                vendor = GfxVendor::AsusDgpuDisabled;
            }
            Ok(Self {
                vendor,
                dgpu_index: 0,
                devices: Vec::new(),
            })
        }
    }

    pub fn vendor(&self) -> GfxVendor {
        self.vendor
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    pub fn is_nvidia(&self) -> bool {
        self.vendor == GfxVendor::Nvidia
    }

    pub fn is_amd(&self) -> bool {
        self.vendor == GfxVendor::Amd
    }

    pub fn is_intel(&self) -> bool {
        self.vendor == GfxVendor::Intel
    }

    pub fn get_runtime_status(&self) -> Result<GfxPower, GfxError> {
        if !self.devices.is_empty() {
            debug!("get_runtime_status: {:?}", self.devices[self.dgpu_index]);
            if self.vendor != GfxVendor::Unknown {
                return self.devices[self.dgpu_index].get_runtime_status();
            }
            if self.vendor == GfxVendor::AsusDgpuDisabled {
                return Ok(GfxPower::AsusDisabled);
            }
        }
        Err(GfxError::NotSupported("Could not find dGPU".to_string()))
    }

    pub fn set_runtime_pm(&self, pm: RuntimePowerManagement) -> Result<(), GfxError> {
        debug!("set_runtime_pm: pm = {:?}, {:?}", pm, self.devices);
        if !matches!(
            self.vendor,
            GfxVendor::Unknown | GfxVendor::AsusDgpuDisabled
        ) {
            for dev in self.devices.iter() {
                dev.set_runtime_pm(pm)?;
                info!("Set PM on {:?} to {pm:?}", dev.path());
            }
            return Ok(());
        }
        if self.vendor == GfxVendor::AsusDgpuDisabled {
            return Ok(());
        }
        Err(GfxError::NotSupported("Could not find dGPU".to_string()))
    }

    pub fn unbind(&self) -> Result<(), GfxError> {
        debug!("unbind: {:?}", self.devices);
        if self.vendor != GfxVendor::Unknown {
            for dev in self.devices.iter() {
                dev.unbind()?;
                info!("Unbound {:?}", dev.path())
            }
            return Ok(());
        }
        if self.vendor == GfxVendor::AsusDgpuDisabled {
            return Ok(());
        }
        Err(GfxError::NotSupported("Could not find dGPU".to_string()))
    }

    pub fn remove(&self) -> Result<(), GfxError> {
        debug!("remove: {:?}", self.devices);
        if self.vendor != GfxVendor::Unknown {
            for dev in self.devices.iter() {
                dev.remove()?;
            }
            return Ok(());
        }
        Err(GfxError::NotSupported("Could not find dGPU".to_string()))
    }

    pub fn unbind_remove(&self) -> Result<(), GfxError> {
        self.unbind()?;
        self.remove()
    }

    pub fn do_driver_action(&self, action: &str) -> Result<(), GfxError> {
        debug!("do_driver_action: action = {}, {:?}", action, self.devices);
        if self.is_nvidia() {
            for driver in NVIDIA_DRIVERS.iter() {
                do_driver_action(driver, action)?;
            }
        }
        Ok(())
    }
}
