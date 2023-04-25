use crate::{actions::StagedAction, error::GfxError};

impl StagedAction {
    /// Verification that the action lists are in the correct order. If incorrect then lockups and other errors can occur
    pub fn verify_previous_action_for_current(
        &self,
        previous_action: StagedAction,
    ) -> Result<(), GfxError> {
        if match self {
            StagedAction::StopDisplayManager => previous_action == StagedAction::WaitLogout,
            StagedAction::StartDisplayManager => true,
            StagedAction::NoLogind => [
                StagedAction::None,
                StagedAction::NoLogind,
                StagedAction::HotplugUnplug,
                StagedAction::AsusDgpuDisable,
                StagedAction::AsusEgpuDisable,
                StagedAction::DevTreeManaged,
                StagedAction::EnableNvidiaPowerd,
                StagedAction::NotNvidia,
            ]
            .contains(&previous_action),
            StagedAction::LoadGpuDrivers => previous_action == StagedAction::RescanPci,
            StagedAction::UnloadGpuDrivers => [
                StagedAction::StopDisplayManager,
                StagedAction::DisableNvidiaPowerd,
                StagedAction::KillNvidia,
                StagedAction::KillAmd,
                StagedAction::NotNvidia,
                StagedAction::AsusEgpuDisable,
            ]
            .contains(&previous_action),
            StagedAction::KillNvidia => [
                StagedAction::StopDisplayManager,
                StagedAction::DisableNvidiaPowerd,
                StagedAction::None,
            ]
            .contains(&previous_action),
            StagedAction::KillAmd => [
                StagedAction::NotNvidia,
                StagedAction::StopDisplayManager,
                StagedAction::None,
            ]
            .contains(&previous_action),
            StagedAction::EnableNvidiaPowerd => [
                StagedAction::DevTreeManaged,
                StagedAction::LoadGpuDrivers,
                StagedAction::None,
            ]
            .contains(&previous_action),
            StagedAction::DisableNvidiaPowerd => [
                StagedAction::StopDisplayManager,
                StagedAction::NoLogind,
                StagedAction::RescanPci,
                StagedAction::None,
            ]
            .contains(&previous_action),
            StagedAction::LoadVfioDrivers => true,
            StagedAction::UnloadVfioDrivers => true,
            StagedAction::RescanPci => [
                StagedAction::None, // Allow None due to VFIO
                StagedAction::AsusDgpuEnable,
                StagedAction::AsusDgpuDisable,
                StagedAction::AsusEgpuEnable,
                StagedAction::AsusEgpuDisable,
                StagedAction::HotplugPlug,
                StagedAction::HotplugUnplug,
                StagedAction::DevTreeManaged,
                StagedAction::WriteModprobeConf,
            ]
            .contains(&previous_action),
            StagedAction::UnbindRemoveGpu => [
                StagedAction::UnloadGpuDrivers,
                StagedAction::UnloadVfioDrivers,
            ]
            .contains(&previous_action),
            StagedAction::HotplugUnplug
            | StagedAction::HotplugPlug
            | StagedAction::AsusDgpuDisable
            | StagedAction::AsusDgpuEnable
            | StagedAction::AsusEgpuDisable
            | StagedAction::AsusEgpuEnable
            | StagedAction::DevTreeManaged => previous_action == StagedAction::WriteModprobeConf,
            StagedAction::AsusMuxIgpu => [
                StagedAction::None,
                StagedAction::DisableNvidiaPowerd,
                StagedAction::NotNvidia,
            ]
            .contains(&previous_action),
            StagedAction::AsusMuxDgpu => [
                StagedAction::EnableNvidiaPowerd,
                StagedAction::NotNvidia,
                StagedAction::None,
            ]
            .contains(&previous_action),
            StagedAction::WriteModprobeConf => [
                StagedAction::StopDisplayManager,
                StagedAction::NoLogind,
                StagedAction::UnbindRemoveGpu,
                StagedAction::UnloadGpuDrivers,
                StagedAction::UnloadVfioDrivers,
                StagedAction::None,
            ]
            .contains(&previous_action),
            StagedAction::WaitLogout | StagedAction::NotNvidia | StagedAction::None => true,
        } {
            Ok(())
        } else {
            Err(GfxError::IncorrectActionOrder(*self, previous_action))
        }
    }

    pub fn verify_next_allowed_action(
        &self,
        next_allowed_action: StagedAction,
    ) -> Result<(), GfxError> {
        if match self {
            StagedAction::WaitLogout => StagedAction::StopDisplayManager == next_allowed_action,
            StagedAction::StopDisplayManager => [
                StagedAction::DisableNvidiaPowerd,
                StagedAction::WriteModprobeConf,
                StagedAction::UnloadVfioDrivers,
                StagedAction::KillAmd,
                StagedAction::KillNvidia,
                StagedAction::NotNvidia,
            ]
            .contains(&next_allowed_action),
            StagedAction::StartDisplayManager => {
                [StagedAction::None].contains(&next_allowed_action)
            }
            StagedAction::NoLogind => [
                StagedAction::NoLogind,
                StagedAction::NotNvidia,
                StagedAction::DisableNvidiaPowerd,
                StagedAction::WriteModprobeConf,
            ]
            .contains(&next_allowed_action),
            StagedAction::LoadGpuDrivers => [
                StagedAction::EnableNvidiaPowerd,
                StagedAction::NotNvidia,
                StagedAction::None,
            ]
            .contains(&next_allowed_action),
            StagedAction::UnloadGpuDrivers => [
                StagedAction::UnbindRemoveGpu,
                StagedAction::WriteModprobeConf,
            ]
            .contains(&next_allowed_action),
            StagedAction::KillNvidia => [
                StagedAction::UnloadGpuDrivers,
                StagedAction::UnloadVfioDrivers,
            ]
            .contains(&next_allowed_action),
            StagedAction::KillAmd => [
                StagedAction::UnloadGpuDrivers,
                StagedAction::UnloadVfioDrivers,
            ]
            .contains(&next_allowed_action),
            StagedAction::EnableNvidiaPowerd => [
                StagedAction::StartDisplayManager,
                StagedAction::AsusMuxDgpu,
                StagedAction::NoLogind,
                StagedAction::None,
            ]
            .contains(&next_allowed_action),
            StagedAction::DisableNvidiaPowerd => {
                [StagedAction::KillNvidia].contains(&next_allowed_action)
            }
            StagedAction::LoadVfioDrivers => [StagedAction::None].contains(&next_allowed_action),
            StagedAction::UnloadVfioDrivers => [
                StagedAction::UnbindRemoveGpu,
                StagedAction::WriteModprobeConf,
            ]
            .contains(&next_allowed_action),
            StagedAction::DevTreeManaged => [
                StagedAction::StartDisplayManager,
                StagedAction::NoLogind,
                StagedAction::RescanPci,
            ]
            .contains(&next_allowed_action),
            StagedAction::RescanPci => [
                StagedAction::LoadGpuDrivers,
                StagedAction::DisableNvidiaPowerd,
                StagedAction::NotNvidia,
            ]
            .contains(&next_allowed_action),
            StagedAction::UnbindRemoveGpu => {
                [StagedAction::WriteModprobeConf].contains(&next_allowed_action)
            }
            StagedAction::HotplugUnplug => {
                [StagedAction::StartDisplayManager, StagedAction::NoLogind]
                    .contains(&next_allowed_action)
            }
            StagedAction::HotplugPlug => [StagedAction::RescanPci].contains(&next_allowed_action),
            StagedAction::AsusDgpuDisable => {
                [StagedAction::StartDisplayManager, StagedAction::NoLogind]
                    .contains(&next_allowed_action)
            }
            StagedAction::AsusDgpuEnable => {
                [StagedAction::RescanPci].contains(&next_allowed_action)
            }
            StagedAction::AsusEgpuDisable => [].contains(&next_allowed_action),
            StagedAction::AsusEgpuEnable => {
                [StagedAction::RescanPci].contains(&next_allowed_action)
            }
            StagedAction::AsusMuxIgpu => [].contains(&next_allowed_action),
            StagedAction::AsusMuxDgpu => [].contains(&next_allowed_action),
            StagedAction::WriteModprobeConf => [
                StagedAction::AsusEgpuDisable,
                StagedAction::AsusEgpuEnable,
                StagedAction::HotplugUnplug,
                StagedAction::AsusDgpuDisable,
                StagedAction::DevTreeManaged,
                StagedAction::HotplugPlug,
                StagedAction::AsusDgpuEnable,
                StagedAction::LoadVfioDrivers,
                StagedAction::RescanPci,
            ]
            .contains(&next_allowed_action),
            StagedAction::NotNvidia => [
                StagedAction::KillAmd,
                StagedAction::StartDisplayManager,
                StagedAction::NoLogind,
            ]
            .contains(&next_allowed_action),
            StagedAction::None => [
                StagedAction::RescanPci,
                StagedAction::NoLogind,
                StagedAction::WriteModprobeConf,
                StagedAction::WaitLogout,
                StagedAction::NotNvidia,
                StagedAction::KillNvidia,
                StagedAction::KillAmd,
                StagedAction::EnableNvidiaPowerd,
                StagedAction::DisableNvidiaPowerd,
                StagedAction::UnloadVfioDrivers,
            ]
            .contains(&next_allowed_action),
        } {
            Ok(())
        } else {
            Err(GfxError::IncorrectActionOrder(next_allowed_action, *self))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        actions::{Action, StagedAction},
        config::GfxConfig,
        pci_device::{GfxMode, GfxVendor, HotplugType},
    };

    #[test]
    fn verify_hybrid_to_integrated_action_order() {
        let mut config = GfxConfig {
            config_path: Default::default(),
            mode: crate::pci_device::GfxMode::Hybrid,
            tmp_mode: None,
            pending_mode: None,
            pending_action: None,
            vfio_enable: false,
            vfio_save: false,
            always_reboot: false,
            no_logind: false,
            logout_timeout_s: 10,
            hotplug_type: crate::pci_device::HotplugType::None,
        };

        let actions = StagedAction::action_list_for_switch(
            &config,
            GfxVendor::Nvidia,
            GfxMode::Hybrid,
            GfxMode::Integrated,
        );

        match actions {
            Action::UserAction(_) => panic!("Should be a list of actions"),
            Action::StagedActions(actions) => {
                let mut previous_action = StagedAction::None;
                for action in actions {
                    action
                        .verify_previous_action_for_current(previous_action)
                        .map_err(|e| {
                            println!("Action thread errored: {e}");
                        })
                        .unwrap();
                    previous_action = action;
                }
            }
        }

        config.no_logind = true;
        let actions = StagedAction::action_list_for_switch(
            &config,
            GfxVendor::Nvidia,
            GfxMode::Hybrid,
            GfxMode::Integrated,
        );

        match actions {
            Action::UserAction(_) => panic!("Should be a list of actions"),
            Action::StagedActions(actions) => {
                let mut previous_action = StagedAction::None;
                for action in actions {
                    action
                        .verify_previous_action_for_current(previous_action)
                        .map_err(|e| {
                            println!("Action thread errored: {e}");
                        })
                        .unwrap();
                    previous_action = action;
                }
            }
        }
    }

    #[test]
    fn verify_integrated_to_hybrid_action_order() {
        let mut config = GfxConfig {
            config_path: Default::default(),
            mode: crate::pci_device::GfxMode::Integrated,
            tmp_mode: None,
            pending_mode: None,
            pending_action: None,
            vfio_enable: false,
            vfio_save: false,
            always_reboot: false,
            no_logind: false,
            logout_timeout_s: 10,
            hotplug_type: crate::pci_device::HotplugType::None,
        };

        let actions = StagedAction::action_list_for_switch(
            &config,
            GfxVendor::Nvidia,
            GfxMode::Integrated,
            GfxMode::Hybrid,
        );

        match actions {
            Action::UserAction(_) => panic!("Should be a list of actions"),
            Action::StagedActions(actions) => {
                let mut previous_action = StagedAction::None;
                for action in actions {
                    action
                        .verify_previous_action_for_current(previous_action)
                        .map_err(|e| {
                            println!("Action thread errored: {e}");
                        })
                        .unwrap();
                    previous_action = action;
                }
            }
        }

        config.no_logind = true;
        let actions = StagedAction::action_list_for_switch(
            &config,
            GfxVendor::Nvidia,
            GfxMode::Integrated,
            GfxMode::Hybrid,
        );

        match actions {
            Action::UserAction(_) => panic!("Should be a list of actions"),
            Action::StagedActions(actions) => {
                let mut previous_action = StagedAction::None;
                for action in actions {
                    action
                        .verify_previous_action_for_current(previous_action)
                        .map_err(|e| {
                            println!("Action thread errored: {e}");
                        })
                        .unwrap();
                    previous_action = action;
                }
            }
        }
    }

    #[test]
    fn verify_all_previous() {
        let modes = [
            GfxMode::Hybrid,
            GfxMode::Integrated,
            GfxMode::NvidiaNoModeset,
            GfxMode::Vfio,
            GfxMode::AsusEgpu,
            GfxMode::AsusMuxDgpu,
            GfxMode::None,
        ];

        let mut config = GfxConfig {
            config_path: Default::default(),
            mode: crate::pci_device::GfxMode::Hybrid,
            tmp_mode: None,
            pending_mode: None,
            pending_action: None,
            vfio_enable: false,
            vfio_save: false,
            always_reboot: false,
            no_logind: false,
            logout_timeout_s: 10,
            hotplug_type: crate::pci_device::HotplugType::None,
        };

        let run = |config: &GfxConfig| {
            for from in modes {
                for to in modes {
                    for vendor in [GfxVendor::Nvidia, GfxVendor::Amd] {
                        if vendor == GfxVendor::Amd && from == GfxMode::NvidiaNoModeset
                            || from == GfxMode::AsusEgpu
                            || from == GfxMode::AsusMuxDgpu
                            || to == GfxMode::NvidiaNoModeset
                            || to == GfxMode::AsusEgpu
                            || to == GfxMode::AsusMuxDgpu
                        {
                            continue;
                        }

                        let actions =
                            StagedAction::action_list_for_switch(&config, vendor, from, to);
                        match actions {
                            Action::UserAction(_) => {} //panic!("Should be a list of actions"),
                            Action::StagedActions(actions) => {
                                let mut previous_action = StagedAction::None;
                                for action in actions {
                                    action
                                        .verify_previous_action_for_current(previous_action)
                                        .map_err(|e| {
                                            println!(
                                                "Action thread errored: from:{from}, to:{to}, {e}"
                                            );
                                        })
                                        .unwrap();
                                    previous_action = action;
                                }
                            }
                        }
                    }
                }
            }
        };

        run(&config);
        config.hotplug_type = HotplugType::Asus;
        run(&config);
        config.hotplug_type = HotplugType::Std;
        run(&config);

        config.no_logind = true;
        config.hotplug_type = HotplugType::None;
        run(&config);
        config.hotplug_type = HotplugType::Asus;
        run(&config);
        config.hotplug_type = HotplugType::Std;
        run(&config);
    }

    #[test]
    fn verify_all_next() {
        let modes = [
            GfxMode::Hybrid,
            GfxMode::Integrated,
            GfxMode::NvidiaNoModeset,
            GfxMode::Vfio,
            GfxMode::AsusEgpu,
            GfxMode::AsusMuxDgpu,
            GfxMode::None,
        ];

        let mut config = GfxConfig {
            config_path: Default::default(),
            mode: crate::pci_device::GfxMode::Hybrid,
            tmp_mode: None,
            pending_mode: None,
            pending_action: None,
            vfio_enable: false,
            vfio_save: false,
            always_reboot: false,
            no_logind: false,
            logout_timeout_s: 10,
            hotplug_type: crate::pci_device::HotplugType::None,
        };

        let run = |config: &GfxConfig| {
            for from in modes {
                for to in modes {
                    for vendor in [GfxVendor::Nvidia, GfxVendor::Amd] {
                        if vendor == GfxVendor::Amd && from == GfxMode::NvidiaNoModeset
                            || from == GfxMode::AsusEgpu
                            || from == GfxMode::AsusMuxDgpu
                            || to == GfxMode::NvidiaNoModeset
                            || to == GfxMode::AsusEgpu
                            || to == GfxMode::AsusMuxDgpu
                        {
                            continue;
                        }

                        let actions =
                            StagedAction::action_list_for_switch(&config, vendor, from, to);
                        match actions {
                            Action::UserAction(_) => {} //panic!("Should be a list of actions"),
                            Action::StagedActions(actions) => {
                                let mut previous_action = StagedAction::None;
                                for action in actions {
                                    previous_action
                                        .verify_next_allowed_action(action)
                                        .map_err(|e| {
                                            println!(
                                                "Action thread errored: from:{from}, to:{to}, {e}"
                                            );
                                        })
                                        .unwrap();
                                    previous_action = action;
                                }
                            }
                        }
                    }
                }
            }
        };

        run(&config);
        config.hotplug_type = HotplugType::Asus;
        run(&config);
        config.hotplug_type = HotplugType::Std;
        run(&config);

        config.no_logind = true;
        config.hotplug_type = HotplugType::None;
        run(&config);
        config.hotplug_type = HotplugType::Asus;
        run(&config);
        config.hotplug_type = HotplugType::Std;
        run(&config);
    }
}
