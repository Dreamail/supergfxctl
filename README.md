# supergfxctl

**You need this only if you:**
1. have a laptop that can't suspend its dGPU
2. need an easy way to use vfio
3. want to monitor the dGPU status
4. want to try using hotplug and/or ASUS ROG dgpu_disable (results will vary)
5. have an ASUS with an eGPU and need to switch to it (testing required please)

**Xorg is no-longer supported (but supergfxd still works with it)**

`supergfxd` can switch graphics modes between:
- `hybrid`, enables dGPU-offload mode
- `integrated`, uses the iGPU only and force-disables the dGPU
- `vfio`, binds the dGPU to vfio for VM pass-through

**If rebootless switch fails:** you may need the following:

```
sudo sed -i 's/#KillUserProcesses=no/KillUserProcesses=yes/' /etc/systemd/logind.conf
```

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
* Fedora/RHEL: `sudo dnf upgrade && sudo dnf install curl git && sudo dnf groupinstall "Development Tools"`
* Arch/Manjaro: `sudo pacman -Syu && sudo pacman -S curl git base-devel`

**Install Rust**
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
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

* Switching to/from Hybrid mode requires a logout only. (no reboot)
* Switching between integrated/vfio is instant. (no logout or reboot)
* Mode can be set via kernel cmdline with `supergfxd.mode=`. Capitalisation does not matter.

| GPU Modes  | Command                       |
|------------|-------------------------------|
| Integrated | supergfxctl --mode integrated |
| Hybrid     | supergfxctl --mode hybrid     |
| VFIO       | supergfxctl --mode vfio       |

#### supergfxctl

```
supergfxctl --help
Optional arguments:
  -h, --help         print help message
  -m, --mode         Set graphics mode
  -v, --version      Get supergfxd version
  -g, --get          Get the current mode
  -s, --supported    Get the supported modes
  -V, --vendor       Get the dGPU vendor name
  -S, --status       Get the current power status
  -p, --pend-action  Get the pending user action if any
  -P, --pend-mode    Get the pending mode change if any
```

#### Config options /etc/supergfxd.conf

1. `mode`: <MODE> : any of supported modes, must be capitalised
2. `vfio_enable` <bool> : enable vfio switching for dGPU passthrough
3. `vfio_save` <bool> : save vfio state in mode (so it sticks between boots)
5. `always_reboot` <bool> : always require a reboot to change modes (helps some laptops)
6. `no_logind` <bool> : don't use logind to see if all sessions are logged out and therefore safe to change mode. This will be useful for people not using a login manager. Ignored if `always_reboot` is set.
7. `logout_timeout_s` <u64> : the timeout in seconds to wait for all user graphical sessions to end. Default is 3 minutes, 0 = infinite. Ignored if `no_logind` or `always_reboot` is set.
8. `hotplug_type` <enum> : None (default), Std, or Asus. Std tries to use the kernel hotplug mechanism if available, while Asus tries to use dgpu_disable if available

**You must restart the service if you edit the config file**

**Changing hotplug_type requires a reboot to ensure correct state**, for example if you were in integrated mode with `hotplug_type = Asus` and changed to `hotplug_type = None` you would not have dGPU available until reboot.

#### Graphics switching notes

**ASUS G-Sync + ASUS GPU-MUX note:** Some ASUS laptops are capable of using the dGPU as the sole GPU in the system which is generally to enable g-sync on the laptop display panel. This is controlled by asusctl at this time, and may be added to supergfxd later. If mux/g-sync is enabled then supergfxd will halt itself until it is disabled again.

**vfio note:** The vfio modules *must not* be compiled into the kernel, they need
to be separate modules. If you don't plan to use vfio mode then you can ignore this
otherwise you may need a custom built kernel.
