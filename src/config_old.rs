use serde_derive::{Deserialize, Serialize};

use crate::{config::GfxConfig, pci_device::GfxMode, special_asus::asus_dgpu_exists};

#[derive(Debug, PartialEq, Eq, Copy, Clone, Deserialize, Serialize)]
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
            GfxMode300::Nvidia => GfxMode::Hybrid,
            GfxMode300::Integrated => GfxMode::Integrated,
            GfxMode300::Compute => GfxMode::Compute,
            GfxMode300::Vfio => GfxMode::Vfio,
            GfxMode300::Egpu => GfxMode::Egpu,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct GfxConfig300 {
    pub gfx_mode: GfxMode,
    pub gfx_managed: bool,
    pub gfx_vfio_enable: bool,
}

impl From<GfxConfig300> for GfxConfig {
    fn from(old: GfxConfig300) -> Self {
        GfxConfig {
            config_path: Default::default(),
            mode: old.gfx_mode,
            tmp_mode: Default::default(),
            pending_mode: None,
            pending_action: None,
            vfio_enable: old.gfx_vfio_enable,
            vfio_save: false,
            compute_save: false,
            always_reboot: false,
            no_logind: false,
            logout_timeout_s: 180,
            asus_use_dgpu_disable: asus_dgpu_exists(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GfxConfig402 {
    pub mode: GfxMode,
    pub vfio_enable: bool,
    pub vfio_save: bool,
    pub compute_save: bool,
    pub always_reboot: bool,
}

impl From<GfxConfig402> for GfxConfig {
    fn from(old: GfxConfig402) -> Self {
        GfxConfig {
            config_path: Default::default(),
            mode: old.mode,
            tmp_mode: Default::default(),
            pending_mode: None,
            pending_action: None,
            vfio_enable: old.vfio_enable,
            vfio_save: old.vfio_save,
            compute_save: old.compute_save,
            always_reboot: old.always_reboot,
            no_logind: false,
            logout_timeout_s: 180,
            asus_use_dgpu_disable: asus_dgpu_exists(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GfxConfig405 {
    pub mode: GfxMode,
    pub vfio_enable: bool,
    pub vfio_save: bool,
    pub compute_save: bool,
    pub always_reboot: bool,
    pub no_logind: bool,
    pub logout_timeout_s: u64,
}

impl From<GfxConfig405> for GfxConfig {
    fn from(old: GfxConfig405) -> Self {
        GfxConfig {
            config_path: Default::default(),
            mode: old.mode,
            tmp_mode: Default::default(),
            pending_mode: None,
            pending_action: None,
            vfio_enable: old.vfio_enable,
            vfio_save: old.vfio_save,
            compute_save: old.compute_save,
            always_reboot: old.always_reboot,
            no_logind: false,
            logout_timeout_s: 180,
            asus_use_dgpu_disable: asus_dgpu_exists(),
        }
    }
}
