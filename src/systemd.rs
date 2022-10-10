use crate::error::GfxError;
use log::info;
use std::process::Command;

/// An action for `systemctl`
#[derive(Debug, Copy, Clone)]
pub enum SystemdUnitAction {
    Stop,
    Start,
    Restart,
}

impl From<SystemdUnitAction> for &str {
    fn from(s: SystemdUnitAction) -> Self {
        match s {
            SystemdUnitAction::Stop => "stop",
            SystemdUnitAction::Start => "start",
            SystemdUnitAction::Restart => "restart",
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SystemdUnitState {
    Active,
    Inactive,
}

impl From<SystemdUnitState> for &str {
    fn from(s: SystemdUnitState) -> Self {
        match s {
            SystemdUnitState::Active => "active",
            SystemdUnitState::Inactive => "inactive",
        }
    }
}

/// Change the state of a systemd unit. Blocks while running command.
pub fn do_systemd_unit_action(action: SystemdUnitAction, unit: &str) -> Result<(), GfxError> {
    let mut cmd = Command::new("systemctl");
    cmd.arg(<&str>::from(action));
    cmd.arg(unit);
    info!("Running systemctl command {action:?} on {unit}");
    let status = cmd
        .status()
        .map_err(|err| GfxError::Command(format!("{:?}", cmd), err))?;
    if !status.success() {
        let msg = format!("systemctl {action:?} {unit} failed: {status:?}",);
        return Err(GfxError::SystemdUnitAction(msg));
    }
    Ok(())
}

/// Get systemd unit state. Blocks while command is run.
pub fn is_systemd_unit_state(state: SystemdUnitState, unit: &str) -> Result<bool, GfxError> {
    let mut cmd = Command::new("systemctl");
    cmd.arg("is-active");
    cmd.arg(unit);

    let output = cmd
        .output()
        .map_err(|err| GfxError::Command(format!("{:?}", cmd), err))?;
    if output.stdout.starts_with(<&str>::from(state).as_bytes()) {
        return Ok(true);
    }
    Ok(false)
}

/// Wait for a systemd unit to change to `state`. Checks state every 250ms for 3 seconds. Blocks while running wait.
pub fn wait_systemd_unit_state(state: SystemdUnitState, unit: &str) -> Result<(), GfxError> {
    let mut cmd = Command::new("systemctl");
    cmd.arg("is-active");
    cmd.arg(unit);

    let mut count = 0;

    while count <= (4 * 3) {
        // 3 seconds max
        let output = cmd
            .output()
            .map_err(|err| GfxError::Command(format!("{:?}", cmd), err))?;
        if output.stdout.starts_with(<&str>::from(state).as_bytes()) {
            return Ok(());
        }
        // fine to block here, nobody doing shit now
        std::thread::sleep(std::time::Duration::from_millis(250));
        count += 1;
    }
    Err(GfxError::SystemdUnitWaitTimeout(<&str>::from(state).into()))
}
