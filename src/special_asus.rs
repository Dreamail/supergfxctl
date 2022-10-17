use log::{debug, info};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

use crate::{error::GfxError, pci_device::rescan_pci_bus};

const ASUS_DGPU_DISABLE_PATH: &str = "/sys/devices/platform/asus-nb-wmi/dgpu_disable";
const ASUS_EGPU_ENABLE_PATH: &str = "/sys/devices/platform/asus-nb-wmi/egpu_enable";
const ASUS_GPU_MUX_PATH: &str = "/sys/devices/platform/asus-nb-wmi/gpu_mux_mode";

pub const ASUS_MODULES_LOAD_PATH: &str = "/etc/modules-load.d/asus.conf";
pub const ASUS_MODULES_LOAD: &[u8] = br#"
asus-wmi
asus-nb-wmi
"#;

/// Create the config. Returns true if it already existed.
pub fn create_asus_modules_load_conf() -> Result<bool, GfxError> {
    if Path::new(ASUS_MODULES_LOAD_PATH).exists() {
        info!("{} exists", ASUS_MODULES_LOAD_PATH);
        return Ok(true);
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(ASUS_MODULES_LOAD_PATH)
        .map_err(|err| GfxError::Path(ASUS_MODULES_LOAD_PATH.into(), err))?;

    info!("Writing {}", ASUS_MODULES_LOAD_PATH);
    file.write_all(ASUS_MODULES_LOAD)
        .and_then(|_| file.sync_all())
        .map_err(|err| GfxError::Write(ASUS_MODULES_LOAD_PATH.into(), err))?;

    Ok(false)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub enum AsusGpuMuxMode {
    Discreet,
    Optimus,
}

impl From<i8> for AsusGpuMuxMode {
    fn from(v: i8) -> Self {
        if v != 0 {
            return Self::Optimus;
        }
        Self::Discreet
    }
}

impl From<char> for AsusGpuMuxMode {
    fn from(v: char) -> Self {
        if v != '0' {
            return Self::Optimus;
        }
        Self::Discreet
    }
}

pub fn has_asus_gpu_mux() -> bool {
    Path::new(ASUS_GPU_MUX_PATH).exists()
}

pub fn get_asus_gpu_mux_mode() -> Result<AsusGpuMuxMode, GfxError> {
    let path = ASUS_GPU_MUX_PATH;
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|err| GfxError::Path(path.into(), err))?;

    let mut data = Vec::new();
    let res = file
        .read_to_end(&mut data)
        .map_err(|err| GfxError::Read(path.into(), err))?;
    if res == 0 {
        return Err(GfxError::Read(
            "Failed to read gpu_mux_mode".to_owned(),
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Could not read"),
        ));
    }

    if let Some(d) = (data[0] as char).to_digit(10) {
        return Ok(AsusGpuMuxMode::from(d as i8));
    }
    Err(GfxError::Read(
        "Failed to read gpu_mux_mode".to_owned(),
        std::io::Error::new(std::io::ErrorKind::InvalidData, "Could not read"),
    ))
}

pub fn asus_dgpu_exists() -> bool {
    if Path::new(ASUS_DGPU_DISABLE_PATH).exists() {
        return true;
    }
    false
}

pub fn asus_dgpu_disabled() -> Result<bool, GfxError> {
    let path = Path::new(ASUS_DGPU_DISABLE_PATH);
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|err| GfxError::Path(ASUS_DGPU_DISABLE_PATH.to_string(), err))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    if buf.contains('1') {
        return Ok(true);
    }
    Ok(false)
}

/// Special ASUS only feature. On toggle to `off` it will rescan the PCI bus.
pub fn asus_dgpu_set_disabled(disabled: bool) -> Result<(), GfxError> {
    // There is a sleep here because this function is generally called after a hotplug
    // enable, and the deivces require at least a touch of time to finish powering up/down
    std::thread::sleep(Duration::from_millis(500));
    // Need to set, scan, set to ensure mode is correctly set
    asus_gpu_toggle(disabled, ASUS_DGPU_DISABLE_PATH)?;
    if !disabled {
        // Purposefully blocking here. Need to force enough time for things to wake
        std::thread::sleep(Duration::from_millis(50));
        rescan_pci_bus()?;
    }
    Ok(())
}

pub(crate) fn asus_egpu_exists() -> bool {
    if Path::new(ASUS_EGPU_ENABLE_PATH).exists() {
        return true;
    }
    false
}

/// Special ASUS only feature. On toggle to `on` it will rescan the PCI bus.
pub(crate) fn asus_egpu_set_enabled(enabled: bool) -> Result<(), GfxError> {
    // There is a sleep here because this function is generally called after a hotplug
    // enable, and the deivces require at least a touch of time to finish powering up
    std::thread::sleep(Duration::from_millis(500));
    // Need to set, scan, set to ensure mode is correctly set
    asus_gpu_toggle(enabled, ASUS_EGPU_ENABLE_PATH)?;
    if enabled {
        // Purposefully blocking here. Need to force enough time for things to wake
        std::thread::sleep(Duration::from_millis(50));
        rescan_pci_bus()?;
    }
    Ok(())
}

fn asus_gpu_toggle(status: bool, path: &str) -> Result<(), GfxError> {
    let pathbuf = Path::new(path);
    let mut file = OpenOptions::new()
        .write(true)
        .open(pathbuf)
        .map_err(|err| GfxError::Path(path.to_string(), err))?;
    let status = if status { 1 } else { 0 };
    file.write_all(status.to_string().as_bytes())
        .map_err(|err| GfxError::Write(path.to_string(), err))?;
    debug!("switched {path} to {status}");
    Ok(())
}
