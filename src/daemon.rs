use std::{env, sync::Arc, time::Duration};

use log::{error, info, trace};
use logind_zbus::manager::ManagerProxy;
use std::io::Write;
use supergfxctl::{
    config::GfxConfig,
    controller::CtrlGraphics,
    error::GfxError,
    pci_device::{DiscreetGpu, GfxMode, GfxPower, HotplugType},
    special_asus::{asus_dgpu_disable_exists, asus_dgpu_set_disabled},
    CONFIG_PATH, DBUS_DEST_NAME, DBUS_IFACE_PATH, VERSION,
};
use tokio::time::sleep;
use zbus::{
    export::futures_util::{lock::Mutex, StreamExt},
    Connection,
};
use zbus::{object_server::SignalEmitter, zvariant::ObjectPath};

#[tokio::main]
async fn main() -> Result<(), GfxError> {
    let mut logger = env_logger::Builder::new();
    logger
        .parse_default_env()
        .target(env_logger::Target::Stdout)
        .format(|buf, record| writeln!(buf, "{}: {}", record.level(), record.args()))
        .init();

    let is_service = match env::var_os("IS_SERVICE") {
        Some(val) => val == "1",
        None => false,
    };

    if !is_service {
        println!("supergfxd schould be only run from the right systemd service");
        println!(
            "do not run in your terminal, if you need an logs please use journalctl -b -u supergfxd"
        );
        println!("supergfxd will now exit");
        return Ok(());
    }

    info!("Daemon version: {VERSION}");

    start_daemon().await
}

async fn start_daemon() -> Result<(), GfxError> {
    // Start zbus server
    let connection = Connection::system().await?;
    // Request dbus name after finishing initalizing all functions
    connection.request_name(DBUS_DEST_NAME).await?;

    let config = GfxConfig::load(CONFIG_PATH.into());
    let use_logind = !config.no_logind;
    let config = Arc::new(Mutex::new(config));

    if use_logind {
        start_logind_tasks(config.clone()).await;
    }

    // Graphics switching requires some checks on boot specifically for g-sync capable laptops
    match CtrlGraphics::new(config.clone()) {
        Ok(mut ctrl) => {
            ctrl.reload()
                .await
                .unwrap_or_else(|err| error!("Gfx controller: {}", err));

            let signal_context = SignalEmitter::new(&connection, DBUS_IFACE_PATH)?;
            start_notify_status(ctrl.dgpu_arc_clone(), signal_context)
                .await
                .ok();

            connection
                .object_server()
                .at(&ObjectPath::from_str_unchecked(DBUS_IFACE_PATH), ctrl)
                .await
                // .map_err(|err| {
                //     warn!("{}: add_to_server {}", path, err);
                //     err
                // })
                .ok();
        }
        Err(err) => {
            error!("Gfx control: {}", err);
        }
    }
    // Request dbus name after finishing initalizing all functions
    connection.request_name(DBUS_DEST_NAME).await?;

    // Loop to check errors and iterate zbus server
    loop {
        sleep(Duration::from_secs(1)).await;
    }
}

async fn start_notify_status(
    dgpu: Arc<Mutex<DiscreetGpu>>,
    signal_ctxt: SignalEmitter<'static>,
) -> Result<(), GfxError> {
    tokio::spawn(async move {
        let mut last_status = GfxPower::Unknown;
        loop {
            let s = dgpu
                .lock()
                .await
                .get_runtime_status()
                .map_err(|e| trace!("{e}"))
                .unwrap_or(GfxPower::Unknown);
            if s != last_status {
                last_status = s;
                trace!("Notify: dGPU status = {s:?}");
                CtrlGraphics::notify_gfx_status(&signal_ctxt, &last_status)
                    .await
                    .map_err(|e| trace!("{e}"))
                    .ok();
            }
            sleep(Duration::from_secs(1)).await;
        }
    });
    Ok(())
}

async fn start_logind_tasks(config: Arc<Mutex<GfxConfig>>) {
    let connection = Connection::system()
        .await
        .expect("Controller could not create dbus connection");

    let manager = ManagerProxy::new(&connection)
        .await
        .expect("Controller could not create ManagerProxy");

    tokio::spawn(async move {
        if let Ok(mut notif) = manager.receive_prepare_for_sleep().await {
            while let Some(event) = notif.next().await {
                if let Ok(args) = event.args() {
                    if !args.start() {
                        // on_wake();
                        let config = config.lock().await;
                        if config.mode == GfxMode::Integrated
                            && config.hotplug_type == HotplugType::Asus
                            && asus_dgpu_disable_exists()
                        {
                            info!("logind task: Waking from suspend, setting dgpu_disable");
                            asus_dgpu_set_disabled(true)
                                .map_err(|e| error!("logind task: {e}"))
                                .ok();
                        }
                    }
                }
            }
        }
    });
}
