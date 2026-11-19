//! Outer terminal graphics capability detection.
//!
//! Determines which graphics protocol sequences the client forwards to the host
//! (`apply_update.rs`) and which `TERM` the server advertises to panes.
//!
//! Detection is name-based, mirroring how Zellij and chafa probe: terminals are
//! trusted only for protocols they are documented to support. Inside tmux we
//! can't see the real outer terminal at all, so everything is wrapped in the DCS
//! passthrough envelope and the outer terminal decides.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Outer terminal's graphics capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OuterGraphics {
    /// Kitty Graphics Protocol: APC `ESC _G … ESC \\` with Unicode Placeholders.
    /// Supported by: kitty, ghostty, wezterm (via passthrough).
    Kitty,
    /// Sixel graphics: `ESC P … q … ESC \\` (foot, konsole, mlterm, contour, mintty).
    Sixel,
    /// We are inside tmux; outer terminal decides via passthrough.
    Tmux,
    /// No graphics capability detected.
    #[default]
    Unknown,
}

/// Maps a graphics capability code (from the query responder) to an `OuterGraphics` value.
/// Bit 1 = Kitty, bit 0 = Sixel.
pub fn from_code(code: u8) -> OuterGraphics {
    if code & (1 << 1) != 0 {
        OuterGraphics::Kitty
    } else if code & (1 << 0) != 0 {
        OuterGraphics::Sixel
    } else {
        OuterGraphics::Unknown
    }
}

/// Maps a terminal name (TERM or TERM_PROGRAM) to its graphics capability, or `None` for unknown terminals.
pub fn from_term_name(term: &str) -> Option<OuterGraphics> {
    let t = term.to_lowercase();
    // kitty-protocol terminals (checked first; wezterm supports both but KGP is preferred)
    if t.contains("kitty") || t.contains("ghostty") || t.contains("wezterm") {
        return Some(OuterGraphics::Kitty);
    }
    // sixel-only terminals
    if t.contains("foot")
        || t.contains("konsole")
        || t.contains("mlterm")
        || t.contains("mintty")
        || t.contains("contour")
        || t.starts_with("xterm-sixel")
    {
        return Some(OuterGraphics::Sixel);
    }
    None
}

/// Resolves graphics support from the terminal name reported by a client.
/// The daemon may have been started in a different terminal, so its own environment
/// is not authoritative for newly attached clients.
pub fn from_client_term(term: &str) -> OuterGraphics {
    from_client_identity(term, None)
}

/// Resolves the client terminal from its `TERM` and `TERM_PROGRAM` values.
pub fn from_client_identity(term: &str, term_program: Option<&str>) -> OuterGraphics {
    if let Some(graphics) = from_term_name(term).or_else(|| term_program.and_then(from_term_name)) {
        return graphics;
    }
    let name = term.to_lowercase();
    if name.starts_with("tmux") || name.starts_with("screen") {
        return match detect() {
            OuterGraphics::Tmux => tmux_outer_graphics().unwrap_or(OuterGraphics::Unknown),
            graphics => graphics,
        };
    }
    OuterGraphics::Unknown
}

/// Returns the most useful terminal identity for child processes and queries.
pub fn client_term_identity(term: &str, term_program: Option<&str>) -> String {
    if from_term_name(term).is_some()
        || term_program.is_none_or(|program| from_term_name(program).is_none())
    {
        term.to_string()
    } else {
        term_program.unwrap_or_default().to_string()
    }
}

/// Returns the `TERM` value to advertise to child panes, based on the outer terminal.
pub fn child_term_for(outer_term: &str, graphics: OuterGraphics) -> String {
    match graphics {
        OuterGraphics::Kitty => "xterm-kitty".to_string(),
        OuterGraphics::Sixel
            if from_term_name(outer_term) == Some(OuterGraphics::Sixel)
                || outer_term.to_lowercase().starts_with("tmux")
                || outer_term.to_lowercase().starts_with("screen") =>
        {
            outer_term.to_string()
        }
        _ => "xterm-256color".to_string(),
    }
}

/// Returns the `TERM_PROGRAM` value to advertise to child panes.
pub fn child_term_program_for(graphics: OuterGraphics) -> &'static str {
    match graphics {
        OuterGraphics::Kitty => "kitty",
        _ => "venti",
    }
}

/// Returns the terminal identity to report for XTVERSION queries as `(name, version)`.
/// We proxy a version string that satisfies apps gating features on version numbers
/// (e.g. yazi requires kitty >= 0.28 for graphics).
pub fn xtversion_identity_for(term: &str, graphics: OuterGraphics) -> (&'static str, &'static str) {
    let term = term.to_lowercase();
    match graphics {
        OuterGraphics::Kitty if term.contains("kitty") => ("kitty", "0.35.0"),
        OuterGraphics::Kitty if term.contains("ghostty") => ("ghostty", "1.0.0"),
        OuterGraphics::Kitty if term.contains("wezterm") => ("WezTerm", "20240128"),
        OuterGraphics::Kitty => ("kitty", "0.35.0"),
        OuterGraphics::Sixel if term.contains("foot") => ("foot", "1.18.0"),
        OuterGraphics::Sixel if term.contains("konsole") => ("Konsole", "24.0"),
        OuterGraphics::Sixel if term.contains("mlterm") => ("mlterm", "3.9"),
        OuterGraphics::Sixel if term.contains("mintty") => ("mintty", "3.7"),
        OuterGraphics::Sixel if term.contains("contour") => ("contour", "0.4"),
        _ => ("venti", env!("CARGO_PKG_VERSION")),
    }
}

fn tmux_client_graphics() -> Option<OuterGraphics> {
    static CACHED: OnceLock<Option<OuterGraphics>> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::process::Command::new("tmux")
            .args(["display-message", "-p", "#{client_termname}"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_lowercase())
            .and_then(|t| from_term_name(&t))
    })
}

fn detect_uncached() -> OuterGraphics {
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    if std::env::var_os("TMUX").is_some()
        || std::env::var_os("STY").is_some()
        || term.starts_with("tmux")
        || term.starts_with("screen")
    {
        return OuterGraphics::Tmux;
    }
    let prog = std::env::var("TERM_PROGRAM").unwrap_or_default();
    if let Some(g) = from_term_name(&term).or_else(|| from_term_name(&prog)) {
        return g;
    }
    if std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var_os("GHOSTTY_RESOURCES_DIR").is_some()
        || std::env::var_os("WEZTERM_PANE").is_some()
    {
        return OuterGraphics::Kitty;
    }
    OuterGraphics::Unknown
}

/// Graphics capability of the outer terminal (cached per process).
pub fn detect() -> OuterGraphics {
    static CACHED: OnceLock<OuterGraphics> = OnceLock::new();
    *CACHED.get_or_init(detect_uncached)
}

/// Graphics capability to use when inside tmux, based on the outer terminal. `None` when it cannot be determined.
pub fn tmux_outer_graphics() -> Option<OuterGraphics> {
    if std::env::var_os("TMUX").is_some() {
        tmux_client_graphics()
    } else {
        None
    }
}

/// Whether the outer terminal (or the one behind tmux) can render Kitty Graphics Protocol.
pub fn kitty_supported() -> bool {
    match detect() {
        OuterGraphics::Kitty => true,
        OuterGraphics::Tmux => tmux_outer_graphics() == Some(OuterGraphics::Kitty),
        _ => false,
    }
}

/// Whether the outer terminal (or the one behind tmux) can render Sixel graphics.
pub fn sixel_supported() -> bool {
    match detect() {
        OuterGraphics::Sixel => true,
        OuterGraphics::Tmux => tmux_outer_graphics() == Some(OuterGraphics::Sixel),
        _ => false,
    }
}

static CELL_PIXELS: AtomicU64 = AtomicU64::new(0);

/// Cell pixel size used for graphics geometry. Falls back to 10×20 when no terminal has reported one.
pub fn cell_pixel_size() -> (u32, u32) {
    let packed = CELL_PIXELS.load(Ordering::Relaxed);
    if packed == 0 {
        (10, 20)
    } else {
        (
            ((packed >> 32) & u64::from(u32::MAX)) as u32,
            (packed & u64::from(u32::MAX)) as u32,
        )
    }
}

/// Records the attached terminal's cell pixel size when the client reports one.
pub fn set_cell_pixel_size(size: Option<(u16, u16)>) {
    let Some((width, height)) = size else {
        return;
    };
    if width == 0 || height == 0 {
        return;
    }
    CELL_PIXELS.store(
        (u64::from(width) << 32) | u64::from(height),
        Ordering::Relaxed,
    );
}

/// Graphics context resolved once per child spawn: which TERM to advertise and
/// whether the attached terminal speaks Kitty or Sixel graphics.
/// Groups spawn parameters so the PTY spawn path does not take a growing list of extra arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsSpawn {
    /// TERM value the client reported to venti; advertised to the child.
    pub term: String,
    /// Capability of the attached terminal (or the one behind tmux).
    pub graphics: OuterGraphics,
}

impl GraphicsSpawn {
    /// Builds from the process TERM, defaulting to xterm-256color.
    pub fn from_env() -> Self {
        let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
        Self::from_term(&term)
    }

    /// Builds from an explicit TERM value, e.g. for a run-time pane spawn.
    pub fn from_term(term: &str) -> Self {
        Self {
            graphics: from_client_term(term),
            term: term.to_string(),
        }
    }

    /// The `TERM` value the child pane must advertise.
    pub fn child_term(&self) -> String {
        child_term_for(&self.term, self.graphics)
    }

    /// The `TERM_PROGRAM` value the child pane must advertise.
    pub fn child_term_program(&self) -> &'static str {
        child_term_program_for(self.graphics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_term_name_maps_known_terminals() {
        assert_eq!(from_term_name("kitty"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("xterm-kitty"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("xterm-256color"), None);
        assert_eq!(from_term_name("xterm-sixel"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("ghostty"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("wezterm"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("foot"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("konsole"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("mlterm"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("mintty"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("contour"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("tmux"), None);
    }

    #[test]
    fn client_term_selects_matching_child_graphics_protocol() {
        assert_eq!(from_client_term("foot"), OuterGraphics::Sixel);
        assert_eq!(child_term_for("foot", OuterGraphics::Sixel), "foot");
        assert_eq!(child_term_program_for(OuterGraphics::Sixel), "venti");

        assert_eq!(from_client_term("xterm-kitty"), OuterGraphics::Kitty);
        assert_eq!(
            child_term_for("xterm-kitty", OuterGraphics::Kitty),
            "xterm-kitty"
        );
        assert_eq!(child_term_program_for(OuterGraphics::Kitty), "kitty");
    }

    #[test]
    fn term_program_identifies_graphics_when_term_is_generic() {
        assert_eq!(
            from_client_identity("xterm-256color", Some("kitty")),
            OuterGraphics::Kitty
        );
        assert_eq!(
            client_term_identity("xterm-256color", Some("kitty")),
            "kitty"
        );
        assert_eq!(
            from_client_identity("xterm-256color", Some("foot")),
            OuterGraphics::Sixel
        );
    }
}
