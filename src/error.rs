use std::fmt;
use std::{error, path::PathBuf};

#[derive(Debug)]
pub enum GfxError {
    ParseVendor,
    DgpuNotFound,
    Udev(String, std::io::Error),
    DisplayManagerAction(String),
    DisplayManagerTimeout(String),
    AsusGpuMuxModeDedicated,
    VfioBuiltin,
    VfioDisabled,
    MissingModule(String),
    Modprobe(String),
    Command(String, std::io::Error),
    Path(String, std::io::Error),
    Read(String, std::io::Error),
    Write(String, std::io::Error),
    NotSupported(String),
    Io(PathBuf, std::io::Error),
    Zbus(zbus::Error),
    ZbusFdo(zbus::fdo::Error),
}

impl GfxError {
    pub fn from_io(error: std::io::Error, detail: PathBuf) -> Self {
        Self::Io(detail, error)
    }
}

impl fmt::Display for GfxError {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GfxError::ParseVendor => write!(f, "Could not parse vendor name"),
            GfxError::DgpuNotFound => write!(
                f,
                "Didn't find dgpu. If this is an ASUS ROG/TUF laptop this is okay"
            ),
            GfxError::Udev(msg, err) => write!(f, "udev: {msg}: {err}"),
            GfxError::DisplayManagerAction(action) => {
                write!(f, "Display-manager action {} failed", action)
            }
            GfxError::DisplayManagerTimeout(state) => {
                write!(f, "Timed out waiting for display-manager {} state", state)
            }
            GfxError::AsusGpuMuxModeDedicated => write!(
                f,
                "Can not switch gfx modes when dedicated/G-Sync mode is active"
            ),
            GfxError::VfioBuiltin => write!(
                f,
                "Can not switch to vfio mode if the modules are built in to kernel"
            ),
            GfxError::VfioDisabled => {
                write!(f, "Can not switch to vfio mode if disabled in config file")
            }
            GfxError::MissingModule(m) => write!(f, "The module {} is missing", m),
            GfxError::Modprobe(detail) => write!(f, "Modprobe error: {}", detail),
            GfxError::Command(func, error) => write!(f, "Command exec error: {}: {}", func, error),
            GfxError::Path(path, error) => write!(f, "Path {}: {}", path, error),
            GfxError::Read(path, error) => write!(f, "Read {}: {}", path, error),
            GfxError::Write(path, error) => write!(f, "Write {}: {}", path, error),
            GfxError::NotSupported(path) => write!(f, "{}", path),
            GfxError::Io(detail, error) => {
                if detail.clone().into_os_string().is_empty() {
                    write!(f, "std::io error: {}", error)
                } else {
                    write!(f, "std::io error: {}, {}", error, detail.display())
                }
            }
            GfxError::Zbus(detail) => write!(f, "Zbus error: {}", detail),
            GfxError::ZbusFdo(detail) => write!(f, "Zbus error: {}", detail),
        }
    }
}

impl error::Error for GfxError {}

impl From<zbus::Error> for GfxError {
    fn from(err: zbus::Error) -> Self {
        GfxError::Zbus(err)
    }
}

impl From<zbus::fdo::Error> for GfxError {
    fn from(err: zbus::fdo::Error) -> Self {
        GfxError::ZbusFdo(err)
    }
}

impl From<std::io::Error> for GfxError {
    fn from(err: std::io::Error) -> Self {
        GfxError::Io(PathBuf::new(), err)
    }
}
