use ::zbus::Connection;
use log::{error, info, warn};
use logind_zbus::{
    types::{SessionClass, SessionInfo, SessionState, SessionType},
    ManagerProxy, SessionProxy,
};
use std::time::Instant;
use std::{process::Command, thread::sleep, time::Duration};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    sync::Mutex,
};

use crate::{
    config::{create_modprobe_conf, create_xorg_conf},
    error::GfxError,
    gfx_devices::DiscreetGpu,
    gfx_vendors::GfxVendor,
    pci_device::{rescan_pci_bus, RuntimePowerManagement},
    special_asus::{
        asus_egpu_exists, asus_egpu_set_status, get_asus_gsync_gfx_mode, has_asus_gsync_gfx_mode,
        is_gpu_enabled,
    },
    *,
};

use super::config::GfxConfig;
use super::gfx_vendors::{GfxMode, GfxRequiredUserAction};

const THREAD_TIMEOUT_MSG: &str = "thread time exceeded 3 minutes, exiting";

pub struct CtrlGraphics {
    dgpu: DiscreetGpu,
    config: Arc<Mutex<GfxConfig>>,
    thread_exit: Arc<AtomicBool>,
}

impl CtrlGraphics {
    pub fn new(config: Arc<Mutex<GfxConfig>>) -> Result<CtrlGraphics, GfxError> {
        Ok(CtrlGraphics {
            dgpu: DiscreetGpu::new()?,
            config,
            thread_exit: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn dgpu(&self) -> &DiscreetGpu {
        &self.dgpu
    }

    /// Force re-init of all state, including reset of device state
    pub fn reload(&mut self) -> Result<(), GfxError> {
        let vfio_enable = if let Ok(config) = self.config.try_lock() {
            config.vfio_enable
        } else {
            false
        };

        Self::do_mode_setup_tasks(self.get_gfx_mode()?, vfio_enable, &self.dgpu)?;

        info!("Reloaded gfx mode: {:?}", self.get_gfx_mode()?);
        Ok(())
    }

    /// Save the selected `Mode` to config
    fn save_gfx_mode(mode: GfxMode, config: Arc<Mutex<GfxConfig>>) {
        if let Ok(mut config) = config.lock() {
            config.mode = mode;
            config.write();
        }
    }

    /// Associated method to get which mode is set
    pub(crate) fn get_gfx_mode(&self) -> Result<GfxMode, GfxError> {
        if let Ok(config) = self.config.lock() {
            if let Some(mode) = config.tmp_mode {
                return Ok(mode);
            }
            return Ok(config.mode);
        }
        Err(GfxError::ParseVendor)
    }

    /// Associated method to get list of supported modes
    pub(crate) fn get_supported_modes(&self) -> Vec<GfxMode> {
        if matches!(self.dgpu.vendor(), GfxVendor::Unknown) {
            return vec![GfxMode::Integrated];
        }

        let mut list = vec![GfxMode::Integrated, GfxMode::Hybrid];

        if self.dgpu.is_nvidia() {
            list.push(GfxMode::Dedicated);
            list.push(GfxMode::Compute);
        }

        if let Ok(config) = self.config.lock() {
            if config.vfio_enable {
                list.push(GfxMode::Vfio);
            }
        }

        if asus_egpu_exists() {
            list.push(GfxMode::Egpu);
        }

        list
    }

    /// Associated method to get which vendor the dgpu is from
    pub(crate) fn get_gfx_vendor(&self) -> GfxVendor {
        self.dgpu.vendor()
    }

    fn do_display_manager_action(action: &str) -> Result<(), GfxError> {
        let mut cmd = Command::new("systemctl");
        cmd.arg(action);
        cmd.arg(DISPLAY_MANAGER);

        let status = cmd
            .status()
            .map_err(|err| GfxError::Command(format!("{:?}", cmd), err))?;
        if !status.success() {
            let msg = format!(
                "systemctl {} {} failed: {:?}",
                action, DISPLAY_MANAGER, status
            );
            return Err(GfxError::DisplayManagerAction(msg, status));
        }
        Ok(())
    }

    fn wait_display_manager_state(state: &str) -> Result<(), GfxError> {
        let mut cmd = Command::new("systemctl");
        cmd.arg("is-active");
        cmd.arg(DISPLAY_MANAGER);

        let mut count = 0;

        while count <= (4 * 3) {
            // 3 seconds max
            let output = cmd
                .output()
                .map_err(|err| GfxError::Command(format!("{:?}", cmd), err))?;
            if output.stdout.starts_with(state.as_bytes()) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
            count += 1;
        }
        Err(GfxError::DisplayManagerTimeout(state.into()))
    }

    /// Determine if we need to logout/thread. Integrated<->Vfio mode does not
    /// require logout.
    fn mode_change_action(&self, vendor: GfxMode) -> GfxRequiredUserAction {
        if nvidia_drm_modeset()
            .map_err(|e| {
                error!("mode_change_action error: {}", e);
                e
            })
            .unwrap_or(false)
        {
            return GfxRequiredUserAction::Reboot;
        }

        if let Ok(config) = self.config.lock() {
            if config.always_reboot {
                return GfxRequiredUserAction::Reboot;
            }

            let current = config.mode;
            // Modes that can switch without logout
            if matches!(
                current,
                GfxMode::Integrated | GfxMode::Vfio | GfxMode::Compute
            ) && matches!(
                vendor,
                GfxMode::Integrated | GfxMode::Vfio | GfxMode::Compute
            ) {
                return GfxRequiredUserAction::None;
            }
            // Modes that require a switch to integrated first
            if matches!(current, GfxMode::Dedicated | GfxMode::Hybrid)
                && matches!(vendor, GfxMode::Compute | GfxMode::Vfio)
            {
                return GfxRequiredUserAction::Integrated;
            }
        }
        GfxRequiredUserAction::Logout
    }

    /// Do a full setup flow for the chosen mode:
    ///
    /// Tasks:
    /// - rescan for devices
    /// - write xorg config
    /// - write modprobe config
    ///   + add drivers
    ///   + or remove drivers and devices
    ///
    /// The daemon needs direct access to this function when it detects that the
    /// bios has G-Sync switch is enabled
    pub fn do_mode_setup_tasks(
        mode: GfxMode,
        vfio_enable: bool,
        devices: &DiscreetGpu,
    ) -> Result<(), GfxError> {
        // Rescan before doing remove or add drivers
        rescan_pci_bus()?;
        devices.set_runtime_pm(RuntimePowerManagement::Auto)?;
        // Only these modes should have xorg config
        if matches!(
            mode,
            GfxMode::Dedicated | GfxMode::Hybrid | GfxMode::Integrated
        ) {
            create_xorg_conf(mode, devices)?;
        }

        // Write different modprobe to enable boot control to work
        create_modprobe_conf(mode, devices)?;

        match mode {
            GfxMode::Dedicated | GfxMode::Hybrid | GfxMode::Compute => {
                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, "rmmod")?;
                    }
                }
                if asus_egpu_exists() {
                    asus_egpu_set_status(false)?;
                }
                devices.do_driver_action("modprobe")?;
            }
            GfxMode::Vfio => {
                if vfio_enable {
                    do_driver_action("nouveau", "rmmod")?;
                    devices.do_driver_action("rmmod")?;
                    devices.unbind()?;
                    do_driver_action("vfio-pci", "modprobe")?;
                } else {
                    return Err(GfxError::VfioDisabled);
                }
            }
            GfxMode::Integrated => {
                do_driver_action("nouveau", "rmmod")?;
                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, "rmmod")?;
                    }
                }
                devices.do_driver_action("rmmod")?;
                devices.unbind_remove()?;
                // This can only be done *after* the drivers are removed or a
                // hardlock will be caused
                if asus_egpu_exists() {
                    asus_egpu_set_status(false)?;
                }
            }
            GfxMode::Egpu => {
                if !asus_egpu_exists() {
                    warn!("eGPU mode attempted while unsupported by this machine");
                    return Ok(());
                }

                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, "rmmod")?;
                    }
                }

                asus_egpu_set_status(true)?;

                devices.do_driver_action("modprobe")?;
            }
        }
        Ok(())
    }

    /// Check if the user has any graphical uiser sessions that are active or online
    fn graphical_user_sessions_exist(
        connection: &Connection,
        sessions: &[SessionInfo],
    ) -> Result<bool, GfxError> {
        for session in sessions {
            let session_proxy = SessionProxy::new(connection, session)?;
            if session_proxy.get_class()? == SessionClass::User {
                match session_proxy.get_type()? {
                    SessionType::X11 | SessionType::Wayland | SessionType::MIR => {
                        match session_proxy.get_state()? {
                            SessionState::Online | SessionState::Active => return Ok(true),
                            SessionState::Closing | SessionState::Invalid => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(false)
    }

    /// Spools until all user sessions are ended then switches to requested mode
    fn mode_change_loop(
        mode: GfxMode,
        devices: DiscreetGpu,
        thread_stop: Arc<AtomicBool>,
        config: Arc<Mutex<GfxConfig>>,
    ) -> Result<String, GfxError> {
        info!("display-manager thread started");

        const SLEEP_PERIOD: Duration = Duration::from_millis(100);
        let start_time = Instant::now();

        let connection = Connection::new_system()?;
        let manager = ManagerProxy::new(&connection)?;
        let mut sessions = manager.list_sessions()?;

        loop {
            if thread_stop.load(Ordering::SeqCst) {
                info!("Thread forced to exit");
                return Ok("Exited".to_string());
            }
            let tmp = manager.list_sessions()?;
            if !tmp.iter().eq(&sessions) {
                info!("GFX thread: Sessions list changed");
                sessions = tmp;
            }

            if !Self::graphical_user_sessions_exist(&connection, &sessions)? {
                break;
            }

            // exit if 3 minutes pass
            if Instant::now().duration_since(start_time).as_secs() > 180 {
                warn!("{}", THREAD_TIMEOUT_MSG);
                return Err(GfxError::DisplayManagerTimeout(THREAD_TIMEOUT_MSG.into()));
            }

            // Don't spin at max speed
            sleep(SLEEP_PERIOD);
        }

        info!("GFX thread: all graphical user sessions ended, continuing");
        Self::do_display_manager_action("stop")?;
        Self::wait_display_manager_state("inactive")?;

        let mut mode_to_save = mode;
        // Need to change to integrated before we can change to vfio or compute
        if let Ok(mut config) = config.try_lock() {
            // Since we have a lock, reset tmp to none. This thread should only ever run
            // for Integrated, Hybrid, or Nvidia. Tmp is also only for informational
            config.tmp_mode = None;
            //
            let vfio_enable = config.vfio_enable;

            // Failsafe. In the event this loop is run with a switch from nvidia in use
            // to vfio or compute do a forced switch to integrated instead to prevent issues
            if matches!(mode, GfxMode::Compute | GfxMode::Vfio)
                && matches!(config.mode, GfxMode::Dedicated | GfxMode::Hybrid)
            {
                Self::do_mode_setup_tasks(GfxMode::Integrated, vfio_enable, &devices)?;
                Self::do_display_manager_action("restart")?;
                mode_to_save = GfxMode::Integrated;
            } else {
                Self::do_mode_setup_tasks(mode, vfio_enable, &devices)?;
                Self::do_display_manager_action("restart")?;
            }
        }

        // Save selected mode in case of reboot
        Self::save_gfx_mode(mode_to_save, config);
        info!("GFX thread: display-manager started");

        let v: &str = mode.into();
        info!("GFX thread: Graphics mode changed to {} successfully", v);
        Ok(format!("Graphics mode changed to {} successfully", v))
    }

    /// The thread is used only in cases where a logout is required
    fn setup_mode_change_thread(&mut self, mode: GfxMode) {
        // First, stop all threads
        self.thread_exit.store(true, Ordering::Release);
        // Give threads a chance to read
        std::thread::sleep(Duration::from_secs(1));
        // then reset
        self.thread_exit.store(false, Ordering::Release);

        let config = self.config.clone();
        let devices = self.dgpu.clone();
        let rx = self.thread_exit.clone();

        std::thread::spawn(move || {
            Self::mode_change_loop(mode, devices, rx, config)
                .map_err(|err| {
                    error!("{}", err);
                })
                .ok();
        });
    }

    /// Initiates a mode change by starting a thread that will wait until all
    /// graphical sessions are exited before performing the tasks required
    /// to switch modes.
    ///
    /// For manually calling (not on boot/startup) via dbus
    pub fn set_gfx_mode(&mut self, mode: GfxMode) -> Result<GfxRequiredUserAction, GfxError> {
        if has_asus_gsync_gfx_mode() {
            if let Ok(gsync) = get_asus_gsync_gfx_mode() {
                if gsync == 1 {
                    return Err(GfxError::AsusGsyncModeActive);
                }
            }
        }

        if !self.get_supported_modes().contains(&mode) {
            is_gpu_enabled()?;
        }

        let vfio_enable = if let Ok(config) = self.config.try_lock() {
            config.vfio_enable
        } else {
            false
        };

        if !vfio_enable && matches!(mode, GfxMode::Vfio) {
            return Err(GfxError::VfioDisabled);
        }

        mode_support_check(&mode, &self.dgpu)?;

        // determine which method we need here
        let action_required = self.mode_change_action(mode);

        match action_required {
            GfxRequiredUserAction::Logout => {
                info!("mode change requires a logout to complete");
                self.setup_mode_change_thread(mode);
            }
            GfxRequiredUserAction::Reboot => {
                info!("mode change requires reboot");
                if let Ok(mut config) = self.config.lock() {
                    config.mode = mode;
                    config.write();
                }
            }
            GfxRequiredUserAction::Integrated => {
                info!("mode change requires user to be in Integrated mode first");
            }
            GfxRequiredUserAction::None => {
                info!("mode change does not require logout");
                Self::do_mode_setup_tasks(mode, vfio_enable, &self.dgpu)?;
                info!("Graphics mode changed to {}", <&str>::from(mode));
                if let Ok(mut config) = self.config.try_lock() {
                    config.tmp_mode = None;
                    if matches!(mode, GfxMode::Vfio | GfxMode::Compute) {
                        config.tmp_mode = Some(mode);
                    }
                    if (matches!(mode, GfxMode::Vfio) && config.vfio_save)
                        || matches!(mode, GfxMode::Compute) && config.compute_save
                    {
                        config.mode = mode;
                        config.write();
                    }
                }
            }
        }

        Ok(action_required)
    }
}
