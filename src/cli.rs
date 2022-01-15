//! Basic CLI tool to control the `supergfxd` daemon

use std::{env::args, process::Command, sync::mpsc::channel};
use supergfxctl::{
    error::GfxError,
    gfx_vendors::{GfxMode, GfxRequiredUserAction},
    nvidia_drm_modeset,
    special_asus::{get_asus_gsync_gfx_mode, has_asus_gsync_gfx_mode},
    zbus_proxy::GfxProxy,
};

use gumdrop::Options;
use zbus::Connection;

#[derive(Default, Options)]
struct CliStart {
    #[options(help = "print help message")]
    help: bool,
    #[options(meta = "", help = "Set graphics mode")]
    mode: Option<GfxMode>,
    #[options(help = "Get supergfxd version")]
    version: bool,
    #[options(help = "Get the current mode")]
    get: bool,
    #[options(help = "Get the supported modes")]
    supported: bool,
    #[options(help = "Get the dGPU vendor name")]
    vendor: bool,
    #[options(help = "Get the current power status")]
    pow: bool,
    #[options(help = "Do not ask for confirmation")]
    force: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = args().skip(1).collect();

    match CliStart::parse_args_default(&args) {
        Ok(command) => {
            do_gfx(command).map_err(|err|{
                eprintln!("Graphics mode change error.");
                if !check_systemd_unit_enabled("supergfxd") {
                    eprintln!("\x1b[0;31msupergfxd is not enabled, enable it with `systemctl enable supergfxd\x1b[0m");
                } else if !check_systemd_unit_active("supergfxd") {
                    eprintln!("\x1b[0;31msupergfxd is not running, start it with `systemctl start supergfxd\x1b[0m");
                } else {
                    match &err {
                        GfxError::Zbus(err) => {
                            match err {
                                zbus::Error::MethodError(_,s,_) => {
                                    if let Some(text) = s {
                                        eprintln!("\x1b[0;31m{}\x1b[0m", text);
                                    } else {
                                        eprintln!("{}", err);
                                    }
                                }
                                _ => eprintln!("{}", err),
                            }
                        }
                        _ => eprintln!("{}", err),
                    }
                    eprintln!("Please check `journalctl -b -u supergfxd`, and `systemctl status supergfxd`");
                }
                err
            }).ok();
        }
        Err(err) => {
            eprintln!("source {}", err);
            std::process::exit(2);
        }
    }

    Ok(())
}

fn do_gfx(command: CliStart) -> Result<(), GfxError> {
    if command.mode.is_none()
        && !command.get
        && !command.version
        && !command.supported
        && !command.vendor
        && !command.pow
        && !command.force
        || command.help
    {
        println!("{}", command.self_usage());
    }

    let conn = Connection::new_system()?;
    let proxy = GfxProxy::new(&conn)?;

    let (tx, rx) = channel();
    proxy.connect_notify_action(tx)?;

    if let Some(mode) = command.mode {
        if has_asus_gsync_gfx_mode() && get_asus_gsync_gfx_mode()? == 1 {
            println!("You can not change modes until you turn dedicated/G-Sync off and reboot");
            std::process::exit(-1);
        }

        println!("If anything fails check `journalctl -b -u supergfxd`\n");

        proxy.write_mode(&mode)?;

        loop {
            proxy.next_signal()?;

            if let Ok(res) = rx.try_recv() {
                match res {
                    GfxRequiredUserAction::Integrated => {
                        println!(
                            "You must change to Integrated before you can change to {}",
                            <&str>::from(mode)
                        );
                    }
                    GfxRequiredUserAction::Logout | GfxRequiredUserAction::Reboot => {
                        if nvidia_drm_modeset()? {
                            println!("\x1b[0;31mRebootless mode requires nvidia-drm.modeset=0 to be set on the kernel cmdline\n\x1b[0m");
                        }
                        println!(
                            "Graphics mode changed to {}. User action required is: {}",
                            <&str>::from(mode),
                            <&str>::from(&res)
                        );
                    }
                    GfxRequiredUserAction::None => {
                        println!("Graphics mode changed to {}", <&str>::from(mode));
                    }
                }
            }
            std::process::exit(0)
        }
    }
    if command.version {
        let res = proxy.get_version()?;
        println!("supergfxd version: {}", res);
    }
    if command.get {
        let res = proxy.get_mode()?;
        println!("Current graphics mode: {}", <&str>::from(res));
    }
    if command.supported {
        let res = proxy.get_supported_modes()?;
        println!("Supported graphics modes: {:?}", res);
    }
    if command.vendor {
        let res = proxy.get_vendor()?;
        println!("dGPU vendor: {}", res);
    }
    if command.pow {
        let res = proxy.get_pwr()?;
        println!("Current power status: {}", <&str>::from(&res));
    }

    Ok(())
}

fn check_systemd_unit_active(name: &str) -> bool {
    if let Ok(out) = Command::new("systemctl")
        .arg("is-active")
        .arg(name)
        .output()
    {
        let buf = String::from_utf8_lossy(&out.stdout);
        return !buf.contains("inactive") && !buf.contains("failed");
    }
    false
}

fn check_systemd_unit_enabled(name: &str) -> bool {
    if let Ok(out) = Command::new("systemctl")
        .arg("is-enabled")
        .arg(name)
        .output()
    {
        let buf = String::from_utf8_lossy(&out.stdout);
        return buf.contains("enabled");
    }
    false
}
