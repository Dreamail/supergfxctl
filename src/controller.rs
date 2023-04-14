use log::{debug, info, warn};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
};
use zbus::export::futures_util::lock::Mutex;

use crate::{
    actions::{StagedAction, UserActionRequired},
    pci_device::HotplugType,
};
use crate::{
    error::GfxError,
    pci_device::{DiscreetGpu, GfxVendor, RuntimePowerManagement},
    special_asus::{asus_dgpu_disable_exists, asus_egpu_enable_exists},
    *,
};

use super::config::GfxConfig;

pub struct CtrlGraphics {
    pub(crate) dgpu: Arc<Mutex<DiscreetGpu>>,
    pub(crate) config: Arc<Mutex<GfxConfig>>,
    loop_exit: Arc<AtomicBool>,
}

impl CtrlGraphics {
    pub fn new(config: Arc<Mutex<GfxConfig>>) -> Result<CtrlGraphics, GfxError> {
        Ok(CtrlGraphics {
            dgpu: Arc::new(Mutex::new(DiscreetGpu::new()?)),
            config,
            loop_exit: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn dgpu_arc_clone(&self) -> Arc<Mutex<DiscreetGpu>> {
        self.dgpu.clone()
    }

    /// Force re-init of all state, including reset of device state
    pub async fn reload(&mut self) -> Result<(), GfxError> {
        let mut config = self.config.lock().await;
        let vfio_enable = config.vfio_enable;

        let mode = get_kernel_cmdline_mode()?
            .map(|mode| {
                warn!("reload: Graphic mode {:?} set on kernel cmdline", mode);
                config.mode = mode;
                config.write();
                mode
            })
            .unwrap_or(self.get_gfx_mode(&config)?);

        if matches!(mode, GfxMode::Vfio) && !vfio_enable {
            warn!("reload: Tried to set vfio mode but it is not enabled");
            return Ok(());
        }

        if matches!(mode, GfxMode::AsusEgpu) && !asus_egpu_enable_exists() {
            warn!("reload: Tried to set egpu mode but it is not supported");
            return Ok(());
        }

        let mut dgpu = self.dgpu.lock().await;
        Self::do_boot_tasks(mode, &mut config, &mut dgpu).await?;

        info!("reload: Reloaded gfx mode: {:?}", mode);
        Ok(())
    }

    /// Associated method to get which mode is set
    pub(crate) fn get_gfx_mode(&self, config: &GfxConfig) -> Result<GfxMode, GfxError> {
        if let Some(mode) = config.tmp_mode {
            dbg!(&mode);
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
    pub(crate) async fn get_pending_user_action(&self) -> UserActionRequired {
        let config = self.config.lock().await;
        if let Some(action) = config.pending_action {
            return action;
        }
        UserActionRequired::Nothing
    }

    /// Associated method to get list of supported modes
    pub(crate) async fn get_supported_modes(&self) -> Vec<GfxMode> {
        let mut list = vec![GfxMode::Integrated, GfxMode::Hybrid];

        let dgpu = self.dgpu.lock().await;
        if matches!(dgpu.vendor(), GfxVendor::Unknown) && !asus_dgpu_disable_exists() {
            return vec![GfxMode::Integrated];
        }

        let config = self.config.lock().await;
        if config.vfio_enable {
            list.push(GfxMode::Vfio);
        }

        if asus_egpu_enable_exists() {
            list.push(GfxMode::AsusEgpu);
        }

        if asus_gpu_mux_exists() {
            list.push(GfxMode::AsusMuxDgpu);
        }

        if let Ok(Some(res)) = get_kernel_cmdline_nvidia_modeset() {
            if !res {
                list.push(GfxMode::NvidiaNoModeset);
            }
        }

        list
    }

    /// Associated method to get which vendor the dgpu is from
    pub(crate) async fn get_gfx_vendor(&self) -> GfxVendor {
        let dgpu = self.dgpu.lock().await;
        dgpu.vendor()
    }

    /// Perform boot tasks required to set last saved mode
    async fn do_boot_tasks(
        mut mode: GfxMode,
        config: &mut GfxConfig,
        device: &mut DiscreetGpu,
    ) -> Result<(), GfxError> {
        debug!(
            "do_mode_setup_tasks(mode:{mode:?}, vfio_enable:{}, asus_use_dgpu_disable: {:?})",
            config.vfio_enable, config.hotplug_type
        );
        // Absolutely must check the ASUS dgpu_disable and gpu mux sanity on boot
        if let Ok(checked_mode) =
            asus_boot_safety_check(mode, config.hotplug_type == HotplugType::Asus)
                .await
                .map_err(|e| {
                    error!("asus_boot_safety_check errored: {e}");
                })
        {
            config.mode = checked_mode;
            mode = checked_mode;
        }

        let loop_exit = Arc::new(AtomicBool::new(false));

        let actions = StagedAction::action_list_for_boot(config, device.vendor(), mode);

        let mut failed = false;
        for action in actions {
            action
                .perform(mode, device, loop_exit.clone())
                .await
                .map_err(|e| {
                    failed = true;
                    error!("Action thread errored: {e}");
                })
                .ok();
        }

        device.set_runtime_pm(RuntimePowerManagement::Auto)?;
        Ok(())
    }

    /// Initiates a mode change by starting a thread that will wait until all
    /// graphical sessions are exited before performing the tasks required
    /// to switch modes.
    ///
    /// For manually calling (not on boot/startup) via dbus
    pub async fn set_gfx_mode(&mut self, mode: GfxMode) -> Result<UserActionRequired, GfxError> {
        mode_support_check(&mode)?;

        self.loop_exit.store(false, Ordering::Release);

        let user_action_required;
        let actions;
        {
            let mut config = self.config.lock().await;
            let vendor = self.dgpu.lock().await.vendor();
            let from = config.mode;

            if config.always_reboot {
                user_action_required = UserActionRequired::Reboot;
            } else {
                user_action_required = UserActionRequired::mode_change_action(mode, config.mode);
            }
            actions = StagedAction::action_list_for_switch(&config, vendor, from, mode);

            config.pending_mode = Some(mode);
            config.pending_action = Some(user_action_required);
        }

        // Start a thread to perform the actions on then return the user action required
        // First, stop all threads
        self.loop_exit.store(true, Ordering::Release);

        match actions {
            actions::Action::UserAction(u) => return Ok(u),
            actions::Action::StagedActions(actions) => {
                let dgpu = self.dgpu.clone();
                // This atomixc is to force an exit of any loops
                let loop_exit = self.loop_exit.clone();
                let config = self.config.clone();
                // This will block if required to wait for logouts, so run concurrently.
                tokio::spawn(async move {
                    let mut failed = false;
                    for action in actions {
                        debug!("Doing action: {action:?}");
                        let mut dgpu = dgpu.lock().await;
                        action
                            .perform(mode, &mut dgpu, loop_exit.clone())
                            .await
                            .map_err(|e| {
                                failed = true;
                                error!("Action thread perform errored: {e}");
                            })
                            .ok();
                    }

                    let mut config = config.lock().await;
                    config.pending_mode = None;
                    config.pending_action = None;
                    if !failed {
                        config.mode = mode;
                        config.write();
                    }
                });
            }
        }

        Ok(user_action_required)
    }
}
