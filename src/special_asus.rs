use log::debug;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
};

use crate::{do_driver_action, error::GfxError, pci_device::rescan_pci_bus, NVIDIA_DRIVERS};

static ASUS_DGPU_DISABLE_PATH: &str = "/sys/devices/platform/asus-nb-wmi/dgpu_disable";
static ASUS_EGPU_ENABLE_PATH: &str = "/sys/devices/platform/asus-nb-wmi/egpu_enable";
static ASUS_GPU_MUX_PATH: &str = "/sys/devices/platform/asus-nb-wmi/gpu_mux_mode";

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub enum AsusGpuMuxMode {
    Dedicated,
    Optimus,
}

impl From<i8> for AsusGpuMuxMode {
    fn from(v: i8) -> Self {
        if v == 0 {
            return Self::Dedicated;
        }
        Self::Optimus
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
    file.read_to_end(&mut data)
        .map_err(|err| GfxError::Read(path.into(), err))?;

    let idx = data.len() - 1;
    Ok((data[idx] as i8).into())
}

pub(crate) fn asus_dgpu_exists() -> bool {
    if Path::new(ASUS_DGPU_DISABLE_PATH).exists() {
        return true;
    }
    false
}

pub(crate) fn asus_dgpu_disabled() -> Result<bool, GfxError> {
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
pub(crate) fn asus_dgpu_set_disabled(disabled: bool) -> Result<(), GfxError> {
    if disabled {
        for driver in NVIDIA_DRIVERS.iter() {
            debug!("dgpu_disable set, ensuring nvidia drivers removed");
            do_driver_action(driver, "rmmod")?;
        }
    }
    // Need to set, scan, set to ensure mode is correctly set
    asus_gpu_toggle(disabled, ASUS_DGPU_DISABLE_PATH)?;
    if !disabled {
        rescan_pci_bus()?;
        asus_gpu_toggle(disabled, ASUS_DGPU_DISABLE_PATH)?;
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
    // toggling from egpu must have the nvidia driver unloaded
    for driver in NVIDIA_DRIVERS.iter() {
        debug!("egpu_enable unset, ensuring nvidia drivers removed");
        do_driver_action(driver, "rmmod")?;
    }
    // Need to set, scan, set to ensure mode is correctly set
    asus_gpu_toggle(enabled, ASUS_EGPU_ENABLE_PATH)?;
    if enabled {
        rescan_pci_bus()?;
        asus_gpu_toggle(enabled, ASUS_EGPU_ENABLE_PATH)?;
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
