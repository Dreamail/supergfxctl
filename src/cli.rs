//! Basic CLI tool to control the `supergfxd` daemon

use std::{env::args, process::Command};
use supergfxctl::{
    actions::UserActionRequired, error::GfxError, pci_device::GfxMode,
    zbus_proxy::DaemonProxyBlocking,
};

use gumdrop::Options;
use zbus::{blocking::Connection, proxy::CacheProperties};

#[derive(Default, Clone, Copy, Options)]
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
    status: bool,
    #[options(help = "Get the pending user action if any")]
    pend_action: bool,
    #[options(help = "Get the pending mode change if any")]
    pend_mode: bool,
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
                    eprintln!("Please check `journalctl -b -u supergfxd`, and `systemctl status supergfxd`");
                    if let GfxError::Zbus(zbus::Error::MethodError(_,Some(text),_)) = &err {
                                eprintln!("\x1b[0;31m{}\x1b[0m", text);
                                std::process::exit(1);
                    }
                }
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }).ok();
        }
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
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
        && !command.status
        && !command.pend_action
        && !command.pend_mode
        || command.help
    {
        println!("{}", command.self_usage());
    }

    let proxy = DaemonProxyBlocking::builder(&Connection::system()?)
        .cache_properties(CacheProperties::No)
        .build()?;

    if let Some(mode) = command.mode {
        let res = proxy.set_mode(&mode)?;
        match res {
            UserActionRequired::SwitchToIntegrated => {
                eprintln!("You must change to Integrated before you can change to {mode}",);
                std::process::exit(1);
            }
            UserActionRequired::Logout => {
                println!(
                    "Graphics mode changed to {mode}. Required user action is: {}",
                    <&str>::from(res)
                );
            }
            UserActionRequired::Nothing => {
                println!("Graphics mode changed to {mode}");
            }

            UserActionRequired::Reboot => {
                println!("A reboot is required to complete the mode change")
            }
            UserActionRequired::AsusEgpuDisable => println!("{res:?}"),
        }
    }

    if command.version {
        let res = proxy.version()?;
        println!("{}", res);
    }
    if command.get {
        let res = proxy.mode()?;
        println!("{res}");
    }
    if command.supported {
        let res = proxy.supported()?;
        println!("{:?}", res);
    }
    if command.vendor {
        let res = proxy.vendor()?;
        println!("{}", res);
    }
    if command.status {
        let res = proxy.power()?;
        println!("{}", <&str>::from(&res));
    }
    if command.pend_action {
        let res = proxy.pending_user_action()?;
        println!("{}", <&str>::from(&res));
    }
    if command.pend_mode {
        let res = proxy.pending_mode()?;
        println!("{res}");
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
