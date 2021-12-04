/// The configuration for graphics. This should be saved and loaded on boot.
pub mod config;
/// Control functions for setting graphics.
pub mod controller;
/// Error: 404
pub mod error;
/// Mode names, follows what distros defined as common.
pub mod gfx_vendors;
/// Special-case functions for check/read/write of key functions on unique laptops
/// such as the G-Sync mode available on some ASUS ROG laptops
pub mod special;
/// System interface helpers.
pub mod system;
/// Defined DBUS Interface for supergfxctl
pub mod zbus_iface;
/// Defined DBUS Proxy for supergfxctl
pub mod zbus_proxy;

/// Helper to expose the current crate version to external code
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Generic path that is used to save the daemon config state
pub const CONFIG_PATH: &str = "/etc/supergfxd.conf";
/// Destination name to be used in the daemon when setting up DBUS connection
pub const DBUS_DEST_NAME: &str = "org.supergfxctl.Daemon";
/// Interface path name. Should be common across daemon and client.
pub const DBUS_IFACE_PATH: &str = "/org/supergfxctl/Gfx";

const NVIDIA_DRIVERS: [&str; 4] = ["nvidia_drm", "nvidia_modeset", "nvidia_uvm", "nvidia"];

const VFIO_DRIVERS: [&str; 6] = [
    "vfio_pci_core",
    "vfio-pci",
    "vfio_iommu_type1",
    "vfio_virqfd",
    "vfio_mdev",
    "vfio",
];

const DISPLAY_MANAGER: &str = "display-manager.service";

const MODPROBE_PATH: &str = "/etc/modprobe.d/supergfxd.conf";

static MODPROBE_BASE: &[u8] = br#"# Automatically generated by supergfxd
blacklist nouveau
alias nouveau off
options nvidia NVreg_DynamicPowerManagement=0x02
"#;

static MODPROBE_DRM_MODESET: &[u8] = br#"
options nvidia-drm modeset=1
"#;

static MODPROBE_INTEGRATED: &[u8] = br#"# Automatically generated by supergfxd
blacklist i2c_nvidia_gpu
blacklist nvidia
blacklist nvidia-drm
blacklist nvidia-modeset
blacklist nouveau
alias nouveau off
"#;

static MODPROBE_VFIO: &[u8] = br#"options vfio-pci ids="#;

const XORG_FILE: &str = "90-nvidia-primary.conf";
const XORG_PATH: &str = "/etc/X11/xorg.conf.d/";

static PRIMARY_GPU_BEGIN: &[u8] = br#"# Automatically generated by supergfxd
Section "OutputClass"
    Identifier "nvidia"
    MatchDriver "nvidia-drm"
    Driver "nvidia"
    Option "AllowEmptyInitialConfiguration" "true""#;

static PRIMARY_GPU_NVIDIA: &[u8] = br#"
    Option "PrimaryGPU" "true""#;

static PRIMARY_GPU_END: &[u8] = br#"
EndSection"#;

static EGPU_ENABLE_PATH: &str = "/sys/devices/platform/asus-nb-wmi/egpu_enable";
