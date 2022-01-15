use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
};

use crate::{do_driver_action, error::GfxError, pci_device::rescan_pci_bus, NVIDIA_DRIVERS};

static ASUS_DGPU_DISABLE_PATH: &str = "/sys/devices/platform/asus-nb-wmi/dgpu_disable";
static ASUS_EGPU_ENABLE_PATH: &str = "/sys/devices/platform/asus-nb-wmi/egpu_enable";

static ASUS_SWITCH_GRAPHIC_MODE: &str =
    "/sys/firmware/efi/efivars/AsusSwitchGraphicMode-607005d5-3f75-4b2e-98f0-85ba66797a3e";

pub fn has_asus_gsync_gfx_mode() -> bool {
    Path::new(ASUS_SWITCH_GRAPHIC_MODE).exists()
}

pub fn get_asus_gsync_gfx_mode() -> Result<i8, GfxError> {
    let path = ASUS_SWITCH_GRAPHIC_MODE;
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|err| GfxError::Path(path.into(), err))?;

    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|err| GfxError::Read(path.into(), err))?;

    let idx = data.len() - 1;
    Ok(data[idx] as i8)
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

pub(crate) fn asus_egpu_exists() -> bool {
    if Path::new(ASUS_EGPU_ENABLE_PATH).exists() {
        return true;
    }
    false
}

pub(crate) fn asus_egpu_enabled() -> Result<bool, GfxError> {
    let path = Path::new(ASUS_EGPU_ENABLE_PATH);
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|err| GfxError::Path(ASUS_EGPU_ENABLE_PATH.to_string(), err))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    if buf.contains('1') {
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn asus_egpu_set_status(status: bool) -> Result<(), GfxError> {
    // toggling from egpu must have the nvidia driver unloaded
    for driver in NVIDIA_DRIVERS.iter() {
        do_driver_action(driver, "rmmod")?;
    }
    // Need to set, scan, set to ensure mode is correctly set
    asus_egpu_toggle(status)?;
    rescan_pci_bus()?;
    asus_egpu_toggle(status)?;
    Ok(())
}

fn asus_egpu_toggle(status: bool) -> Result<(), GfxError> {
    let path = Path::new(ASUS_EGPU_ENABLE_PATH);
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|err| GfxError::Path(ASUS_EGPU_ENABLE_PATH.to_string(), err))?;
    let status = if status { 1 } else { 0 };
    file.write_all(status.to_string().as_bytes())
        .map_err(|err| GfxError::Write(ASUS_EGPU_ENABLE_PATH.to_string(), err))?;
    Ok(())
}

pub(crate) fn is_gpu_enabled() -> Result<bool, GfxError> {
    if asus_dgpu_exists() {
        if asus_dgpu_disabled()? {
            if asus_egpu_exists() {
                if !asus_egpu_enabled()? {
                    return Err(GfxError::NotSupported("dGPU and eGPU disabled".to_string()));
                }
            } else {
                return Err(GfxError::NotSupported("dGPU disabled".to_string()));
            }
        }
    }
    Ok(true)
}
