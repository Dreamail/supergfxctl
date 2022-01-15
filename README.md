# supergfxctl

`supergfxd` can switch graphics modes between:
- `hybrid`, enables dGPU-offload mode
- `integrated`, uses the iGPU only and force-disables the dGPU
- `vfio`, binds the dGPU to vfio for VM pass-through

**Nvidia only**

- `dedicated`, uses the dGPU only (note, nvidia + xorg only)
- `compute`, enables Nvidia without Xorg. Useful for ML/Cuda (note, nvidia only)

**ASUS ROG Flow 13" only**

- `egpu`, this is for certain ASUS laptops like 13" Flow to enable external GPU

This switcher conflicts with other gpu switchers like optimus-manager, suse-prime
or ubuntu-prime, system76-power, and bbswitch. If you have issues with `supergfxd`
always defaulting to `integrated` mode on boot then you will need to check for
stray configs blocking nvidia modules from loading in:
- `/etc/modprobe.d/`
- `/usr/lib/modprope.d/`

ASUS laptops require a kernel 5.15.x or newer.

## Building

First you need to install the dev packages required.

* Debian/Ubuntu: `sudo apt update && sudo apt install curl git build-essential`
* Fedora/RHEL: `sudo dnf upgrade && sudo dnf install curl git @development_tools`
* Arch/Manjaro: `sudo pacman -Syu && sudo pacman -S curl git base-devel`

**Install Rust**
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs
source ~/.cargo/env
```

**Clone supergfxctl repository**

`git clone https://gitlab.com/asus-linux/supergfxctl.git`

**Install supergfxctl**
```
cd supergfxctl
make && sudo make install
```

**Enable and start the service**

`sudo systemctl enable supergfxd.service --now`

**Add user to group**

Depending on your distro add your username to one of `adm`, `users`, `wheel` then
refresh your session (easiest way is to reboot).

* `sudo usermod -a -G users $USER`

**Switch GPU modes**

* Switching to/from Hybrid and Dedicated modes requires a logout only. (no reboot)
* Switching between integrated/compute/vfio is instant. (no logout or reboot)

| GPU Modes  | Command                       |
|------------|-------------------------------|
| Integrated | supergfxctl --mode integrated |
| Dedicated  | supergfxctl --mode dedicated  |
| Hybrid     | supergfxctl --mode hybrid     |
| Compute    | supergfxctl --mode compute    |
| VFIO       | supergfxctl --mode vfio       |

#### Required actions in distro

**NVIDIA Rebootless note:** You must edit `/etc/default/grub` to edit `nvidia-drm.modeset=1`
to `nvidia-drm.modeset=0` on the line `GRUB_CMDLINE_LINUX=` and then recreate your grub config.

In fedora you can do this with `sudo grub2-mkconfig -o /etc/grub2.cfg` - other distro may be
similar but with a different config location. It's possible that graphics driver updates
may change this.

If `nvidia-drm.modeset=1` is used then supergfxd requires a reboot to change modes.

#### supergfxctl

```
supergfxctl --help
Optional arguments:
  -h, --help       print help message
  -m, --mode       Set graphics mode
  -v, --version    Get supergfxd version
  -g, --get        Get the current mode
  -s, --supported  Get the supported modes
  -V, --vendor     Get the dGPU vendor name
  -p, --pow        Get the current power status
  -f, --force      Do not ask for confirmation
  --verbose        Verbose output
```

#### Config options

1. `mode`: <MODE> : any of supported modes, must be capitalised
2. `vfio_enable` <bool> : enable vfio switching for dGPU passthrough
3. `vfio_save` <bool> : save vfio state in mode (so it sticks between boots)
4. `compute_save` <bool> : save compute state in mode (so it sticks between boots)
5. `always_reboot` <bool> : always require a reboot to change modes (helps some laptops)

#### Graphics switching notes

**G-Sync note:** Some laptops are capable of using the dGPU as the sole GPU in the system which is generally to enable g-sync on the laptop display panel. This is controlled by asusctl at this time, and may be added to supergfxd later.

**vfio note:** The vfio modules *must not* be compiled into the kernel, they need
to be separate modules. If you don't plan to use vfio mode then you can ignore this
otherwise you may need a custom built kernel.
