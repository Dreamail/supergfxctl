use std::{env, sync::Arc, time::Duration};

use log::{error, info};
use std::io::Write;
use supergfxctl::{
    config::GfxConfig, controller::CtrlGraphics, error::GfxError, CONFIG_PATH, DBUS_DEST_NAME,
    DBUS_IFACE_PATH, VERSION,
};
use tokio::time::sleep;
use zbus::{export::futures_util::lock::Mutex, Connection};
use zvariant::ObjectPath;

#[tokio::main]
async fn main() -> Result<(), GfxError> {
    let mut logger = env_logger::Builder::new();
    logger
        .parse_default_env()
        .target(env_logger::Target::Stdout)
        .format(|buf, record| writeln!(buf, "{}: {}", record.level(), record.args()))
        // .filter(None, LevelFilter::Debug)
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
    let config = Arc::new(Mutex::new(config));

    // Graphics switching requires some checks on boot specifically for g-sync capable laptops
    match CtrlGraphics::new(config.clone()) {
        Ok(mut ctrl) => {
            ctrl.reload()
                .await
                .unwrap_or_else(|err| error!("Gfx controller: {}", err));

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
        // if let Err(err) = object_server.try_handle_next() {
        //     error!("{}", err);
        // }
        sleep(Duration::from_secs(1)).await;
    }
}
