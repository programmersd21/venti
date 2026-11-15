//! which sequences the client forwards to the host (apply_update.rs) and
//! which TERM the server advertises to panes.
//!
//! detection is name-based, mirroring how Zellij and chafa probe: terms are
//! trusted only for protocols they are documented to support, and inside
//! tmux we can't see the real outer terminal at all, so everything is
//! wrapped in the DCS passthrough envelope and the outer terminal decides.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// outer terminal's graphics capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OuterGraphics {
    /// kitty graphics protocol: `ESC _ G … ESC \\` (kitty, ghostty, wezterm).
    Kitty,
    /// sixel: `ESC P … q … ESC \\` (konsole, foot, mlterm, mintty, contour).
    Sixel,
    /// iTerm2 inline images: `ESC ] 1337 ; File=…` (iTerm2, wezterm).
    Iterm2,
    /// we are inside tmux; capabilities of the outer terminal are not directly
    /// visible, so everything is forwarded wrapped in the DCS passthrough
    /// envelope and the outer terminal decides what it can render.
    Tmux,
    /// nothing known; do not advertise graphics to panes.
    #[default]
    Unknown,
}

/// maps a terminal name (TERM or TERM_PROGRAM) to its graphics capability.
pub fn from_term_name(term: &str) -> Option<OuterGraphics> {
    let t = term.to_lowercase();
    if t.contains("kitty") || t.contains("ghostty") || t.contains("wezterm") {
        return Some(OuterGraphics::Kitty);
    }
    if t.contains("konsole")
        || t.contains("foot")
        || t.contains("mlterm")
        || t.contains("mintty")
        || t.contains("contour")
    {
        return Some(OuterGraphics::Sixel);
    }
    if t.contains("iterm") {
        return Some(OuterGraphics::Iterm2);
    }
    None
}

/// resolves graphics support from the terminal name reported by a client.
/// the daemon may have been started by a different terminal, so its own
/// environment is not authoritative for newly attached clients.
pub fn from_client_term(term: &str) -> OuterGraphics {
    if let Some(graphics) = from_term_name(term) {
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

/// child `TERM` value based on the attached terminal, not the daemon's parent.
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
        OuterGraphics::Iterm2
        | OuterGraphics::Sixel
        | OuterGraphics::Tmux
        | OuterGraphics::Unknown => "xterm-256color".to_string(),
    }
}

/// `TERM_PROGRAM` value based on the attached terminal's graphics protocol.
pub fn child_term_program_for(graphics: OuterGraphics) -> &'static str {
    match graphics {
        OuterGraphics::Kitty => "kitty",
        OuterGraphics::Iterm2 => "iTerm.app",
        OuterGraphics::Sixel | OuterGraphics::Tmux | OuterGraphics::Unknown => "venti",
    }
}

/// terminal identity to report to applications running inside an attached pane.
/// returns `(name, version)`. when we proxy a known terminal the
/// version is a capability-compatible string so apps that gate features on
/// version numbers (e.g. yazi requires kitty >= 0.28) get the right answer.
pub fn xtversion_identity_for(term: &str, graphics: OuterGraphics) -> (&'static str, &'static str) {
    let term = term.to_lowercase();
    match graphics {
        // kitty: yazi requires >= 0.28.0 for unicode placeholders; report a
        // version that signals full graphics-protocol support.
        OuterGraphics::Kitty => ("kitty", "0.36.0"),
        OuterGraphics::Iterm2 if term.contains("iterm") => ("iTerm2", "3.5"),
        OuterGraphics::Sixel if term.contains("foot") => ("foot", "1.18.0"),
        OuterGraphics::Sixel if term.contains("konsole") => ("Konsole", "24.0"),
        OuterGraphics::Sixel if term.contains("mlterm") => ("mlterm", "3.9"),
        OuterGraphics::Sixel if term.contains("mintty") => ("mintty", "3.7"),
        OuterGraphics::Sixel if term.contains("contour") => ("contour", "0.4"),
        _ => ("venti", env!("CARGO_PKG_VERSION")),
    }
}

/// best-effort detection of the terminal *outside* tmux (tmux's own client).
/// spawns one subprocess per host process; result is cached.
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
    // inside tmux/screen the real outer terminal is unknowable: wrap and let
    // the outer terminal decide.
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
    {
        return OuterGraphics::Kitty;
    }
    OuterGraphics::Unknown
}

/// graphics capability of the outer terminal (cached per process).
pub fn detect() -> OuterGraphics {
    static CACHED: OnceLock<OuterGraphics> = OnceLock::new();
    *CACHED.get_or_init(detect_uncached)
}

/// graphics capability to use when inside tmux, based on the outer terminal.
/// `None` when it can't be determined.
pub fn tmux_outer_graphics() -> Option<OuterGraphics> {
    if std::env::var_os("TMUX").is_some() {
        tmux_client_graphics()
    } else {
        None
    }
}

/// whether the outer terminal (or the one behind tmux) can render sixel.
pub fn sixel_supported() -> bool {
    match detect() {
        OuterGraphics::Sixel => true,
        OuterGraphics::Tmux => tmux_outer_graphics() == Some(OuterGraphics::Sixel),
        _ => false,
    }
}

/// whether the outer terminal (or the one behind tmux) can render kitty graphics.
pub fn kitty_supported() -> bool {
    match detect() {
        OuterGraphics::Kitty => true,
        OuterGraphics::Tmux => tmux_outer_graphics() == Some(OuterGraphics::Kitty),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_term_name_maps_known_terminals() {
        assert_eq!(from_term_name("xterm-kitty"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("ghostty"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("WezTerm"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("konsole"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("foot"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("mlterm"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("mintty"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("contour"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("iTerm2"), Some(OuterGraphics::Iterm2));
        assert_eq!(from_term_name("tmux"), None);
        assert_eq!(from_term_name("xterm-256color"), None);
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

        assert_eq!(from_client_term("xterm-256color"), OuterGraphics::Unknown);
        assert_eq!(
            child_term_for("xterm-256color", OuterGraphics::Unknown),
            "xterm-256color"
        );
    }
}

static CELL_PIXELS: AtomicU64 = AtomicU64::new(0);

/// cell pixel size used for graphics geometry. falls back to the historic
/// 10x20 size when the attached terminal never reports one.
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

/// records the attached terminal's cell pixel size when the client reports one.
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

/// graphics context resolved once per child spawn: which TERM to advertise and
/// whether the attached terminal can speak kitty/sixel/iterm2 inline graphics.
/// grouped so pty spawn paths don't take a growing list of extra parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsSpawn {
    /// TERM value the client reported to venti; advertised to the child.
    pub term: String,
    /// capability of the attached terminal (or the one behind tmux).
    pub graphics: OuterGraphics,
}

impl GraphicsSpawn {
    /// builds from the process TERM, defaulting to xterm-256color.
    pub fn from_env() -> Self {
        let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
        Self::from_term(&term)
    }

    /// builds from an explicit TERM value, e.g. for run-time spawn of a pane.
    pub fn from_term(term: &str) -> Self {
        Self {
            graphics: from_client_term(term),
            term: term.to_string(),
        }
    }

    /// the TERM value the child pane must advertise.
    pub fn child_term(&self) -> String {
        child_term_for(&self.term, self.graphics)
    }

    /// the TERM_PROGRAM value the child pane must advertise.
    pub fn child_term_program(&self) -> &'static str {
        child_term_program_for(self.graphics)
    }
}
