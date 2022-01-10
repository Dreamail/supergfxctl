use log::{error, info, warn};
use serde_derive::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::GfxError;
use crate::gfx_devices::DiscreetGpu;
use crate::gfx_vendors::GfxMode;
use crate::{
    MODPROBE_INTEGRATED, MODPROBE_NVIDIA_BASE, MODPROBE_NVIDIA_DRM_MODESET, MODPROBE_PATH,
    MODPROBE_VFIO, PRIMARY_GPU_END, PRIMARY_GPU_NVIDIA, PRIMARY_GPU_NVIDIA_BEGIN, XORG_FILE,
    XORG_PATH,
};

#[derive(Deserialize, Serialize)]
pub struct GfxConfig {
    #[serde(skip)]
    config_path: String,
    /// The current mode set, also applies on boot
    pub gfx_mode: GfxMode,
    /// Only for informational purposes
    #[serde(skip)]
    pub gfx_tmp_mode: Option<GfxMode>,
    /// Set if graphics management is enabled
    pub gfx_managed: bool,
    /// Set if vfio option is enabled. This requires the vfio drivers to be built as modules
    pub gfx_vfio_enable: bool,
}

impl GfxConfig {
    fn new(config_path: String) -> Self {
        Self {
            config_path,
            gfx_mode: GfxMode::Hybrid,
            gfx_tmp_mode: None,
            gfx_managed: true,
            gfx_vfio_enable: false,
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
            } else {
                warn!("Could not deserialise {}", config_path);
                panic!("Please remove {} then restart service", config_path);
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
                x.gfx_tmp_mode = self.gfx_tmp_mode;
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
    for (f_count, func) in devices.functions().iter().enumerate() {
        let vendor = func.vendor().unwrap();
        let device = func.device().unwrap();
        unsafe {
            vifo.append(format!("{:x}", vendor).as_mut_vec());
        }
        vifo.append(&mut vec![b':']);
        unsafe {
            vifo.append(format!("{:x}", device).as_mut_vec());
        }
        if f_count < devices.functions().len() - 1 {
            vifo.append(&mut vec![b',']);
        }
    }
    vifo.append(&mut vec![b',']);

    let mut conf = MODPROBE_INTEGRATED.to_vec();
    conf.append(&mut vifo);
    conf
}

pub(crate) fn create_modprobe_conf(vendor: GfxMode, devices: &DiscreetGpu) -> Result<(), GfxError> {
    info!("Writing {}", MODPROBE_PATH);
    let content = match vendor {
        GfxMode::Dedicated | GfxMode::Hybrid | GfxMode::Egpu => {
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
        GfxMode::Integrated => MODPROBE_INTEGRATED.to_vec(),
        GfxMode::Compute => MODPROBE_NVIDIA_BASE.to_vec(),
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

/// Write the appropriate xorg config for the chosen mode
pub(crate) fn create_xorg_conf(mode: GfxMode, gfx: &DiscreetGpu) -> Result<(), GfxError> {
    let text = if gfx.is_nvidia() {
        if mode == GfxMode::Dedicated {
            [
                PRIMARY_GPU_NVIDIA_BEGIN,
                PRIMARY_GPU_NVIDIA,
                PRIMARY_GPU_END,
            ]
            .concat()
        } else {
            [PRIMARY_GPU_NVIDIA_BEGIN, PRIMARY_GPU_END].concat()
        }
    } else if gfx.is_amd() {
        warn!("No valid AMD dGPU xorg config available yet");
        return Ok(());
    } else {
        warn!("No valid xorg config for device");
        return Ok(());
    };

    if !Path::new(XORG_PATH).exists() {
        std::fs::create_dir(XORG_PATH).map_err(|err| GfxError::Write(XORG_PATH.into(), err))?;
    }

    let mut path = PathBuf::from(XORG_PATH);
    path.push(XORG_FILE);
    info!("Writing {}", path.display());
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .map_err(|err| GfxError::Write(format!("{}", path.display()), err))?;

    file.write_all(&text)
        .and_then(|_| file.sync_all())
        .map_err(|err| GfxError::Write(MODPROBE_PATH.into(), err))?;
    Ok(())
}
