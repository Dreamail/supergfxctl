use serde_derive::{Deserialize, Serialize};

use crate::{config::GfxConfig, gfx_vendors::GfxMode};

#[derive(Debug, PartialEq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxMode300 {
    Hybrid,
    Nvidia,
    Integrated,
    Compute,
    Vfio,
    Egpu,
}

impl From<GfxMode300> for GfxMode {
    fn from(m: GfxMode300) -> Self {
        match m {
            GfxMode300::Hybrid => GfxMode::Hybrid,
            GfxMode300::Nvidia => GfxMode::Dedicated,
            GfxMode300::Integrated => GfxMode::Integrated,
            GfxMode300::Compute => GfxMode::Compute,
            GfxMode300::Vfio => GfxMode::Vfio,
            GfxMode300::Egpu => GfxMode::Egpu,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct GfxConfig300 {
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

impl From<GfxConfig300> for GfxConfig {
    fn from(old: GfxConfig300) -> Self {
        GfxConfig {
            config_path: old.config_path,
            mode: old.gfx_mode.into(),
            tmp_mode: old.gfx_tmp_mode,
            pending_mode: None,
            pending_action: None,
            vfio_enable: old.gfx_vfio_enable,
            vfio_save: false,
            compute_save: false,
            always_reboot: false,
            no_logind: false,
            logout_timeout_s: 180,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GfxConfig402 {
    #[serde(skip)]
    pub config_path: String,
    /// The current mode set, also applies on boot
    pub mode: GfxMode,
    /// Only for informational purposes
    #[serde(skip)]
    pub tmp_mode: Option<GfxMode>,
    /// Set if vfio option is enabled. This requires the vfio drivers to be built as modules
    pub vfio_enable: bool,
    /// Save the VFIO mode so that it is reloaded on boot
    pub vfio_save: bool,
    /// Save the Compute mode so that it is reloaded on boot
    pub compute_save: bool,
    /// Should always reboot?
    pub always_reboot: bool,
}

impl From<GfxConfig402> for GfxConfig {
    fn from(old: GfxConfig402) -> Self {
        GfxConfig {
            config_path: old.config_path,
            mode: old.mode,
            tmp_mode: old.tmp_mode,
            pending_mode: None,
            pending_action: None,
            vfio_enable: old.vfio_enable,
            vfio_save: old.vfio_save,
            compute_save: old.compute_save,
            always_reboot: old.always_reboot,
            no_logind: false,
            logout_timeout_s: 180,
        }
    }
}
