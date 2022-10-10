use log::{debug, info, warn};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::process::Command;
use std::str::FromStr;
use std::{fs::write, path::PathBuf};

use crate::error::GfxError;
use crate::special_asus::{
    asus_dgpu_disabled, asus_dgpu_exists, get_asus_gpu_mux_mode, has_asus_gpu_mux, AsusGpuMuxMode,
};
use crate::{do_driver_action, find_slot_power, NVIDIA_DRIVERS};

use serde_derive::{Deserialize, Serialize};
use zvariant_derive::Type;

const PCI_BUS_PATH: &str = "/sys/bus/pci";

#[derive(Debug, Type, PartialEq, Eq, Copy, Clone)]
pub enum HotplugState {
    On,
    Off,
}

impl FromStr for HotplugState {
    type Err = GfxError;

    fn from_str(s: &str) -> Result<Self, GfxError> {
        match s.to_lowercase().trim() {
            "1" => Ok(Self::On),
            _ => Ok(Self::Off),
        }
    }
}

impl From<&HotplugState> for &str {
    fn from(gfx: &HotplugState) -> &'static str {
        match gfx {
            HotplugState::On => "1",
            HotplugState::Off => "0",
        }
    }
}

#[derive(Debug, Type, PartialEq, Eq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxPower {
    Active,
    Suspended,
    Off,
    AsusDisabled,
    AsusMuxDiscreet,
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
            GfxPower::AsusMuxDiscreet => "asus_mux_discreet",
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
    AsusMuxDiscreet,
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
            GfxMode::AsusMuxDiscreet => "asus_mux_discreet",
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

fn lscpi(vendor_device: &str) -> Result<String, GfxError> {
    let mut cmd = Command::new("lspci");
    cmd.args(["-d", vendor_device]);
    let s = String::from_utf8_lossy(&cmd.output()?.stdout).into_owned();
    Ok(s)
}

pub fn lscpi_dgpu_check(label: &str) -> bool {
    for pat in ["Radeon RX", "AMD/ATI", "GeForce", "Geforce", "Quadro"] {
        if label.contains(pat) {
            return true;
        }
    }
    false
}

#[derive(Clone, Debug)]
pub struct Device {
    /// Concrete path to the device control
    dev_path: PathBuf,
    /// Concrete path to the slot this device is in for hotplug support
    hotplug_path: Option<PathBuf>,
    vendor: GfxVendor,
    is_dgpu: bool,
    /// System name given by kerne, e.g `0000:01:00.0`
    name: String,
    /// Vendor:Device, typically used only for VFIO setup
    pci_id: String,
}

impl Device {
    pub fn dev_path(&self) -> &PathBuf {
        &self.dev_path
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

    fn set_hotplug(&self, state: HotplugState) -> Result<(), GfxError> {
        if let Some(path) = self.hotplug_path.as_ref() {
            info!("set_hotplug: Setting hotplug power to {state:?}");
            let mut file = OpenOptions::new()
                .write(true)
                .open(path)
                .map_err(|err| GfxError::Path(path.to_string_lossy().to_string(), err))?;

            file.write_all(<&str>::from(&state).as_bytes())
                .map_err(|err| GfxError::Write(path.to_string_lossy().to_string(), err))?;
        }
        Ok(())
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

        let get_parent = |dev: &udev::Device| -> String {
            dev.sysname()
                .to_string_lossy()
                .trim_end_matches(char::is_numeric)
                .trim_end_matches('.')
                .to_string()
        };

        for device in enumerator.scan_devices().map_err(|err| {
            warn!("{}", err);
            GfxError::Udev("scan_devices failed".into(), err)
        })? {
            let sysname = device.sysname().to_string_lossy();
            debug!("Looking at PCI device {:?}", sysname);
            // PCI_ID can be given directly to lspci to get a database label
            // This is the same as ID_MODEL_FROM_DATABASE
            if let Some(id) = device.property_value("PCI_ID") {
                if let Some(class) = device.property_value("PCI_CLASS") {
                    let id = id.to_string_lossy();
                    // class can be 0x030200 or 0x030000
                    let class = class.to_string_lossy();
                    // Match only      Nvidia or AMD
                    if id.starts_with("10DE") || id.starts_with("1002") {
                        if let Some(vendor) = id.split(':').next() {
                            // DGPU CHECK
                            // Assumes that the enumeration is always in order, so things on the same bus after the dGPU
                            // are attached. Look at parent system name to match
                            let dgpu = if let Some(boot_vga) = device.attribute_value("boot_vga") {
                                class.starts_with("30") && boot_vga == "0"
                            } else if let Some(label) =
                                device.property_value("ID_MODEL_FROM_DATABASE")
                            {
                                lscpi_dgpu_check(&label.to_string_lossy())
                            } else {
                                // last resort - this is typically only required if ID_MODEL_FROM_DATABASE is
                                // missing due to dgpu_disable being on at boot
                                lscpi_dgpu_check(&lscpi(&id)?)
                            };

                            if dgpu || !parent.is_empty() && sysname.contains(&parent) {
                                let mut hotplug_path = None;
                                if dgpu {
                                    info!("Found dgpu {id} at {:?}", device.sysname());
                                    match find_slot_power(&sysname) {
                                        Ok(slot) => hotplug_path = Some(slot),
                                        Err(e) => {
                                            debug!("Laptop does not have a hotplug dgpu: {e:?}")
                                        }
                                    }
                                } else {
                                    info!("Found additional device {id} at {:?}", device.sysname());
                                }
                                parent = get_parent(&device);
                                devices.push(Self {
                                    dev_path: PathBuf::from(device.syspath()),
                                    hotplug_path,
                                    vendor: vendor.into(),
                                    is_dgpu: dgpu,
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
        let mut path = self.dev_path.clone();
        path.push("power");
        path.push("control");
        if path.exists() {
            debug!("set_runtime_pm: {path:?}");
            Self::write_file(path, <&str>::from(state).as_bytes())?;
        } else {
            debug!("set_runtime_pm: {path:?} doesn't exist, device may have been removed (can be ignored)");
        }
        Ok(())
    }

    pub fn get_runtime_status(&self) -> Result<GfxPower, GfxError> {
        let mut path = self.dev_path.clone();
        path.push("power");
        path.push("runtime_status");
        debug!("get_runtime_status: {path:?}");
        match Self::read_file(path) {
            Ok(inner) => GfxPower::from_str(inner.as_str()),
            Err(_) => Ok(GfxPower::Off),
        }
    }

    pub fn driver(&self) -> std::io::Result<PathBuf> {
        fs::canonicalize(self.dev_path.join("driver"))
    }

    pub fn unbind(&self) -> Result<(), GfxError> {
        if let Ok(mut path) = self.driver() {
            if path.exists() {
                path.push("unbind");
                return Self::write_file(path, self.name.as_bytes());
            }
        }
        info!(
            "unbind path {:?} did not exist, driver unloaded?",
            self.dev_path
        );
        Ok(())
    }

    pub fn remove(&self) -> Result<(), GfxError> {
        if self.dev_path.exists() {
            let mut path = self.dev_path.clone();
            path.push("remove");
            return Self::write_file(path, "1".as_bytes());
        }
        info!(
            "remove path {:?} did not exist, device removed already?",
            self.dev_path
        );
        Ok(())
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
        info!("DiscreetGpu::new: Rescanning PCI bus");
        rescan_pci_bus()?;

        if let Ok(device) = Device::find() {
            let mut vendor = GfxVendor::Unknown;
            let mut dgpu_index = 0;
            for (idx, dev) in device.iter().enumerate() {
                if dev.is_dgpu() {
                    dgpu_index = idx;
                    vendor = dev.vendor();
                }
            }
            Ok(Self {
                vendor,
                dgpu_index,
                devices: device,
            })
        } else {
            warn!("DiscreetGpu::new: no devices??");
            let mut vendor = GfxVendor::Unknown;
            if asus_dgpu_exists()
                && if let Ok(c) = asus_dgpu_disabled() {
                    c
                } else {
                    false
                }
            {
                warn!("ASUS dGPU appears to be disabled");
                vendor = GfxVendor::AsusDgpuDisabled;
            } else if has_asus_gpu_mux()
                && if let Ok(c) = get_asus_gpu_mux_mode() {
                    c == AsusGpuMuxMode::Discreet
                } else {
                    false
                }
            {
                warn!("ASUS GPU MUX is in discreet mode");
                vendor = GfxVendor::Nvidia;
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
            if self.vendor == GfxVendor::AsusDgpuDisabled {
                warn!("ASUS dgpu status: {:?}", self.vendor);
                return Ok(GfxPower::AsusDisabled);
            } else if self.vendor != GfxVendor::Unknown {
                return self.devices[self.dgpu_index].get_runtime_status();
            }
        } else if let Ok(disabled) = asus_dgpu_disabled() {
            debug!("No dGPU tracked. Maybe booted with dgpu_disable set via Windows");
            info!("Is ASUS laptop, dgpu_disable = {disabled}");
            if disabled {
                return Ok(GfxPower::AsusDisabled);
            }
        }
        Err(GfxError::NotSupported(
            "get_runtime_status: Could not find dGPU".to_string(),
        ))
    }

    pub fn set_runtime_pm(&self, pm: RuntimePowerManagement) -> Result<(), GfxError> {
        debug!("set_runtime_pm: pm = {:?}, {:?}", pm, self.devices);
        if self.devices.is_empty() {
            warn!("set_runtime_pm: Did not have dGPU handle");
            return Ok(());
        }
        if !matches!(
            self.vendor,
            GfxVendor::Unknown | GfxVendor::AsusDgpuDisabled
        ) {
            for dev in self.devices.iter() {
                dev.set_runtime_pm(pm)?;
                info!("set_runtime_pm: Set PM on {:?} to {pm:?}", dev.dev_path());
            }
            return Ok(());
        }
        if self.vendor == GfxVendor::AsusDgpuDisabled {
            info!("set_runtime_pm: ASUS dgpu_disable set, ignoring");
            return Ok(());
        }
        Err(GfxError::NotSupported(
            "set_runtime_pm: Could not find dGPU".to_string(),
        ))
    }

    pub fn set_hotplug(&self, state: HotplugState) -> Result<(), GfxError> {
        for dev in self.devices.iter() {
            if dev.is_dgpu() {
                dev.set_hotplug(state)?;
                break;
            }
        }
        Ok(())
    }

    pub fn unbind(&self) -> Result<(), GfxError> {
        if self.vendor != GfxVendor::Unknown {
            for dev in self.devices.iter().rev() {
                dev.unbind()?;
                info!("Unbound {:?}", dev.dev_path())
            }
            return Ok(());
        }
        if self.vendor == GfxVendor::AsusDgpuDisabled {
            return Ok(());
        }
        Err(GfxError::NotSupported(
            "unbind: Could not find dGPU".to_string(),
        ))
    }

    pub fn remove(&self) -> Result<(), GfxError> {
        if self.vendor != GfxVendor::Unknown {
            for dev in self.devices.iter().rev() {
                dev.remove()?;
                info!("Removed {:?}", dev.dev_path())
            }
            return Ok(());
        }
        Err(GfxError::NotSupported(
            "remove: Could not find dGPU".to_string(),
        ))
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
