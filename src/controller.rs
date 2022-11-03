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

use crate::pci_device::{rescan_pci_bus, HotplugState, HotplugType};
use crate::systemd::{SystemdUnitAction, SystemdUnitState};
use crate::{
    config::create_modprobe_conf,
    error::GfxError,
    pci_device::{DiscreetGpu, GfxRequiredUserAction, GfxVendor, RuntimePowerManagement},
    special_asus::{
        asus_dgpu_exists, asus_dgpu_set_disabled, asus_egpu_exists, get_asus_gpu_mux_mode,
        has_asus_gpu_mux, AsusGpuMuxMode,
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
        let hotplug_type = config.hotplug_type;
        let always_reboot = config.always_reboot;
        let vfio_save = config.vfio_save;

        let mode = get_kernel_cmdline_mode()?
            .map(|mode| {
                warn!("reload: Graphic mode {:?} set on kernel cmdline", mode);
                if !vfio_save {
                } else {
                    config.mode = mode;
                    config.write();
                }
                mode
            })
            .unwrap_or(self.get_gfx_mode(&config)?);

        {
            // Do asus specific checks and setup first
            let dgpu = self.dgpu.lock().await;
            asus_reload(
                mode,
                hotplug_type == HotplugType::Asus,
                always_reboot,
                &dgpu,
            )
            .await?;
        }

        if matches!(mode, GfxMode::Vfio) && !vfio_enable {
            warn!("reload: Tried to set vfio mode but it is not enabled");
            return Ok(());
        }

        if matches!(mode, GfxMode::Egpu) && !asus_egpu_exists() {
            warn!("reload: Tried to set egpu mode but it is not supported");
            return Ok(());
        }

        let mut dgpu = self.dgpu.lock().await;
        Self::do_mode_setup_tasks(mode, vfio_enable, hotplug_type, &mut dgpu)?;

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
        if matches!(current, GfxMode::Integrated | GfxMode::Vfio)
            && matches!(vendor, GfxMode::Integrated | GfxMode::Vfio)
        {
            return GfxRequiredUserAction::None;
        }
        // Modes that require a switch to integrated first
        if matches!(current, GfxMode::Hybrid) && matches!(vendor, GfxMode::Vfio) {
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
                if asus_use_dgpu_disable && matches!(mode, GfxMode::Hybrid) {
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
        let hotplug_type;
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
            hotplug_type = config.hotplug_type;
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

        if matches!(mode, GfxMode::Vfio) {
            warn!("mode_change_loop: compute or vfio mode require setting integrated mode first");
        } else {
            Self::do_mode_setup_tasks(
                mode,
                config.vfio_enable,
                hotplug_type,
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
            match Self::mode_change_loop(mode, dgpu.clone(), thread_exit.clone(), config.clone())
                .await
            {
                Ok(_) => info!("setup_mode_change_thread: success"),
                Err(err) => {
                    error!("setup_mode_change_thread: {}, will retry in 1s", err);
                    sleep(Duration::from_secs(1)).await;
                    Self::mode_change_loop(mode, dgpu, thread_exit, config)
                        .await
                        .map_err(|err| {
                            error!("setup_mode_change_thread: retry failed, {}", err);
                        })
                        .ok();
                }
            }
        });

        let mut config = self.config.lock().await;
        config.pending_mode = None;
        config.pending_action = None;
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
        hotplug_type: HotplugType,
        devices: &mut DiscreetGpu,
    ) -> Result<(), GfxError> {
        debug!("do_mode_setup_tasks(mode:{mode:?}, vfio_enable:{vfio_enable}, asus_use_dgpu_disable: {hotplug_type:?})");
        let debug = |s: &str| debug!("do_mode_setup_tasks: {s}");
        let warn = |s: &str| warn!("do_mode_setup_tasks: {s}");

        create_modprobe_conf(mode, devices)?;
        Self::do_rescan(mode, hotplug_type == HotplugType::Asus, devices)?;

        match mode {
            GfxMode::Hybrid => {
                debug("Mode match: GfxMode::Hybrid | GfxMode::Compute");
                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, DriverAction::Remove)?;
                    }
                }
                if hotplug_type == HotplugType::Std {
                    devices.set_hotplug(HotplugState::On)?;
                } else if hotplug_type == HotplugType::Asus {
                    asus_set_mode(mode, hotplug_type == HotplugType::Asus, devices)?;
                }
                devices.do_driver_action(DriverAction::Load)?;
                // TODO: Need to enable or disable on AC status
                toggle_nvidia_powerd(true, devices.vendor())?;
            }
            GfxMode::Vfio => {
                debug!("Mode match: GfxMode::Vfio");
                if vfio_enable {
                    toggle_nvidia_powerd(false, devices.vendor())?;
                    kill_nvidia_lsof()?;
                    devices.unbind()?;
                    do_driver_action("vfio-pci", DriverAction::Load)?;
                } else {
                    return Err(GfxError::VfioDisabled);
                }
            }
            GfxMode::Integrated => {
                debug!("Mode match: GfxMode::Integrated");
                if vfio_enable {
                    for driver in VFIO_DRIVERS.iter() {
                        do_driver_action(driver, DriverAction::Remove)?;
                    }
                }
                toggle_nvidia_powerd(false, devices.vendor())?;
                if devices.vendor() == GfxVendor::Nvidia {
                    kill_nvidia_lsof()?;
                    devices.do_driver_action(DriverAction::Remove)?;
                }
                devices.unbind_remove()?;
                if hotplug_type == HotplugType::Std {
                    devices.set_hotplug(HotplugState::Off)?;
                } else if hotplug_type == HotplugType::Asus {
                    asus_set_mode(mode, hotplug_type == HotplugType::Asus, devices)?;
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
                        do_driver_action(driver, DriverAction::Remove)?;
                    }
                }
                if hotplug_type == HotplugType::Std {
                    devices.set_hotplug(HotplugState::Off)?;
                } else if hotplug_type == HotplugType::Asus {
                    asus_set_mode(mode, hotplug_type == HotplugType::Asus, devices)?;
                }
                devices.do_driver_action(DriverAction::Load)?;
                toggle_nvidia_powerd(true, devices.vendor())?;
            }
            GfxMode::None | GfxMode::AsusMuxDiscreet => {}
        }
        devices.set_runtime_pm(RuntimePowerManagement::Auto)?;
        Ok(())
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

        let hotplug_type;
        let vfio_enable;
        {
            let config = self.config.lock().await;
            vfio_enable = config.vfio_enable;
            hotplug_type = config.hotplug_type;
        }

        if !vfio_enable && matches!(mode, GfxMode::Vfio) {
            return Err(GfxError::VfioDisabled);
        }

            mode_support_check(&mode)?;

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
                
                Self::do_mode_setup_tasks(mode, vfio_enable, hotplug_type, &mut dgpu)?;
                info!(
                    "set_gfx_mode: Graphics mode changed to {}",
                    <&str>::from(mode)
                );
                let mut config = self.config.lock().await;
                config.tmp_mode = None;
                config.pending_action = None;
                config.pending_mode = None;

                if matches!(mode, GfxMode::Vfio) && config.vfio_save {
                    config.mode = mode;
                    config.write();
                } else if matches!(mode, GfxMode::Vfio) {
                    config.tmp_mode = Some(mode);
                }
            }
            GfxRequiredUserAction::AsusGpuMuxDisable => {}
        }
        Ok(action_required)
    }
}
