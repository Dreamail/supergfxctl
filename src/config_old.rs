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
            vfio_enable: old.gfx_vfio_enable,
            vfio_save: false,
            compute_save: false,
            always_reboot: false,
        }
    }
}
