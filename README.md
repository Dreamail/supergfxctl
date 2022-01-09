### Graphics switching

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

**Install requirements**

* Debian/Ubuntu: `sudo apt update && sudo apt install curl git build-essential`
* Fedora/RHEL: `sudo dnf upgrade && sudo dnf install curl git @development_tools`
* Arch/Manjaro: `sudo pacman -Syu && sudo pacman -S curl git base-devel`

**Install Rust**

* `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs`

**Activate Rust environment**

* `source ~/.bash_profile`
* `source ~/.profile`
* `source ~/.cargo/env`

**Clone supergfxctl repository**

* `git clone https://gitlab.com/asus-linux/supergfxctl.git`

**Install supergfxctl**

* `cd supergfxctl`
* `make && sudo make install`

**Enable service**

* `sudo systemctl enable supergfxd.service`
* `sudo systemctl start supergfxd.service`

**Add user to group**

Depending on your distro add your username to one of `adm`, `users`, `wheel` then
refresh your session (easiest way is to reboot).

* `sudo usermod -a -G users $USER`

**Switch GPU modes**

* Switching to/from Hybrid and Nvidia modes requires a logout only. (no reboot)
* Switching between integrated/compute/vfio is instant. (no logout or reboot)

| GPU Modes  | Command                            |
|------------|------------------------------------|
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

#### supergfxctl

```
supergfxctl --help
Optional arguments:
  -h, --help   print help message
  -m, --mode   Set graphics mode: <hybrid, dedicated, integrated, compute, vfio, egpu>
  -g, --get    Get the current mode
  -p, --pow    Get the current power status
  -f, --force  Do not ask for confirmation
```

#### Config options

1. `"gfx_mode": "<MODE>",`: MODE can be <hybrid, dedicated, integrated, compute, vfio, egpu>
2. `"gfx_last_mode": "Dedicated",`: currently unused
3. `"gfx_managed": true,`: enable or disable graphics switching controller
4. `"gfx_vfio_enable": false,`: enable vfio switching for dGPU passthrough
5. `"gfx_save_compute_vfio": false,`: wether or not to save the vfio state (so it sticks between boots)

#### Graphics switching notes

**G-Sync note:** Some laptops are capable of using the dGPU as the sole GPU in the system which is generally to enable g-sync on the laptop display panel. This is controlled by asusctl at this time, and may be added to supergfxd later.

**vfio note:** The vfio modules *must not* be compiled into the kernel, they need
to be separate modules. If you don't plan to use vfio mode then you can ignore this
otherwise you may need a custom built kernel.
