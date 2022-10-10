use ::zbus::Connection;
use log::{debug, error, info, warn};
use logind_zbus::manager::{ManagerProxy, SessionInfo};
use logind_zbus::session::{SessionClass, SessionProxy, SessionState, SessionType};
use std::time::Duration;
use std::time::Instant;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
};
use tokio::time::sleep;
use zbus::export::futures_util::lock::Mutex;

use crate::pci_device::{rescan_pci_bus, HotplugState};
use crate::systemd::{SystemdUnitAction, SystemdUnitState};
use crate::{
    config::create_modprobe_conf,
    error::GfxError,
    pci_device::{DiscreetGpu, GfxRequiredUserAction, GfxVendor, RuntimePowerManagement},
    special_asus::{
        asus_dgpu_exists, asus_dgpu_set_disabled, asus_egpu_exists, asus_egpu_set_enabled,
        get_asus_gpu_mux_mode, has_asus_gpu_mux, AsusGpuMuxMode,
    },
    systemd::{do_systemd_unit_action, wait_systemd_unit_state},
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
        let mut config = self.config.lock().await;
        let vfio_enable = config.vfio_enable;
        let asus_use_dgpu_disable = config.asus_use_dgpu_disable;
        let vfio_save = config.vfio_save;
        let compute_save = config.compute_save;

        // This is a bit shit but I'm not sure of the best way to handle
        // dgpu_disable not being available when required...
        if asus_use_dgpu_disable && !asus_dgpu_exists() {
            if !create_asus_modules_load_conf()? {
                warn!(
                    "reload: Reboot required due to {} creation",
                    ASUS_MODULES_LOAD_PATH
                );
                // let mut cmd = Command::new("reboot");
                // cmd.spawn()?;
            }
            warn!("reload: asus_use_dgpu_disable is set but asus-wmi appear not loaded yet. Trying for 3 seconds. If there are issues you may need to add asus_nb_wmi to modules.load.d");
            let mut count = 3000 / 50;
            while !asus_dgpu_exists() && count != 0 {
                sleep(Duration::from_millis(50)).await;
                count -= 1;
            }
        }

        if has_asus_gpu_mux() {
            if let Ok(mux_mode) = get_asus_gpu_mux_mode() {
                if mux_mode == AsusGpuMuxMode::Discreet {
                    let dgpu = self.dgpu.lock().await;
                    create_modprobe_conf(GfxMode::Hybrid, &dgpu)?;

                    info!("reload: ASUS GPU MUX is in discreet mode");
                    if asus_dgpu_exists() {
                        if let Ok(d) = asus_dgpu_disabled() {
                            if d {
                                error!("reload: dgpu_disable is on while gpu_mux_mode is descrete, can't continue safely, attempting to set dgpu_disable off");
                                asus_dgpu_set_disabled(false)?;
                                panic!("reload: dgpu_disable is on while gpu_mux_mode is descrete, can't continue safely. Check logs");
                            } else {
                                info!("reload: dgpu_disable is off");
                            }
                        }
                    }
                    return Ok(());
                }
            }
        }

        if asus_use_dgpu_disable && asus_dgpu_exists() {
            warn!("Has ASUS dGPU and dgpu_disable, toggling hotplug power off/on to prep");
            let dgpu = self.dgpu.lock().await;
            dgpu.set_hotplug(HotplugState::Off)?;
            dgpu.set_hotplug(HotplugState::On)?;
        }

        let mode = get_kernel_cmdline_mode()?
            .map(|mode| {
                warn!("reload: Graphic mode {:?} set on kernel cmdline", mode);
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
            warn!("reload: Tried to set vfio mode but it is not enabled");
            return Ok(());
        }

        if matches!(mode, GfxMode::Egpu) && !asus_egpu_exists() {
            warn!("reload: Tried to set egpu mode but it is not supported");
            return Ok(());
        }

        let mut dgpu = self.dgpu.lock().await;
        Self::do_mode_setup_tasks(mode, vfio_enable, asus_use_dgpu_disable, &mut dgpu)?;
        // Self::mode_change_loop(
        //     mode,
        //     self.dgpu.clone(),
        //     self.thread_exit.clone(),
        //     self.config.clone(),
        // )
        // .await
        // .map_err(|err| {
        //     error!("Loop error: {}", err);
        // })
        // .ok();

        info!("reload: Reloaded gfx mode: {:?}", mode);
        Ok(())
    }

    /// Associated method to get which mode is set
    pub(crate) fn get_gfx_mode(&self, config: &GfxConfig) -> Result<GfxMode, GfxError> {
        if let Some(mode) = config.tmp_mode {
            return Ok(mode);
        }
        Ok(config.mode)
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

    /// Check if the user has any graphical uiser sessions that are active or online
    async fn graphical_user_sessions_exist(
        connection: &Connection,
        sessions: &[SessionInfo],
    ) -> Result<bool, GfxError> {
        for session in sessions {
            // should ignore error such as:
            // Zbus error: org.freedesktop.DBus.Error.UnknownObject: Unknown object '/org/freedesktop/login1/session/c2'
            if let Ok(session_proxy) = SessionProxy::builder(connection)
                .path(session.path())?
                .build()
                .await
                .map_err(|e| warn!("graphical_user_sessions_exist: builder: {e:?}"))
            {
                let class = session_proxy.class().await.map_err(|e| {
                    warn!("graphical_user_sessions_exist: class: {e:?}");
                    e
                })?;
                if class == SessionClass::User {
                    match session_proxy.type_().await.map_err(|e| {
                        warn!("graphical_user_sessions_exist: type_: {e:?}");
                        e
                    })? {
                        SessionType::X11 | SessionType::Wayland | SessionType::MIR => {
                            match session_proxy.state().await.map_err(|e| {
                                warn!("graphical_user_sessions_exist: state: {e:?}");
                                e
                            })? {
                                SessionState::Online | SessionState::Active => return Ok(true),
                                SessionState::Closing => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(false)
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

    fn do_rescan(
        mode: GfxMode,
        asus_use_dgpu_disable: bool,
        devices: &mut DiscreetGpu,
    ) -> Result<(), GfxError> {
        // Don't do a rescan unless the dev list is empty. This might be the case if
        // asus dgpu_disable is set before the daemon starts. But in general the daemon
        // should have the correct device on boot and retain that.
        let mut do_find_device = devices.devices().is_empty();
        for dev in devices.devices() {
            if dev.is_dgpu() {
                do_find_device = false;
                break;
            }
            do_find_device = true;
        }

        if do_find_device {
            info!("do_rescan: Device rescan required");
            if asus_dgpu_exists() {
                debug!("do_rescan: ASUS dgpu_disable found");
                if asus_use_dgpu_disable && matches!(mode, GfxMode::Hybrid | GfxMode::Compute) {
                    // re-enable the ASUS dgpu
                    // Ignore the error if there is one. Sometimes the kernel causes an I/O error and I'm
                    // not sure why yet. But the dgpu seems to change..
                    asus_dgpu_set_disabled(false)
                        .map_err(|e| {
                            warn!("do_rescan: Re-enable ASUS dGPU failed: {e}");
                        })
                        .ok();
                } else if !asus_use_dgpu_disable {
                    match asus_dgpu_disabled() {
                        Ok(disabled) => {
                            error!("do_rescan: dgpu_disable is {disabled} and config option asus_use_dgpu_disable is {asus_use_dgpu_disable}, can't set {mode:?}");
                            return Err(GfxError::DgpuNotFound);
                        }
                        Err(e) => return Err(e),
                    }
                }
            }

            match DiscreetGpu::new() {
                Ok(dev) => *devices = dev,
                Err(e) => warn!("do_rescan: tried to reset Unknown dgpu status/devices: {e:?}"),
            }
        } else {
            info!("do_rescan: Rescanning PCI bus");
            rescan_pci_bus()?; // should force re-attach of driver
        }

        Ok(())
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
        asus_use_dgpu_disable: bool,
        devices: &mut DiscreetGpu,
    ) -> Result<(), GfxError> {
        debug!("do_mode_setup_tasks(mode:{mode:?}, vfio_enable:{vfio_enable}, asus_use_dgpu_disable: {asus_use_dgpu_disable})");
        let debug = |s: &str| debug!("do_mode_setup_tasks: {s}");
        let warn = |s: &str| warn!("do_mode_setup_tasks: {s}");

        create_modprobe_conf(mode, devices)?;
        Self::do_rescan(mode, asus_use_dgpu_disable, devices)?;

        match mode {
            GfxMode::Hybrid | GfxMode::Compute => {
                debug("Mode match: GfxMode::Hybrid | GfxMode::Compute");
                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, "rmmod")?;
                    }
                }
                devices.set_hotplug(HotplugState::On)?;
                if asus_egpu_exists() {
                    asus_egpu_set_enabled(false)?;
                }
                if asus_dgpu_exists() && asus_use_dgpu_disable {
                    asus_dgpu_set_disabled(false)?;
                }
                devices.do_driver_action("modprobe")?;
                // TODO: Need to enable or disable on AC status
                toggle_nvidia_powerd(true, devices.vendor())?;
            }
            GfxMode::Vfio => {
                debug!("Mode match: GfxMode::Vfio");
                if vfio_enable {
                    toggle_nvidia_powerd(false, devices.vendor())?;
                    kill_nvidia_lsof()?;
                    devices.unbind()?;
                    devices.do_driver_action("rmmod")?;
                    do_driver_action("nouveau", "rmmod")?;
                    do_driver_action("vfio-pci", "modprobe")?;
                } else {
                    return Err(GfxError::VfioDisabled);
                }
            }
            GfxMode::Integrated => {
                debug!("Mode match: GfxMode::Integrated");
                do_driver_action("nouveau", "rmmod")?;
                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, "rmmod")?;
                    }
                }
                toggle_nvidia_powerd(false, devices.vendor())?;
                kill_nvidia_lsof()?;
                devices.unbind().ok();
                devices.remove().ok();
                devices.do_driver_action("rmmod")?;
                devices.set_hotplug(HotplugState::Off)?;
                // This can only be done *after* the drivers are removed or a
                // hardlock will be caused
                if asus_dgpu_exists() && asus_use_dgpu_disable {
                    asus_dgpu_set_disabled(true)?;
                }
                if asus_egpu_exists() {
                    asus_egpu_set_enabled(false)?;
                }
            }
            GfxMode::Egpu => {
                debug!("Mode match: GfxMode::Egpu");
                if !asus_egpu_exists() {
                    warn("eGPU mode attempted while unsupported by this machine");
                    return Err(GfxError::NotSupported(
                        "eGPU mode not supported".to_string(),
                    ));
                }

                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, "rmmod")?;
                    }
                }

                devices.set_hotplug(HotplugState::Off)?;
                asus_egpu_set_enabled(true)?;
                devices.do_driver_action("modprobe")?;
                toggle_nvidia_powerd(true, devices.vendor())?;
            }
            GfxMode::None | GfxMode::AsusMuxDiscreet => {}
        }
        devices.set_runtime_pm(RuntimePowerManagement::Auto)?;
        Ok(())
    }

    /// Spools until all user sessions are ended then switches to requested mode
    async fn mode_change_loop(
        mode: GfxMode,
        device: Arc<Mutex<DiscreetGpu>>,
        thread_stop: Arc<AtomicBool>,
        config: Arc<Mutex<GfxConfig>>,
    ) -> Result<String, GfxError> {
        let info = |s: &str| info!("mode_change_loop: {s}");
        info("display-manager thread started");
        let no_logind;
        let asus_use_dgpu_disable;
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
            asus_use_dgpu_disable = config.asus_use_dgpu_disable;
            logout_timeout_s = config.logout_timeout_s;
            info!("mode_change_loop: logout_timeout_s = {}", logout_timeout_s);
        }

        if no_logind {
            info("no_logind is set");
        } else {
            loop {
                if thread_stop.load(Ordering::SeqCst) {
                    info("Thread forced to exit");
                    thread_stop.store(false, Ordering::Release);
                    return Ok("Exited".to_string());
                }

                let tmp = manager.list_sessions().await?;
                if !tmp.iter().eq(&sessions) {
                    info("Sessions list changed");
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
                    warn!("mode_change_loop: {}", detail);
                    return Err(GfxError::SystemdUnitWaitTimeout(detail));
                }

                // Don't spin at max speed
                //sleep(SLEEP_PERIOD).await;
            }
        }

        let mut device = device.lock().await;
        if !no_logind {
            info("all graphical user sessions ended, continuing");
            do_systemd_unit_action(SystemdUnitAction::Stop, DISPLAY_MANAGER)?;
            wait_systemd_unit_state(SystemdUnitState::Inactive, DISPLAY_MANAGER)?;
        }

        // Need to change to integrated before we can change to vfio or compute
        // Since we have a lock, reset tmp to none. This thread should only ever run
        // for Integrated, Hybrid, or Nvidia. Tmp is also only for informational
        let mut config = config.lock().await;
        config.tmp_mode = None;

        if matches!(mode, GfxMode::Compute | GfxMode::Vfio) {
            warn!("mode_change_loop: compute or vfio mode require setting integrated mode first");
        } else {
            Self::do_mode_setup_tasks(
                mode,
                config.vfio_enable,
                asus_use_dgpu_disable,
                &mut device,
            )?;
            if !no_logind {
                do_systemd_unit_action(SystemdUnitAction::Restart, DISPLAY_MANAGER)?;
            }
        }

        // Save selected mode in case of reboot
        config.mode = mode;
        config.write();

        info("display-manager started");

        let v: &str = mode.into();
        info!(
            "mode_change_loop: Graphics mode changed to {} successfully",
            v
        );
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

        let dgpu = self.dgpu.clone();
        let thread_exit = self.thread_exit.clone();
        let config = self.config.clone();
        // This will block if required to wait for logouts, so run concurrently.
        tokio::spawn(async move {
            Self::mode_change_loop(mode, dgpu, thread_exit, config)
                .await
                .map_err(|err| {
                    error!("setup_mode_change_thread: {}", err);
                })
                .ok();
        });

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
                if mux == AsusGpuMuxMode::Discreet {
                    warn!("set_gfx_mode: ASUS GPU MUX is in discreet mode");
                    return Ok(GfxRequiredUserAction::AsusGpuMuxDisable);
                }
            }
        }

        let asus_use_dgpu_disable;
        let vfio_enable;
        {
            let config = self.config.lock().await;
            vfio_enable = config.vfio_enable;
            asus_use_dgpu_disable = config.asus_use_dgpu_disable;
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
                info!("set_gfx_mode: mode change requires a logout to complete");
                {
                    let mut config = self.config.lock().await;
                    config.pending_action = Some(action_required);
                }
                self.setup_mode_change_thread(mode).await;
            }
            GfxRequiredUserAction::Integrated => {
                info!("set_gfx_mode: mode change requires user to be in Integrated mode first");
            }
            // Generally None for vfio, compute, integrated only
            GfxRequiredUserAction::None => {
                info!("set_gfx_mode: mode change does not require logout");
                let mut dgpu = self.dgpu.lock().await;
                Self::do_mode_setup_tasks(mode, vfio_enable, asus_use_dgpu_disable, &mut dgpu)?;
                info!(
                    "set_gfx_mode: Graphics mode changed to {}",
                    <&str>::from(mode)
                );
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
        Ok(action_required)
    }
}
