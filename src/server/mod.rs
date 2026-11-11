pub mod accept;
pub mod core;
pub mod persist;
pub mod session;
pub mod signals;

use std::path::Path;
use std::sync::Arc;

use crate::config::{Config, ConfigWatcher, Theme};
use crate::util::paths::config_path;
use accept::spawn_accept_loop;
use core::{CoreEvent, ServerCore};
use persist::{load_session, save_session};
use signals::setup_signal_handlers;

/// starts the server daemon listening on socket_path for session_name.
pub fn run_server(
    socket_path: &Path,
    session_name: &str,
    config: Config,
    theme: Theme,
) -> Result<(), String> {
    let running = setup_signal_handlers();
    let restore = load_session(
        &crate::util::paths::state_dir().join("sessions"),
        session_name,
    );
    let (mut core, event_tx) = ServerCore::new_for_server(
        session_name,
        config.clone(),
        theme,
        Arc::clone(&running),
        restore,
    );
    let max_clients = config.general.max_clients.clamp(1, 1000);
    let accept_handle = spawn_accept_loop(
        socket_path,
        event_tx.clone(),
        Arc::clone(&running),
        max_clients,
    )?;

    let _watcher = match ConfigWatcher::watch(config_path(), {
        let tx = event_tx.clone();
        move || {
            let _ = tx.try_send(CoreEvent::ConfigChanged);
        }
    }) {
        Ok(watcher) => Some(watcher),
        Err(e) => {
            // missing config on a fresh machine is normal; don't warn noisily about it.
            if config_path().exists() {
                tracing::warn!("config hot-reload disabled: {e}");
            } else {
                tracing::info!("config hot-reload disabled (no config file): {e}");
            }
            None
        }
    };
    core.run();

    // clean shutdown persists the layout so the next start restores it
    let snapshot = core.session.snapshot();
    if let Err(e) = save_session(&crate::util::paths::state_dir().join("sessions"), &snapshot) {
        tracing::warn!("failed to save session snapshot: {e}");
    }

    running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = accept_handle.join();

    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }

    Ok(())
}
