### Graphics switching

`asusd` can switch graphics modes between:
- `integrated`, uses the iGPU only and force-disables the dGPU
- `compute`, enables Nvidia without Xorg. Useful for ML/Cuda
- `hybrid`, enables Nvidia prime-offload mode
- `nvidia`, uses the Nvidia gpu only
- `vfio`, binds the Nvidia gpu to vfio for VM pass-through

There is no guide or tutorial for new Linux users who want to use this tool. It would be better if this gets added to the [README.md](https://gitlab.com/asus-linux/supergfxctl/-/blob/main/README.md) and/or the [Wiki](https://gitlab.com/asus-linux/supergfxctl/-/wikis/home) of this project.

**Install requirements**

* Debian/Ubuntu: `sudo apt update && install curl git build-essential`
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

**Switch GPU modes**

* Switching to/from Hybrid and Nvidia modes requires a logout only. (no reboot)
* Switching between integrated/compute/vfio is instant. (no logout or reboot)

| GPU Modes  | Command                            |
|------------|------------------------------------|
| Integrated | sudo supergfxctl --mode integrated |
| Nvidia     | sudo supergfxctl --mode nvidia     |
| Hybrid     | sudo supergfxctl --mode hybrid     |
| Compute    | sudo supergfxctl --mode compute    |
| VFIO       | sudo supergfxctl --mode vfio       |

#### Required actions in distro

**Rebootless note:** You must edit `/etc/default/grub` to remove `nvidia-drm.modeset=1`
from the line `GRUB_CMDLINE_LINUX=` and then recreate your grub config. In fedora
you can do this with `sudo grub2-mkconfig -o /etc/grub2.cfg` - other distro may be
similar but with a different config location. It's possible that graphics driver updates
may change this.

This switcher conflicts with other gpu switchers like optimus-manager, suse-prime
or ubuntu-prime, system76-power, and bbswitch. If you have issues with `asusd`
always defaulting to `integrated` mode on boot then you will need to check for
stray configs blocking nvidia modules from loading in:
- `/etc/modprobe.d/`
- `/usr/lib/modprope.d/`

#### Config options

1. `"gfx_mode": "<MODE>",`: MODE can be <Integrated, Hybrid, Compute, Nvidia, vfio>
2. `"gfx_last_mode": "Nvidia",`: currently unused
3. `"gfx_managed": true,`: enable or disable graphics switching controller
4. `"gfx_vfio_enable": false,`: enable vfio switching for Nvidia GPU passthrough
5. `"gfx_save_compute_vfio": false,`: wether or not to save the vfio state (so it sticks between boots)

#### Graphics switching notes

**G-Sync note:** Some laptops are capable of using the dGPU as the sole GPU in the system which is generally to enable g-sync on the laptop display panel. This is controlled by the bios/efivar control and will be covered in that section.

**vfio note:** The vfio modules *must not* be compiled into the kernel, they need
to be separate modules. If you don't plan to use vfio mode then you can ignore this
otherwise you may need a custom built kernel.
