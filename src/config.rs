use log::{error, info, warn};
use serde_derive::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use zvariant_derive::Type;

use crate::config_old::{GfxConfig300, GfxConfig405};
use crate::error::GfxError;
use crate::pci_device::{DiscreetGpu, GfxMode, GfxRequiredUserAction};
use crate::special_asus::asus_dgpu_exists;
use crate::{
    MODPROBE_INTEGRATED, MODPROBE_NVIDIA_BASE, MODPROBE_NVIDIA_DRM_MODESET, MODPROBE_PATH,
    MODPROBE_VFIO,
};

/// Cleaned config for passing over dbus only
#[derive(Debug, Clone, Deserialize, Serialize, Type)]
pub struct GfxConfigDbus {
    pub mode: GfxMode,
    pub vfio_enable: bool,
    pub vfio_save: bool,
    pub compute_save: bool,
    pub always_reboot: bool,
    pub no_logind: bool,
    pub logout_timeout_s: u64,
    pub asus_use_dgpu_enable: bool,
}

impl From<&GfxConfig> for GfxConfigDbus {
    fn from(c: &GfxConfig) -> Self {
        Self {
            mode: c.mode,
            vfio_enable: c.vfio_enable,
            vfio_save: c.vfio_save,
            compute_save: c.compute_save,
            always_reboot: c.always_reboot,
            no_logind: c.no_logind,
            logout_timeout_s: c.logout_timeout_s,
            asus_use_dgpu_enable: c.asus_use_dgpu_disable,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GfxConfig {
    #[serde(skip)]
    pub config_path: String,
    /// The current mode set, also applies on boot
    pub mode: GfxMode,
    /// Only for temporary modes like compute or vfio
    #[serde(skip)]
    pub tmp_mode: Option<GfxMode>,
    /// Just for tracking the requested mode change in rebootless mode
    #[serde(skip)]
    pub pending_mode: Option<GfxMode>,
    /// Just for tracking the required user action
    #[serde(skip)]
    pub pending_action: Option<GfxRequiredUserAction>,
    /// Set if vfio option is enabled. This requires the vfio drivers to be built as modules
    pub vfio_enable: bool,
    /// Save the VFIO mode so that it is reloaded on boot
    pub vfio_save: bool,
    /// Save the Compute mode so that it is reloaded on boot
    pub compute_save: bool,
    /// Should always reboot?
    pub always_reboot: bool,
    /// Don't use logind to see if all sessions are logged out and therefore safe to change mode
    pub no_logind: bool,
    /// The timeout in seconds to wait for all user graphical sessions to end. Default is 3 minutes, 0 = infinite. Ignored if `no_logind` or `always_reboot` is set.
    pub logout_timeout_s: u64,
    /// Specific to ASUS ROG/TUF laptops
    pub asus_use_dgpu_disable: bool,
}

impl GfxConfig {
    fn new(config_path: String) -> Self {
        Self {
            config_path,
            mode: GfxMode::Hybrid,
            tmp_mode: None,
            pending_mode: None,
            pending_action: None,
            vfio_enable: false,
            vfio_save: false,
            compute_save: false,
            always_reboot: false,
            no_logind: false,
            logout_timeout_s: 180,
            asus_use_dgpu_disable: asus_dgpu_exists(),
        }
    }

    /// `load` will attempt to read the config, and panic if the dir is missing
    pub fn load(config_path: String) -> Self {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&config_path)
            .unwrap_or_else(|_| panic!("The directory {} is missing", config_path)); // okay to cause panic here
        let mut buf = String::new();
        let mut config;
        if let Ok(read_len) = file.read_to_string(&mut buf) {
            if read_len == 0 {
                config = Self::new(config_path);
            } else if let Ok(data) = serde_json::from_str(&buf) {
                config = data;
                config.config_path = config_path;
            } else if let Ok(data) = serde_json::from_str(&buf) {
                let old: GfxConfig300 = data;
                config = old.into();
                config.config_path = config_path;
            } else if let Ok(data) = serde_json::from_str(&buf) {
                let old: GfxConfig405 = data;
                config = old.into();
                config.config_path = config_path;
            } else {
                warn!("Could not deserialise {}, recreating", config_path);
                config = GfxConfig::new(config_path);
            }
        } else {
            config = Self::new(config_path)
        }
        config.write();
        config
    }

    pub fn read(&mut self) {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&self.config_path)
            .unwrap_or_else(|err| panic!("Error reading {}: {}", self.config_path, err));
        let mut buf = String::new();
        if let Ok(l) = file.read_to_string(&mut buf) {
            if l == 0 {
                warn!("File is empty {}", self.config_path);
            } else {
                let mut x: Self = serde_json::from_str(&buf)
                    .unwrap_or_else(|_| panic!("Could not deserialise {}", self.config_path));
                // copy over serde skipped values
                x.tmp_mode = self.tmp_mode;
                *self = x;
            }
        }
    }

    pub fn write(&self) {
        let mut file = File::create(&self.config_path).expect("Couldn't overwrite config");
        let json = serde_json::to_string_pretty(self).expect("Parse config to JSON failed");
        file.write_all(json.as_bytes())
            .unwrap_or_else(|err| error!("Could not write config: {}", err));
    }
}

/// Creates the full modprobe.conf required for vfio pass-through
fn create_vfio_conf(devices: &DiscreetGpu) -> Vec<u8> {
    let mut vifo = MODPROBE_VFIO.to_vec();
    for (f_count, func) in devices.devices().iter().enumerate() {
        unsafe {
            vifo.append(func.pci_id().to_owned().as_mut_vec());
        }
        if f_count < devices.devices().len() - 1 {
            vifo.append(&mut vec![b',']);
        }
    }
    vifo.append(&mut vec![b',']);

    let mut conf = MODPROBE_INTEGRATED.to_vec();
    conf.append(&mut vifo);
    conf
}

pub(crate) fn create_modprobe_conf(mode: GfxMode, devices: &DiscreetGpu) -> Result<(), GfxError> {
    info!("Writing {}", MODPROBE_PATH);
    let content = match mode {
        GfxMode::Integrated | GfxMode::Hybrid | GfxMode::Egpu => {
            if devices.is_nvidia() {
                let mut base = MODPROBE_NVIDIA_BASE.to_vec();
                base.append(&mut MODPROBE_NVIDIA_DRM_MODESET.to_vec());
                base
            } else if devices.is_amd() {
                return Ok(());
            } else {
                warn!("No valid modprobe config for device");
                return Ok(());
            }
        }
        GfxMode::Vfio => create_vfio_conf(devices),
        GfxMode::Compute => MODPROBE_NVIDIA_BASE.to_vec(),
        GfxMode::None | GfxMode::AsusMuxDiscreet => vec![],
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(MODPROBE_PATH)
        .map_err(|err| GfxError::Path(MODPROBE_PATH.into(), err))?;

    file.write_all(&content)
        .and_then(|_| file.sync_all())
        .map_err(|err| GfxError::Write(MODPROBE_PATH.into(), err))?;

    Ok(())
}
