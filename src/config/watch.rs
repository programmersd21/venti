use notify::{Config as NotifyConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

/// hot reloader watcher handle for configuration updates.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// spawns a background watcher for the specified path, invoking callback on modifications.
    pub fn watch<F>(path: PathBuf, mut on_change: F) -> Result<Self, String>
    where
        F: FnMut() + Send + 'static,
    {
        let (tx, rx) = channel();
        let target = path.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| match res {
                Ok(event)
                    if (event.kind.is_modify() || event.kind.is_create())
                        && event.paths.iter().any(|p| paths_match(p, &target)) =>
                {
                    let _ = tx.send(());
                }
                Err(e) => tracing::debug!("config watcher error: {e}"),
                _ => {}
            },
            NotifyConfig::default(),
        )
        .map_err(|e| format!("failed to initialize file watcher: {e}"))?;

        let Some(parent) = path.parent() else {
            return Err(format!("config path has no parent: {}", path.display()));
        };
        if !parent.exists() {
            return Err(format!(
                "config directory does not exist: {}",
                parent.display()
            ));
        }
        watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .map_err(|e| format!("failed to watch directory: {e}"))?;

        std::thread::spawn(move || {
            let debounce = Duration::from_millis(150);
            while rx.recv().is_ok() {
                while rx.recv_timeout(debounce).is_ok() {}
                on_change();
            }
        });

        Ok(Self { _watcher: watcher })
    }
}

fn paths_match(event_path: &Path, target: &Path) -> bool {
    if event_path == target {
        return true;
    }
    // notify may report canonicalized or relative paths
    match (event_path.canonicalize(), target.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
