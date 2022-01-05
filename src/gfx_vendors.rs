use serde_derive::{Deserialize, Serialize};
use std::str::FromStr;
use zvariant_derive::Type;

use crate::error::GfxError;

#[derive(Debug, Type, PartialEq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxPower {
    Active,
    Suspended,
    Off,
    Unknown,
}

impl FromStr for GfxPower {
    type Err = GfxError;

    fn from_str(s: &str) -> Result<Self, GfxError> {
        match s.to_lowercase().trim() {
            "active" => Ok(GfxPower::Active),
            "suspended" => Ok(GfxPower::Suspended),
            "off" => Ok(GfxPower::Off),
            _ => Ok(GfxPower::Unknown),
        }
    }
}

impl From<&GfxPower> for &str {
    fn from(gfx: &GfxPower) -> &'static str {
        match gfx {
            GfxPower::Active => "active",
            GfxPower::Suspended => "suspended",
            GfxPower::Off => "off",
            GfxPower::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Type, PartialEq, Copy, Clone)]
pub enum GfxVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

impl From<u16> for GfxVendor {
    fn from(vendor: u16) -> Self {
        match vendor {
            0x1002 => GfxVendor::Amd,
            0x10DE => GfxVendor::Nvidia,
            0x8086 => GfxVendor::Intel,
            _ => GfxVendor::Unknown,
        }
    }
}

impl From<&GfxVendor> for &str {
    fn from(vendor: &GfxVendor) -> Self {
        match vendor {
            GfxVendor::Nvidia => "Nvidia",
            GfxVendor::Amd => "AMD",
            GfxVendor::Intel => "Intel",
            GfxVendor::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Type, PartialEq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxMode {
    Hybrid,
    Dedicated,
    Integrated,
    Compute,
    Vfio,
    Egpu,
}

impl FromStr for GfxMode {
    type Err = GfxError;

    fn from_str(s: &str) -> Result<Self, GfxError> {
        match s.to_lowercase().trim() {
            "hybrid" => Ok(GfxMode::Hybrid),
            "dedicated" => Ok(GfxMode::Dedicated),
            "integrated" => Ok(GfxMode::Integrated),
            "compute" => Ok(GfxMode::Compute),
            "vfio" => Ok(GfxMode::Vfio),
            "epgu" => Ok(GfxMode::Egpu),
            _ => Err(GfxError::ParseVendor),
        }
    }
}

impl From<&GfxMode> for &str {
    fn from(gfx: &GfxMode) -> &'static str {
        match gfx {
            GfxMode::Hybrid => "hybrid",
            GfxMode::Dedicated => "dedicated",
            GfxMode::Integrated => "integrated",
            GfxMode::Compute => "compute",
            GfxMode::Vfio => "vfio",
            GfxMode::Egpu => "egpu",
        }
    }
}

impl From<GfxMode> for &str {
    fn from(gfx: GfxMode) -> &'static str {
        (&gfx).into()
    }
}

#[derive(Debug, Type, PartialEq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxRequiredUserAction {
    Logout,
    Reboot,
    Integrated,
    None,
}

impl From<&GfxRequiredUserAction> for &str {
    fn from(gfx: &GfxRequiredUserAction) -> &'static str {
        match gfx {
            GfxRequiredUserAction::Logout => "logout",
            GfxRequiredUserAction::Reboot => "reboot",
            GfxRequiredUserAction::Integrated => "switch to integrated first",
            GfxRequiredUserAction::None => "no action",
        }
    }
}
