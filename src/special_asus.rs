use log::{debug, error, info, warn};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use tokio::time::sleep;

use crate::{
    error::GfxError,
    pci_device::{rescan_pci_bus, GfxMode},
};

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

pub fn asus_gpu_mux_exists() -> bool {
    Path::new(ASUS_GPU_MUX_PATH).exists()
}

pub fn asus_gpu_mux_mode() -> Result<AsusGpuMuxMode, GfxError> {
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

pub fn asus_gpu_mux_set_igpu(igpu_on: bool) -> Result<(), GfxError> {
    debug!("asus_gpu_mux_set_igpu: {igpu_on}");
    asus_gpu_toggle(igpu_on, ASUS_GPU_MUX_PATH)?;
    debug!("asus_gpu_mux_set_igpu: success");
    Ok(())
}

pub fn asus_dgpu_disable_exists() -> bool {
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
    // Do not try to set it again if it has already been changed
    if asus_dgpu_disabled()? == disabled {
        debug!("asus_dgpu_set_disabled: already set to {disabled}. Early return");
        return Ok(());
    }
    debug!("asus_dgpu_set_disabled: {disabled}");
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
    debug!("asus_dgpu_set_disabled: success");
    Ok(())
}

pub fn asus_egpu_enable_exists() -> bool {
    if Path::new(ASUS_EGPU_ENABLE_PATH).exists() {
        return true;
    }
    false
}

pub fn asus_egpu_enabled() -> Result<bool, GfxError> {
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

/// Special ASUS only feature. On toggle to `on` it will rescan the PCI bus.
pub fn asus_egpu_set_enabled(enabled: bool) -> Result<(), GfxError> {
    if asus_egpu_enabled()? {
        // Do not try to set it again if it has already been changedif asus_egpu_enabled()? {
        return Ok(());
    }
    debug!("asus_egpu_set_enabled: {enabled}");
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
    debug!("asus_egpu_set_enabled: success");
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

/// To be called in main reload code. Specific actions required for asus laptops depending
/// on is dgpu_disable, egpu_enable, or gpu_mux_mode are available.
///
/// The returned mode may be different to the requested mode depending on the bios settings active,
/// the differing value *must* be used.
pub async fn asus_boot_safety_check(
    mode: GfxMode,
    asus_use_dgpu_disable: bool,
) -> Result<GfxMode, GfxError> {
    debug!("asus_reload: asus_use_dgpu_disable: {asus_use_dgpu_disable}");
    // This is a bit of a crap cycle to ensure that dgpu_disable is there before setting it.
    if asus_use_dgpu_disable && !asus_dgpu_disable_exists() {
        if !create_asus_modules_load_conf()? {
            warn!(
                "asus_boot_safety_check: Reboot required due to {} creation",
                ASUS_MODULES_LOAD_PATH
            );
            // let mut cmd = Command::new("reboot");
            // cmd.spawn()?;
        }
        warn!("asus_boot_safety_check: HotPlug type Asus is set but asus-wmi appear not loaded yet. Trying for 2 seconds. If there are issues you may need to add asus_nb_wmi to modules.load.d");
        let mut count = 2000 / 50;
        while !asus_dgpu_disable_exists() && count != 0 {
            sleep(Duration::from_millis(50)).await;
            count -= 1;
        }
    }

    if asus_gpu_mux_exists() {
        match asus_gpu_mux_mode()? {
            AsusGpuMuxMode::Discreet => {
                if asus_dgpu_disable_exists() && asus_dgpu_disabled()? {
                    error!("asus_boot_safety_check: dgpu_disable is on while gpu_mux_mode is descrete, can't continue safely, attempting to set dgpu_disable off");
                    asus_dgpu_set_disabled(false)?;
                } else {
                    info!("asus_boot_safety_check: dgpu_disable is off");
                }
                return Ok(GfxMode::AsusMuxDgpu);
            }
            AsusGpuMuxMode::Optimus => {
                if mode == GfxMode::AsusMuxDgpu {
                    warn!("asus_boot_safety_check: MUX is in Optimus mode but mode is set to AsusMuxDgpu. Switching to Hybrid");
                    return Ok(GfxMode::Hybrid);
                }
            }
        }
    }

    // Need to always check if dgpu_disable exists since GA401I series and older doesn't have this
    if asus_dgpu_disable_exists() {
        let dgpu_disabled = asus_dgpu_disabled()?;
        // If dgpu_disable is hard set then users won't have a dgpu at all, try set dgpu enabled
        if !asus_use_dgpu_disable && dgpu_disabled {
            warn!("It appears dgpu_disable is true on boot with HotPlug type not set to Asus, will attempt to re-enable dgpu");
            asus_dgpu_set_disabled(false)
                .map_err(|e| error!("asus_dgpu_set_disabled: {e:?}"))
                .ok();
        } else if dgpu_disabled && mode != GfxMode::Integrated {
            warn!("asus_boot_safety_check: dgpu_disable is on but the mode isn't Integrated, setting mode to Integrated");
            return Ok(GfxMode::Integrated);
        }
    }

    if asus_egpu_enable_exists() && asus_egpu_enabled()? && mode != GfxMode::AsusEgpu {
        warn!("asus_boot_safety_check: egpu_enable is on but the mode isn't AsusEgpu, setting mode to AsusEgpu");
        return Ok(GfxMode::AsusEgpu);
    }

    Ok(mode)
}
