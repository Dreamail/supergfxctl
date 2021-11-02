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

#[derive(Debug, Type, PartialEq, Copy, Clone, Deserialize, Serialize)]
pub enum GfxVendors {
    Nvidia,
    Integrated,
    Compute,
    Vfio,
    Egpu,
    Hybrid,
}

impl FromStr for GfxVendors {
    type Err = GfxError;

    fn from_str(s: &str) -> Result<Self, GfxError> {
        match s.to_lowercase().trim() {
            "nvidia" => Ok(GfxVendors::Nvidia),
            "hybrid" => Ok(GfxVendors::Hybrid),
            "compute" => Ok(GfxVendors::Compute),
            "vfio" => Ok(GfxVendors::Vfio),
            "epgu" => Ok(GfxVendors::Egpu),
            "integrated" => Ok(GfxVendors::Integrated),
            _ => Err(GfxError::ParseVendor),
        }
    }
}

impl From<&GfxVendors> for &str {
    fn from(gfx: &GfxVendors) -> &'static str {
        match gfx {
            GfxVendors::Nvidia => "nvidia",
            GfxVendors::Hybrid => "hybrid",
            GfxVendors::Compute => "compute",
            GfxVendors::Vfio => "vfio",
            GfxVendors::Egpu => "egpu",
            GfxVendors::Integrated => "integrated",
        }
    }
}

impl From<GfxVendors> for &str {
    fn from(gfx: GfxVendors) -> &'static str {
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
