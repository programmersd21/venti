use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// current snapshot schema version.
pub const SNAPSHOT_VERSION: u32 = 2;
const MAX_SNAPSHOT_SIZE: u64 = 8 * 1024 * 1024;

/// serializable snapshot of a session layout for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    /// snapshot schema version; only [`SNAPSHOT_VERSION`] is restored.
    #[serde(default)]
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub active_workspace: usize,
    #[serde(default)]
    pub focused: Option<u32>,
    #[serde(default)]
    pub zoomed: Option<u32>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceSnapshot>,
}

/// serializable state of one workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub workspace: usize,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tree: Option<crate::layout::LayoutNode>,
    #[serde(default)]
    pub floating: Vec<FloatingSnapshot>,
}

/// serializable floating pane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingSnapshot {
    pub id: u32,
    pub rect: RectSnapshot,
    #[serde(default)]
    pub z_index: u32,
}

/// serializable rectangle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RectSnapshot {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// returns true if the name is a single safe file-name component.
fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && !name.contains(char::is_control)
}

fn snapshot_path(dir: &Path, name: &str) -> Result<PathBuf, String> {
    if !is_safe_name(name) {
        return Err(format!("unsafe session name: {name:?}"));
    }
    Ok(dir.join(format!("{name}.toml")))
}

/// saves session state to disk at `$xdg_state_home/venti/sessions/<name>.toml`.
pub fn save_session(dir: &Path, snapshot: &SessionSnapshot) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("failed to create session dir: {e}"))?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("failed to secure session dir: {e}"))?;

    let path = snapshot_path(dir, &snapshot.name)?;
    let toml_str =
        toml::to_string(snapshot).map_err(|e| format!("failed to serialize session: {e}"))?;

    // write to a temp file in the same directory, then rename atomically.
    let tmp_path = dir.join(format!(
        ".{}.toml.tmp-{}",
        snapshot.name,
        std::process::id()
    ));
    fs::write(&tmp_path, toml_str).map_err(|e| format!("failed to write session file: {e}"))?;
    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("failed to secure session file: {e}"))?;
    fs::rename(&tmp_path, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("failed to rename session file: {e}")
    })?;
    Ok(())
}

/// loads session snapshot from disk if present and current.
pub fn load_session(dir: &Path, name: &str) -> Option<SessionSnapshot> {
    let path = snapshot_path(dir, name).ok()?;
    if !path.exists() {
        return None;
    }
    let file = fs::File::open(&path).ok()?;
    let mut content = String::new();
    file.take(MAX_SNAPSHOT_SIZE + 1)
        .read_to_string(&mut content)
        .ok()?;
    if content.len() as u64 > MAX_SNAPSHOT_SIZE {
        return None;
    }
    match toml::from_str(&content) {
        Ok(s) => Some(s),
        Err(_) => {
            // rename corrupt file with .bad suffix, keeping any existing .bad backup.
            let bad_path = dir.join(format!("{name}.toml.bad-{}", std::process::id()));
            if fs::rename(&path, &bad_path).is_err() {
                let _ = fs::remove_file(&path);
            }
            None
        }
    }
}

impl RectSnapshot {
    pub fn from_rect(rect: ratatui::layout::Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }

    pub fn to_rect(self) -> ratatui::layout::Rect {
        ratatui::layout::Rect::new(self.x, self.y, self.width, self.height)
    }
}
