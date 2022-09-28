use ::zbus::Connection;
use log::{error, info, warn};
use logind_zbus::manager::{ManagerProxy, SessionInfo};
use logind_zbus::session::{SessionClass, SessionProxy, SessionState, SessionType};
use std::time::Instant;
use std::{process::Command, time::Duration};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
};
use zbus::export::futures_util::lock::Mutex;

use crate::{
    config::create_modprobe_conf,
    error::GfxError,
    pci_device::{
        rescan_pci_bus, DiscreetGpu, GfxRequiredUserAction, GfxVendor, RuntimePowerManagement,
    },
    special_asus::{
        asus_dgpu_exists, asus_dgpu_set_disabled, asus_egpu_exists, asus_egpu_set_enabled,
        get_asus_gpu_mux_mode, has_asus_gpu_mux, AsusGpuMuxMode,
    },
    *,
};

use super::config::GfxConfig;

pub struct CtrlGraphics {
    pub(crate) dgpu: Arc<Mutex<DiscreetGpu>>,
    pub(crate) config: Arc<Mutex<GfxConfig>>,
    thread_exit: Arc<AtomicBool>,
}

impl CtrlGraphics {
    pub fn new(config: Arc<Mutex<GfxConfig>>) -> Result<CtrlGraphics, GfxError> {
        Ok(CtrlGraphics {
            dgpu: Arc::new(Mutex::new(DiscreetGpu::new()?)),
            config,
            thread_exit: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn dgpu_arc_clone(&self) -> Arc<Mutex<DiscreetGpu>> {
        self.dgpu.clone()
    }

    /// Force re-init of all state, including reset of device state
    pub async fn reload(&mut self) -> Result<(), GfxError> {
        let vfio_enable;
        let use_asus_dgpu_disable;
        let vfio_save;
        let compute_save;

        let mut config = self.config.lock().await;
        vfio_enable = config.vfio_enable;
        use_asus_dgpu_disable = config.asus_use_dgpu_disable;
        vfio_save = config.vfio_save;
        compute_save = config.compute_save;

        let mode = get_kernel_cmdline_mode()?
            .map(|mode| {
                warn!("Graphic mode {:?} set on kernel cmdline", mode);
                let mut save = true;
                if (matches!(mode, GfxMode::Compute) && !compute_save)
                    || (matches!(mode, GfxMode::Vfio) && !vfio_save || !vfio_enable)
                {
                    save = false;
                }

                if save {
                    config.mode = mode;
                    config.write();
                }
                mode
            })
            .unwrap_or(self.get_gfx_mode(&config)?);

        if matches!(mode, GfxMode::Vfio) && !vfio_enable {
            warn!("Tried to set vfio mode but it is not enabled");
            return Ok(());
        }

        if matches!(mode, GfxMode::Egpu) && !asus_egpu_exists() {
            warn!("Tried to set egpu mode but it is not supported");
            return Ok(());
        }

        let mut dgpu = self.dgpu.lock().await;
        Self::do_mode_setup_tasks(mode, vfio_enable, use_asus_dgpu_disable, &mut dgpu)?;

        info!("Reloaded gfx mode: {:?}", mode);
        Ok(())
    }

    /// Associated method to get which mode is set
    pub(crate) fn get_gfx_mode(&self, config: &GfxConfig) -> Result<GfxMode, GfxError> {
        if let Some(mode) = config.tmp_mode {
            return Ok(mode);
        }
        return Ok(config.mode);
    }

    ///
    pub(crate) async fn get_pending_mode(&self) -> GfxMode {
        let config = self.config.lock().await;
        if let Some(mode) = config.pending_mode {
            return mode;
        }
        GfxMode::None
    }

    ///
    pub(crate) async fn get_pending_user_action(&self) -> GfxRequiredUserAction {
        let config = self.config.lock().await;
        if let Some(action) = config.pending_action {
            return action;
        }
        GfxRequiredUserAction::None
    }

    /// Associated method to get list of supported modes
    pub(crate) async fn get_supported_modes(&self) -> Vec<GfxMode> {
        let mut list = vec![GfxMode::Integrated, GfxMode::Hybrid];

        let dgpu = self.dgpu.lock().await;
        if matches!(dgpu.vendor(), GfxVendor::Unknown) && !asus_dgpu_exists() {
            return vec![GfxMode::Integrated];
        }

        if dgpu.is_nvidia() {
            if asus_dgpu_exists() {
                let config = self.config.lock().await;
                if !config.asus_use_dgpu_disable {
                    list.push(GfxMode::Compute);
                }
            } else {
                list.push(GfxMode::Compute);
            }
        }

        let config = self.config.lock().await;
        if config.vfio_enable {
            list.push(GfxMode::Vfio);
        }

        if asus_egpu_exists() {
            list.push(GfxMode::Egpu);
        }

        list
    }

    /// Associated method to get which vendor the dgpu is from
    pub(crate) async fn get_gfx_vendor(&self) -> GfxVendor {
        let dgpu = self.dgpu.lock().await;
        dgpu.vendor()
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
            return Err(GfxError::DisplayManagerAction(msg));
        }
        Ok(())
    }

    async fn wait_display_manager_state(state: &str) -> Result<(), GfxError> {
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
            // fine to block here, nobody doing shit now
            std::thread::sleep(std::time::Duration::from_millis(250));
            count += 1;
        }
        Err(GfxError::DisplayManagerTimeout(state.into()))
    }

    /// Determine if we need to logout/thread. Integrated<->Vfio mode does not
    /// require logout.
    async fn mode_change_action(&self, vendor: GfxMode) -> GfxRequiredUserAction {
        let config = self.config.lock().await;
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
        if matches!(current, GfxMode::Hybrid) && matches!(vendor, GfxMode::Compute | GfxMode::Vfio)
        {
            return GfxRequiredUserAction::Integrated;
        }

        GfxRequiredUserAction::Logout
    }

    /// Do a full setup flow for the chosen mode:
    ///
    /// Tasks:
    /// - rescan for devices
    /// - write modprobe config
    ///   + add drivers
    ///   + or remove drivers and devices
    pub fn do_mode_setup_tasks(
        mode: GfxMode,
        vfio_enable: bool,
        use_asus_dgpu_disable: bool,
        devices: &mut DiscreetGpu,
    ) -> Result<(), GfxError> {
        if asus_dgpu_exists() && matches!(mode, GfxMode::Hybrid | GfxMode::Compute) {
            if use_asus_dgpu_disable
                && asus_dgpu_exists()
                && matches!(mode, GfxMode::Hybrid | GfxMode::Compute)
            {
                // re-enable the ASUS dgpu
                asus_dgpu_set_disabled(false)
                    .map_err(|e| {
                        warn!("Re-enable ASUS dGPU failed: {e}");
                        e
                    })
                    .unwrap();
            } else {
                match asus_dgpu_disabled() {
                    Ok(disabled) => {
                        error!("dgpu_disable is {disabled} and config option use_asus_dgpu_disable is {use_asus_dgpu_disable}, can't set {mode:?}");
                        return Err(GfxError::DgpuNotFound);
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Rescan before doing remove or add drivers
        rescan_pci_bus()?;
        devices.set_runtime_pm(RuntimePowerManagement::Auto)?;

        match DiscreetGpu::new() {
            Ok(dev) => *devices = dev,
            Err(e) => warn!("loop: tried to reset Unknown dgpu status/devices: {e:?}"),
        }

        create_modprobe_conf(mode, devices)?;

        match mode {
            GfxMode::Hybrid | GfxMode::Compute => {
                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, "rmmod")?;
                    }
                }
                if asus_egpu_exists() {
                    asus_egpu_set_enabled(false)?;
                }
                if asus_dgpu_exists() {
                    asus_dgpu_set_disabled(false)?;
                }
                devices.do_driver_action("modprobe")?;
                // TODO: Need to enable or disable on AC status
                toggle_nvidia_powerd(true, devices.vendor())?;
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
                toggle_nvidia_powerd(false, devices.vendor())?;
                kill_nvidia_lsof()?;
                devices.do_driver_action("rmmod")?;
                //devices.unbind_remove()?;

                // This can only be done *after* the drivers are removed or a
                // hardlock will be caused
                if asus_dgpu_exists() {
                    asus_dgpu_set_disabled(true)?;
                }
                if asus_egpu_exists() {
                    asus_egpu_set_enabled(false)?;
                }
            }
            GfxMode::Egpu => {
                if !asus_egpu_exists() {
                    warn!("eGPU mode attempted while unsupported by this machine");
                    return Err(GfxError::NotSupported(
                        "eGPU mode not supported".to_string(),
                    ));
                }

                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, "rmmod")?;
                    }
                }

                asus_egpu_set_enabled(true)?;
                devices.do_driver_action("modprobe")?;
                toggle_nvidia_powerd(true, devices.vendor())?;
            }
            GfxMode::None => {}
        }
        Ok(())
    }

    /// Check if the user has any graphical uiser sessions that are active or online
    async fn graphical_user_sessions_exist(
        connection: &Connection,
        sessions: &[SessionInfo],
    ) -> Result<bool, GfxError> {
        for session in sessions {
            let session_proxy = SessionProxy::builder(connection)
                .path(session.path())?
                .build()
                .await?;
            if session_proxy.class().await? == SessionClass::User {
                match session_proxy.type_().await? {
                    SessionType::X11 | SessionType::Wayland | SessionType::MIR => {
                        match session_proxy.state().await? {
                            SessionState::Online | SessionState::Active => return Ok(true),
                            SessionState::Closing => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(false)
    }

    /// Spools until all user sessions are ended then switches to requested mode
    async fn mode_change_loop(
        mode: GfxMode,
        device: Arc<Mutex<DiscreetGpu>>,
        thread_stop: Arc<AtomicBool>,
        config: Arc<Mutex<GfxConfig>>,
    ) -> Result<String, GfxError> {
        info!("display-manager thread started");
        let no_logind;
        let use_asus_dgpu_disable;
        let logout_timeout_s;

        const SLEEP_PERIOD: Duration = Duration::from_millis(100);
        let start_time = Instant::now();

        let connection = Connection::system().await?;
        let manager = ManagerProxy::new(&connection).await?;
        let mut sessions = manager.list_sessions().await?;

        // Don't wait on logind stuff if set
        {
            let config = config.lock().await;
            no_logind = config.no_logind;
            use_asus_dgpu_disable = config.asus_use_dgpu_disable;
            logout_timeout_s = config.logout_timeout_s;
            info!("logout_timeout_s = {}", logout_timeout_s);
        }

        if no_logind {
            info!("no_logind option is set");
        }

        while !no_logind {
            if thread_stop.load(Ordering::SeqCst) {
                info!("Thread forced to exit");
                thread_stop.store(false, Ordering::Release);
                return Ok("Exited".to_string());
            }

            let tmp = manager.list_sessions().await?;
            if !tmp.iter().eq(&sessions) {
                info!("GFX thread: Sessions list changed");
                sessions = tmp;
            }

            if !Self::graphical_user_sessions_exist(&connection, &sessions).await? {
                break;
            }

            // exit if 3 minutes pass
            if logout_timeout_s != 0
                && Instant::now().duration_since(start_time).as_secs() > logout_timeout_s
            {
                let detail = format!("Time ({} seconds) for logout exceeded", logout_timeout_s);
                warn!("{}", detail);
                return Err(GfxError::DisplayManagerTimeout(detail));
            }

            // Don't spin at max speed
            //sleep(SLEEP_PERIOD).await;
        }

        let mut device = device.lock().await;
        if !no_logind {
            info!("GFX thread: all graphical user sessions ended, continuing");
            Self::do_display_manager_action("stop")?;
            Self::wait_display_manager_state("inactive").await?;
        }

        let mut mode_to_save = mode;
        // Need to change to integrated before we can change to vfio or compute
        // Since we have a lock, reset tmp to none. This thread should only ever run
        // for Integrated, Hybrid, or Nvidia. Tmp is also only for informational
        let mut config = config.lock().await;
        config.tmp_mode = None;
        //
        let vfio_enable = config.vfio_enable;

        // Failsafe. In the event this loop is run with a switch from nvidia in use
        // to vfio or compute do a forced switch to integrated instead to prevent issues
        if matches!(mode, GfxMode::Compute | GfxMode::Vfio)
            && matches!(config.mode, GfxMode::Hybrid)
        {
            Self::do_mode_setup_tasks(
                GfxMode::Integrated,
                vfio_enable,
                use_asus_dgpu_disable,
                &mut device,
            )?;
            if !no_logind {
                Self::do_display_manager_action("restart")?;
            }
            mode_to_save = GfxMode::Integrated;
        } else {
            Self::do_mode_setup_tasks(mode, vfio_enable, use_asus_dgpu_disable, &mut device)?;
            if !no_logind {
                Self::do_display_manager_action("restart")?;
            }
        }

        // Save selected mode in case of reboot
        config.mode = mode_to_save;
        config.write();

        info!("GFX thread: display-manager started");

        let v: &str = mode.into();
        info!("GFX thread: Graphics mode changed to {} successfully", v);
        Ok(format!("Graphics mode changed to {} successfully", v))
    }

    /// The thread is used only in cases where a logout is required
    async fn setup_mode_change_thread(&mut self, mode: GfxMode) {
        // First, stop all threads
        self.thread_exit.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(100));
        self.thread_exit.store(false, Ordering::Release);

        {
            let mut config = self.config.lock().await;
            config.pending_mode = Some(mode);
        }

        Self::mode_change_loop(
            mode,
            self.dgpu.clone(),
            self.thread_exit.clone(),
            self.config.clone(),
        )
        .await
        .map_err(|err| {
            error!("Loop error: {}", err);
        })
        .ok();

        let mut config = self.config.lock().await;
        config.pending_mode = None;
        config.pending_action = None;
    }

    /// Initiates a mode change by starting a thread that will wait until all
    /// graphical sessions are exited before performing the tasks required
    /// to switch modes.
    ///
    /// For manually calling (not on boot/startup) via dbus
    pub async fn set_gfx_mode(&mut self, mode: GfxMode) -> Result<GfxRequiredUserAction, GfxError> {
        self.thread_exit.store(false, Ordering::Release);
        if has_asus_gpu_mux() {
            if let Ok(mux) = get_asus_gpu_mux_mode() {
                if mux == AsusGpuMuxMode::Dedicated {
                    warn!("ASUS GPU MUX is in discreet mode");
                    return Ok(GfxRequiredUserAction::AsusGpuMuxDisable);
                }
            }
        }

        let use_asus_dgpu_disable;
        let vfio_enable;
        {
            let config = self.config.lock().await;
            vfio_enable = config.vfio_enable;
            use_asus_dgpu_disable = config.asus_use_dgpu_disable;
        }

        if !vfio_enable && matches!(mode, GfxMode::Vfio) {
            return Err(GfxError::VfioDisabled);
        }

        {
            let dgpu = self.dgpu.lock().await;
            mode_support_check(&mode, &dgpu)?;
        }

        // determine which method we need here
        let action_required = self.mode_change_action(mode).await;

        match action_required {
            GfxRequiredUserAction::Logout => {
                info!("mode change requires a logout to complete");
                {
                    let mut config = self.config.lock().await;
                    config.pending_action = Some(action_required);
                }
                self.setup_mode_change_thread(mode).await;
            }
            GfxRequiredUserAction::Integrated => {
                info!("mode change requires user to be in Integrated mode first");
            }
            // Generally None for vfio, compute, integrated only
            GfxRequiredUserAction::None => {
                info!("mode change does not require logout");
                let mut dgpu = self.dgpu.lock().await;
                Self::do_mode_setup_tasks(mode, vfio_enable, use_asus_dgpu_disable, &mut dgpu)?;
                info!("Graphics mode changed to {}", <&str>::from(mode));
                let mut config = self.config.lock().await;
                config.tmp_mode = None;
                config.pending_action = None;
                config.pending_mode = None;

                if (matches!(mode, GfxMode::Vfio) && config.vfio_save)
                    || matches!(mode, GfxMode::Compute) && config.compute_save
                {
                    config.mode = mode;
                    config.write();
                } else if matches!(mode, GfxMode::Vfio | GfxMode::Compute) {
                    config.tmp_mode = Some(mode);
                }
            }
            GfxRequiredUserAction::AsusGpuMuxDisable => {}
        }
        return Ok(action_required);
    }
}
