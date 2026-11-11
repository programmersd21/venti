use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// installs signal handlers for graceful shutdown and cleanup.
///
/// the returned flag starts as `true` ("running") and is set to `false`
/// when sigterm or sigint is received, which is what the core loop waits on.
pub fn setup_signal_handlers() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let running_in_thread = Arc::clone(&running);

    std::thread::spawn(move || {
        let mut signals = match signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGINT,
        ]) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to install signal handlers: {e}");
                return;
            }
        };
        if let Some(sig) = signals.forever().next() {
            tracing::info!("received signal {sig}, shutting down");
            running_in_thread.store(false, Ordering::Relaxed);
        }
    });

    running
}
