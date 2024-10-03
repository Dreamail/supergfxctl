# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [5.243] - 2024-10-03

### Changed
- Bump deps

## [5.2.3] - 2024-05-06

### Changed
- Minor adjustsments to asus egpu safety logic
- Add missing mode/string conversions
- Bump deps

## [5.2.1] - 2024-03-17

### Changed

- Better sanity check for booting without egpu after it was previously set

### Added

- Vulkan ICD profile switching (thanks Armas Spann)

## [5.1.2] - 2023-09-07
### Changed
- Fix Asus disable_dgpu mode re-enable of dgpu
- Fix asus egpu switching
- Fix writing the correct modprobe if switching from Integrated to Hybrid when Asus hotplug_mode is enabled
- Update zbus deps

## [5.1.1] - 2023-4-26
### Changed
- Adjust the internal action list for VFIO mode

## [5.1.0] - 2023-4-23
### Notes:
- The ASUS Egpu is still in a state of testing. It works, but you must plug it in and flick the switch before changing modes.
- The ASUS MUX toggle always requires a reboot due to how it works internally in the ACPI. The iGPU may still be available.
  - If you have an encrypted disk you may need to bliindly enter your password. A black screen does not always mean boot failed, it's an artifact of kernel boot plus this MUX.
- ASUS dgpu_disable is able to be set to be used for Integrated, but it may or may not work well for the same reasons as egpu above.
- If you dual boot with Windows then the states of dgpu_disable, egpu_enable, and gpu_mux_mode should be picked up by supergfxd and the OS put in the right mode - vice versa for Windows.
### Changed
- Add "Display" to GfxMode
- Add "Display" to GfxRequiredUserAction
- Refactor architecture to actions plus action lists that are dependant on which mode is booted and which mode switching to/from
- Refactor asus egpu handling
- Add a "NvidiaNoModeset" mode specially for machines like the GA401I series
- Enable ASUS MUX control
- Better boot safety checks of dgpu_disable, egpu_enable, gpu_mux_mode
- Adjust boot actions for asus egpu
### **Breaking**
- Dbus args for get/set mode changed to:
  - Hybrid,
  - Integrated,
  - NvidiaNoModeset,
  - Vfio,
  - AsusEgpu,
  - AsusMuxDgpu,
  - None,

## [5.0.1] - 2022-11-03
### Changed
- Rmeoved Compute mode
- Added a check in asus_reload to help GA401I and older laptops
- Add notify gfx power status to dbus

## [5.0.0] - 2022-10-21
### Added
- 99-nvidia-ac.rules udev rule added to ./data, this rule is useful for stopping `nvidia-powerd` on battery as some nvidia based laptops are poorly behaved when it is active (OPTIONAL)
- New config option: `hotplug_type`. This accepts: None (default), Std, or Asus. Std tries to use the kernel hotplug mechanism if available, while Asus tries to use dgpu_disable if available
- With the above, your success with with Std or Asus may vary, and may be unreliable. In general try Std first and check the battery drain after toggling.
### Changed
- nvidia.modeset=0 not required for rebootless switching now
- Removed dedicated mode as it causes more trouble than it is worth (ASUS: use gpu_mux_mode patch or kernel 6.1)
- Better support for ASUS dgpu_disable and egpu_enable
- Ensure sufficient time passes before rescan on dgpu_disable change
- Check if asus gpu_mux_mode exists, and value of it
- Fix: add logind sleep/resume task to ensure dgpu_disable is set (only if hotplug_type == Asus)
- Using udev internally to find devices rather than manually scanning directories
- Cleaner systemd systemctl interaction
- Try to ignore session zbus error on path object not existing
- Rework dgpu detection
- Refactor ordering of device ops
- Retry mode change thread on first fail
- Remove the hotplug prep thing
- Change some &str args to enum + From<T> impl
- **Please review README.md for further info**

## [4.0.5] - 2022-06-22
### Changed
- Fix interaction with lspci >= 3.8.0 (Author: Anton Shangareev)
- add "Quadro" to the lspci parsing for NVIDIA cards (Author: Brandon Bennett)

## [4.0.4] - 2022-02-05
### Changed
- Adjust the kernel cmdline arg code path

## [4.0.3] - 2022-02-04
### Added
- Add config option `no_logind`: Don't use logind to see if all sessions are
  logged out and therefore safe to change mode. This will be useful for people not
  using a login manager, however it is not guaranteed to work unless all graphical
  sessions are ended and nothing is hooking the drivers. Ignored if `always_reboot`
  is set.
- Add config option `logout_timeout_s`: The timeout in seconds to wait for all user
  graphical sessions to end. Default is 3 minutes, 0 = infinite. Ignored if
  `no_logind` or `always_reboot` is set.
- Add new dbus method: `PendingMode`, to check if a mode change is required
- Add new dbus method: `PendingUserAction`, to check if the user is required to perform an action
- Add new dbus method: `Config`, to get the current base config
- Add new dbus method: `SetConfig`, to set the base config
- Add `-p, --pend-action` CLI arg to get the pending user action if any
- Add `-P, --pend-mode` CLI arg to get the pending mode change if any`
- Add ability to read `supergfxd.mode=` from kernel cmdline on startup and set the mode appropriately
### Removed
- CLI option `--force` was unused, it is now removed.

## [4.0.2] - 2022-01-22
### Changed
- Adjust how xorg config is created so that EGPU mode uses it also

## [4.0.1] - 2022-01-20
### Changed
- Fix version upgrade of config
- Recreate the config if parsing fails
- Only write the mode change to config file, don't update live config
### Added
- AMD dedicated + hybrid config for xorg
- "AllowExternalGpus" added to xorg for Nvidia Egpu mode

## [4.0.0] - 2022-01-18
### Added
- Add new dbus method: `Version` to get supergfxd version
- Add new dbus method: `Vendor` to get dGPU vendor name
- Add new dbus method: `Supported` to get list of supported modes
- Add `-v, --version` CLI arg to get supergfxd version
- Add `-V, --vendor` CLI arg to get dGPU vendor name
- Add `-s, --supported` CLI arg to get list of supported modes
- Add new config option: `vfio_save` to reload VFIO on boot
- Add new config option: `compute_save` to reload compute on boot
- Add new config option: `always_reboot` reboot to change modes
### Changed
- Adjust startup to check for ASUS eGPU and dGPU enablement if no modes supported
- If nvidia-drm.modeset=1 is set then save mode and require a reboot by default\
- Add extra check for Nvidia dGPU (fixes Flow 13")
- Properly check the correct device for power status
### Breaking
- Rename Vendor, GetVendor to Mode, GetMode to better reflect their results

## [3.0.0] - 2022-01-10
### Added
- Keep a changelog
### Changed
- Support laptops with AMD dGPU
  + `hybrid`, `integrated`, `vfio` only
  + Modes unsupported by AMD dGPU will return an error
- `nvidia` mode is now `dedicated`
- Don't write the config twice on laptops with hard-mux switch
- CLI print zbus error string if available
- Heavy internal cleanup and refactor to make the project a bit nicer to work with