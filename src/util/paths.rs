use directories::ProjectDirs;
use std::path::PathBuf;

/// returns project directories qualifying venti under standard xdg paths.
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "venti")
}

/// returns the configuration file path: $xdg_config_home/venti/config.toml.
pub fn config_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("venti").join("config.toml");
    }
    if let Some(dirs) = project_dirs() {
        dirs.config_dir().join("config.toml")
    } else {
        PathBuf::from(".config/venti/config.toml")
    }
}

/// returns the state directory path: $xdg_state_home/venti.
pub fn state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_STATE_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("venti");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("venti");
    }
    PathBuf::from(".local/state/venti")
}

/// fnv-1a 32-bit hash, used to disambiguate session names that sanitize identically.
fn fnv1a(s: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for byte in s.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

/// sanitizes a session name for safe use as a file name.
///
/// non-lossless conversions get a short hash suffix so two distinct session
/// names can never silently share the same socket or file name.
pub fn sanitize_session_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    let base = if trimmed.is_empty() {
        "default"
    } else {
        trimmed
    };
    if base == name {
        // lossless: safe to use as-is (still bounded for file-name limits).
        if base.len() <= 64 {
            return base.to_string();
        }
        let keep = 56;
        return base.chars().take(keep).collect();
    }
    let truncated: String = base.chars().take(52).collect();
    format!("{}-{:08x}", truncated, fnv1a(name))
}

/// returns the directory containing session sockets.
pub fn socket_dir() -> PathBuf {
    let uid = rustix::process::getuid().as_raw();
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR")
        && !runtime_dir.is_empty()
    {
        return PathBuf::from(runtime_dir)
            .join("venti")
            .join(uid.to_string());
    }
    PathBuf::from(format!("/tmp/venti-{uid}"))
}

/// returns the socket path for the given session name.
pub fn socket_path(session: &str) -> PathBuf {
    socket_dir().join(format!("{}.sock", sanitize_session_name(session)))
}
