use log::{error, info, warn};

use crate::{
    do_driver_action,
    error::GfxError,
    gfx_vendors::{GfxPower, GfxVendor},
    pci_device::{rescan_pci_bus, PciDevice, RuntimePowerManagement},
    NVIDIA_DRIVERS,
};

use std::path::PathBuf;

/// Collection of all graphics devices. Functions intend to work on the device
/// determined to be the discreet GPU only.
#[derive(Clone)]
pub struct DiscreetGpu {
    vendor: GfxVendor,
    functions: Vec<PciDevice>,
}

impl DiscreetGpu {
    pub fn new() -> Result<DiscreetGpu, GfxError> {
        info!("Rescanning PCI bus");
        rescan_pci_bus()?;
        let devs = PciDevice::all()?;

        let functions = |parent: &PciDevice| -> Vec<PciDevice> {
            let mut functions = Vec::new();
            if let Some(parent_slot) = parent.id().split('.').next() {
                for func in devs.iter() {
                    if let Some(func_slot) = func.id().split('.').next() {
                        if func_slot == parent_slot {
                            info!("{}: Function for {}", func.id(), parent.id());
                            functions.push(func.clone());
                        }
                    }
                }
            }
            functions
        };

        for dev in devs.iter() {
            // graphics device class
            if 0x03 == (dev.class()? >> 16) & 0xFF {
                if dev.is_dgpu()? {
                    let vendor = GfxVendor::from(dev.vendor()?);
                    if matches!(
                        vendor,
                        GfxVendor::Nvidia | GfxVendor::Amd | GfxVendor::Intel
                    ) {
                        info!("{} dGPU found", <&str>::from(&vendor));
                        dev.set_runtime_pm(RuntimePowerManagement::Auto)?;
                        return Ok(Self {
                            vendor,
                            functions: functions(dev),
                        });
                    }
                }
            }
        }
        Err(GfxError::NotSupported("No dGPU found".to_string()))
    }

    pub fn functions(&self) -> &[PciDevice] {
        &self.functions
    }

    pub fn vendor(&self) -> GfxVendor {
        self.vendor
    }

    pub fn is_nvidia(&self) -> bool {
        self.vendor == GfxVendor::Nvidia
    }

    pub fn is_amd(&self) -> bool {
        self.vendor == GfxVendor::Amd
    }

    pub fn is_intel(&self) -> bool {
        self.vendor == GfxVendor::Intel
    }

    pub fn get_runtime_status(&self) -> Result<GfxPower, GfxError> {
        self.functions[0].get_pm_status()
    }

    pub fn set_runtime_pm(&self, pm: RuntimePowerManagement) -> Result<(), GfxError> {
        self.functions
            .iter()
            .try_for_each(|f| f.set_runtime_pm(pm))
            .map_err(|e| GfxError::from(e))
    }

    pub fn unbind(&self) -> Result<(), GfxError> {
        for func in self.functions.iter() {
            if func.path().exists() {
                match func.driver() {
                    Ok(driver) => {
                        info!("{}: Unbinding {}", driver.id(), func.id());
                        unsafe {
                            driver.unbind(func).map_err(|err| {
                                error!("gfx unbind: {}", err);
                                err
                            })?;
                        }
                    }
                    Err(err) => match err.kind() {
                        std::io::ErrorKind::NotFound => (),
                        _ => {
                            error!("gfx driver: {:?}, {}", func.path(), err);
                            return Err(GfxError::from_io(err, PathBuf::from(func.path())));
                        }
                    },
                }
            }
        }
        Ok(())
    }

    pub fn remove(&self) -> Result<(), GfxError> {
        for func in self.functions.iter() {
            if func.path().exists() {
                match func.driver() {
                    Ok(driver) => {
                        error!("{}: in use by {}", func.id(), driver.id());
                    }
                    Err(why) => match why.kind() {
                        std::io::ErrorKind::NotFound => {
                            info!("{}: Removing", func.id());
                            unsafe {
                                // ignore errors and carry on
                                if let Err(err) = func.remove() {
                                    error!("gfx remove: {}", err);
                                }
                            }
                        }
                        _ => {
                            error!("Remove device failed");
                        }
                    },
                }
            } else {
                warn!("{}: Already removed", func.id());
            }
        }
        info!("Removed all gfx devices");
        Ok(())
    }

    pub fn unbind_remove(&self) -> Result<(), GfxError> {
        self.unbind()?;
        self.remove()
    }

    pub fn do_driver_action(&self, action: &str) -> Result<(), GfxError> {
        if self.is_nvidia() {
            for driver in NVIDIA_DRIVERS.iter() {
                do_driver_action(driver, action)?;
            }
        }
        Ok(())
    }
}
