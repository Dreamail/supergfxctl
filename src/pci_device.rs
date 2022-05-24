use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::process::Command;
use std::str::FromStr;
use std::{
    fs::write,
    path::{Path, PathBuf},
};

use crate::error::GfxError;
use crate::gfx_vendors::GfxPower;

const PCI_BUS_PATH: &str = "/sys/bus/pci";
const PM_CONTROL_PATH: &str = "power/control";
const PM_RUNTIME_STATUS_PATH: &str = "power/runtime_status";

/// Will rescan the device tree, which adds all removed devices back
pub fn rescan_pci_bus() -> Result<(), GfxError> {
    let path = PathBuf::from(PCI_BUS_PATH).join("rescan");
    write(&path, "1").map_err(|e| GfxError::from_io(e, path))
}

#[derive(Clone)]
pub struct PciDriver {
    path: PathBuf,
}

impl PciDriver {
    /// Return the id of the sys object
    pub fn id(&self) -> &str {
        self.path
            .file_name()
            .unwrap() // A valid path does not end in .., so safe
            .to_str()
            .unwrap() // A valid path must be valid UTF-8, so safe
    }

    /// Write a file underneath the sys object
    fn write_file<P: AsRef<Path>, S: AsRef<[u8]>>(&self, name: P, data: S) -> Result<(), GfxError> {
        let path = self.path.join(name.as_ref());
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| GfxError::from_io(e, path.clone()))?;
        file.write_all(data.as_ref())
            .map_err(|e| GfxError::from_io(e, path))?;

        Ok(())
    }

    pub unsafe fn bind(&self, device: &PciDevice) -> Result<(), GfxError> {
        self.write_file("bind", device.id())
    }

    pub unsafe fn unbind(&self, device: &PciDevice) -> Result<(), GfxError> {
        self.write_file("unbind", device.id())
    }
}

macro_rules! pci_devices {
    ($( fn $file:tt -> $out:tt; )*) => {
        $(
            pub fn $file(&self) -> Result<$out, GfxError> {
                let v = self.read_file(stringify!($file))?;
                $out::from_str_radix(v[2..].trim(), 16).map_err(|err| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{}/{}: {}", self.path().display(), stringify!($file), err)
                    ).into()
                })
            }
        )*
    }
}

#[derive(Clone)]
pub struct PciDevice {
    path: PathBuf,
}

impl PciDevice {
    /// Retrieve all of the object instances of a sys class
    pub fn all() -> Result<Vec<Self>, GfxError> {
        let mut ret = Vec::new();
        let path = PathBuf::from(PCI_BUS_PATH).join("devices");
        for entry_res in fs::read_dir(&path).map_err(|e| GfxError::from_io(e, path.clone()))? {
            let entry = entry_res.map_err(|e| GfxError::from_io(e, path.clone()))?;
            if entry.path().is_dir() {
                ret.push(Self { path: entry.path() });
            }
        }

        Ok(ret)
    }

    /// Return the id of the sys object
    pub fn id(&self) -> &str {
        self.path()
            .file_name()
            .unwrap() // A valid path does not end in .., so safe
            .to_str()
            .unwrap() // A valid path must be valid UTF-8, so safe
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn owned_path(&self) -> PathBuf {
        self.path.clone()
    }

    /// Read a file underneath the sys object
    fn read_file<P: AsRef<Path>>(&self, name: P) -> Result<String, GfxError> {
        let mut data = String::new();
        let path = self.path().join(name.as_ref());
        let mut file = fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(|e| GfxError::from_io(e, path.clone()))?;
        file.read_to_string(&mut data)
            .map_err(|e| GfxError::from_io(e, path))?;

        Ok(data)
    }

    /// Write a file underneath the sys object
    fn write_file<P: AsRef<Path>, S: AsRef<[u8]>>(&self, name: P, data: S) -> Result<(), GfxError> {
        let path = self.path().join(name.as_ref());
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| GfxError::from_io(e, path.clone()))?;
        file.write_all(data.as_ref())
            .map_err(|e| GfxError::from_io(e, path))?;

        Ok(())
    }

    pci_devices! {
        fn class -> u32;
        fn device -> u16;
        fn vendor -> u16;
    }

    fn lscpi(&self) -> Result<String, GfxError> {
        let code = format!("{:#01X}:{:#01X}", self.vendor()?, self.device()?);
        let mut cmd = Command::new("lspci");
        cmd.args(["-d", &code]);
        let s = String::from_utf8_lossy(&cmd.output()?.stdout).into_owned();
        Ok(s)
    }

    pub fn lscpi_amd_check(&self) -> Result<bool, GfxError> {
        let s = self.lscpi()?;
        for pat in ["Radeon RX"] {
            if s.contains(pat) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn lscpi_nvidia_check(&self) -> Result<bool, GfxError> {
        let s = self.lscpi()?;
        for pat in ["GeForce", "Quadro"] {
            if s.contains(pat) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn is_dgpu(&self) -> Result<bool, GfxError> {
        // The non-boot GPU is the dGPU
        match self.read_file("boot_vga") {
            Ok(n) => {
                if n.trim() == "0" {
                    return Ok(true);
                } else {
                    return Ok(false);
                }
            }
            Err(_) => {
                if self.lscpi_nvidia_check()? | self.lscpi_amd_check()? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn driver(&self) -> io::Result<PciDriver> {
        fs::canonicalize(self.path.join("driver")).map(|path| PciDriver { path })
    }

    pub unsafe fn remove(&self) -> Result<(), GfxError> {
        self.write_file("remove", "1")
    }

    pub fn set_runtime_pm(&self, state: RuntimePowerManagement) -> Result<(), GfxError> {
        self.write_file(PM_CONTROL_PATH, <&'static str>::from(state))
    }

    pub fn get_runtime_status(&self) -> Result<GfxPower, GfxError> {
        match self.read_file(PM_RUNTIME_STATUS_PATH) {
            Ok(inner) => GfxPower::from_str(inner.as_str()),
            Err(_) => {
                // if let Some(er) = inner.raw_os_error() {
                //     if er != 2 {
                //         return Err(GfxError::from_io(inner));
                //     }
                // }
                Ok(GfxPower::Off)
            }
        }
    }
}

/// Control whether a device uses, or does not use, runtime power management.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RuntimePowerManagement {
    Auto,
    On,
    Off,
}

impl From<RuntimePowerManagement> for &'static str {
    fn from(pm: RuntimePowerManagement) -> &'static str {
        match pm {
            RuntimePowerManagement::Auto => "auto",
            RuntimePowerManagement::On => "on",
            RuntimePowerManagement::Off => "off",
        }
    }
}

impl From<&str> for RuntimePowerManagement {
    fn from(pm: &str) -> RuntimePowerManagement {
        match pm {
            "auto" => RuntimePowerManagement::Auto,
            "on" => RuntimePowerManagement::On,
            "off" => RuntimePowerManagement::Off,
            _ => RuntimePowerManagement::On,
        }
    }
}
