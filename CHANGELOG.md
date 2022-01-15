# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Add new dbus method: `Version` to get supergfxd version
- Add new dbus method: `Vendor` to get dGPU vendor name
- Add new dbus method: `Supported` to get list of supported modes
- Add `-v, --version` CLI arg to get supergfxd version
- Add `-V, --vendor` CLI arg to get dGPU vendor name
- Add `-s, --supported` CLI arg to get list of supported modes
### Changed
- Adjust startup to check for ASUS eGPU and dGPU enablement if no modes supported
- If nvidia-drm.modeset=1 is set then save mode and require a reboot by default
- Add option in `/etc/supergfxd.conf` for `always_reboot`
- Add extra check for Nvidia dGPU (fixes Flow 13")
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