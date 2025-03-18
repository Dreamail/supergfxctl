use log::{error, info, warn};
use serde_derive::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use zbus::zvariant::Type;

use crate::actions::UserActionRequired;
use crate::config_old::{GfxConfig300, GfxConfig405, GfxConfig500};
use crate::error::GfxError;
use crate::pci_device::{DiscreetGpu, GfxMode, HotplugType};
use crate::{
    CONFIG_NVIDIA_VKICD, MODPROBE_INTEGRATED, MODPROBE_NVIDIA_BASE, MODPROBE_NVIDIA_DRM_MODESET_ON,
    MODPROBE_PATH, MODPROBE_VFIO, MODPROBE_NVIDIA_EC_BKLT
};

/// Cleaned config for passing over dbus only
#[derive(Debug, Clone, Deserialize, Serialize, Type)]
pub struct GfxConfigDbus {
    pub mode: GfxMode,
    pub vfio_enable: bool,
    pub vfio_save: bool,
    pub always_reboot: bool,
    pub no_logind: bool,
    pub logout_timeout_s: u64,
    pub hotplug_type: HotplugType,
}

impl From<&GfxConfig> for GfxConfigDbus {
    fn from(c: &GfxConfig) -> Self {
        Self {
            mode: c.mode,
            vfio_enable: c.vfio_enable,
            vfio_save: c.vfio_save,
            always_reboot: c.always_reboot,
            no_logind: c.no_logind,
            logout_timeout_s: c.logout_timeout_s,
            hotplug_type: c.hotplug_type,
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
    pub pending_action: Option<UserActionRequired>,
    /// Set if vfio option is enabled. This requires the vfio drivers to be built as modules
    pub vfio_enable: bool,
    /// Save the VFIO mode so that it is reloaded on boot
    pub vfio_save: bool,
    /// Should always reboot?
    pub always_reboot: bool,
    /// Don't use logind to see if all sessions are logged out and therefore safe to change mode
    pub no_logind: bool,
    /// The timeout in seconds to wait for all user graphical sessions to end. Default is 3 minutes, 0 = infinite. Ignored if `no_logind` or `always_reboot` is set.
    pub logout_timeout_s: u64,
    /// The type of method to use for hotplug. ASUS is... fiddly.
    pub hotplug_type: HotplugType,
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
            always_reboot: false,
            no_logind: false,
            logout_timeout_s: 180,
            hotplug_type: HotplugType::None,
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
            } else if let Ok(data) = serde_json::from_str(&buf) {
                let old: GfxConfig500 = data;
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

pub(crate) fn check_vulkan_icd(mode: GfxMode) -> Result<(), GfxError> {
    let inactive_nv_icd: String = CONFIG_NVIDIA_VKICD.to_owned() + "_inactive";
    info!("check_vulkan_icd: checking for Vulkan ICD profiles...");
    if mode == GfxMode::Vfio || mode == GfxMode::Integrated {
        if std::path::Path::new(CONFIG_NVIDIA_VKICD).exists() {
            info!(
                "check_vulkan_icd: moving {} to {}",
                CONFIG_NVIDIA_VKICD,
                inactive_nv_icd.clone()
            );
            std::fs::rename(CONFIG_NVIDIA_VKICD, inactive_nv_icd)
                .map_err(|err| GfxError::Write(CONFIG_NVIDIA_VKICD.to_owned(), err))?;
        }
    } else if std::path::Path::new(&inactive_nv_icd).exists() {
        info!(
            "check_vulkan_icd: moving {} to {}",
            inactive_nv_icd.clone(),
            CONFIG_NVIDIA_VKICD
        );
        // nvidia icd must be applied
        std::fs::rename(inactive_nv_icd.clone(), CONFIG_NVIDIA_VKICD)
            .map_err(|err| GfxError::Write(inactive_nv_icd, err))?;
    }
    Ok(())
}

pub(crate) fn create_modprobe_conf(mode: GfxMode, device: &DiscreetGpu) -> Result<(), GfxError> {
    if device.is_amd() || device.is_intel() {
        return Ok(());
    }

    let content = match mode {
        GfxMode::Hybrid | GfxMode::AsusEgpu | GfxMode::NvidiaNoModeset => {
            let mut base = MODPROBE_NVIDIA_BASE.to_vec();
            base.append(&mut MODPROBE_NVIDIA_DRM_MODESET_ON.to_vec());
            base.append(&mut MODPROBE_NVIDIA_EC_BKLT.to_vec());
            base
        }
        GfxMode::Vfio => create_vfio_conf(device),
        GfxMode::Integrated => {
            let mut base = MODPROBE_INTEGRATED.to_vec();
            base.append(&mut MODPROBE_NVIDIA_DRM_MODESET_ON.to_vec());
            base.append(&mut MODPROBE_NVIDIA_EC_BKLT.to_vec()); // only 
            base
        }
        GfxMode::None | GfxMode::AsusMuxDgpu => vec![],
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(MODPROBE_PATH)
        .map_err(|err| GfxError::Path(MODPROBE_PATH.into(), err))?;

    info!("create_modprobe_conf: writing {}", MODPROBE_PATH);
    file.write_all(&content)
        .and_then(|_| file.sync_all())
        .map_err(|err| GfxError::Write(MODPROBE_PATH.into(), err))?;

    Ok(())
}
