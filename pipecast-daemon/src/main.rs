mod stop;
mod servers;
mod handler;
mod platform;
mod settings;
mod profile;

use crate::handler::primary_worker::start_primary_worker;
use crate::platform::spawn_runtime;
use crate::servers::http_server::spawn_http_server;
use crate::servers::ipc_server::{bind_socket, spawn_ipc_server};
use crate::stop::Stop;
use anyhow::{anyhow, bail, Context, Result};
use directories::ProjectDirs;
use log::{debug, error, info, LevelFilter};
use pipecast_ipc::commands::HttpSettings;
use simplelog::{ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode};
use tokio::sync::{broadcast, mpsc};
use tokio::{join, task};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<()> {
    let dirs = ProjectDirs::from("io", "github", "pipecast").ok_or(anyhow!("Unable to locate project directory"))?;


    // We need to ignore a couple of packages log output so create a builder.
    let mut log_config = ConfigBuilder::new();

    // The tracing package, when used, will output to INFO from zbus every second..
    log_config.add_filter_ignore_str("tracing");

    // Actix is a little noisy on startup and shutdown, so quiet it a bit :)
    log_config.add_filter_ignore_str("actix_server::accept");
    log_config.add_filter_ignore_str("actix_server::worker");
    log_config.add_filter_ignore_str("actix_server::server");
    log_config.add_filter_ignore_str("actix_server::builder");


    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        log_config.build(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )]).context("Could not configure the logger")?;

    info!("Starting PipeCast v{}", VERSION);

    let shutdown = Stop::new();

    // Create the Global Manager Channels...
    let (manager_send, manager_recv) = mpsc::channel(32);

    // Prepare the IPC Socket..
    let ipc_socket = bind_socket().await;
    if ipc_socket.is_err() {
        error!("Error Starting Daemon: ");
        bail!("{}", ipc_socket.err().unwrap());
    }
    let ipc_socket = ipc_socket?;
    let communications_handle = tokio::spawn(spawn_ipc_server(
        ipc_socket,
        manager_send.clone(),
        shutdown.clone(),
    ));

    // Prepare the HTTP Server..
    let http_settings = HttpSettings {
        enabled: true,
        bind_address: "localhost".to_string(),
        cors_enabled: false,
        port: 14565,
    };

    let (httpd_tx, httpd_rx) = tokio::sync::oneshot::channel();
    let (broadcast_tx, broadcast_rx) = broadcast::channel(16);
    drop(broadcast_rx);

    tokio::spawn(spawn_http_server(
        manager_send.clone(),
        httpd_tx,
        broadcast_tx.clone(),
        http_settings,
    ));
    let http_server = httpd_rx.await?;

    // We're going to go to sleep, then trigger the shutdown..
    // sleep(Duration::from_secs(5)).await;
    // shutdown.trigger();

    let task = task::spawn(start_primary_worker(
        manager_recv,
        shutdown.clone(),
        broadcast_tx.clone(),
    ));

    let runtime = task::spawn(spawn_runtime(shutdown.clone()));

    let _ = join!(task, communications_handle, runtime);
    http_server.stop(false).await;

    debug!("Should be done!");
    Ok(())
}
