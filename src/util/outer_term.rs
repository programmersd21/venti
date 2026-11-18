//! Outer terminal graphics capability detection for Venti v0.2.

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
    /// iTerm2 graphics protocol: OSC 1337 File=...
    Iterm2,
    /// We are inside tmux; outer terminal decides via passthrough.
    Tmux,
    /// No graphics capability detected.
    #[default]
    Unknown,
}

pub fn from_code(code: u8) -> OuterGraphics {
    if code & (1 << 1) != 0 {
        OuterGraphics::Kitty
    } else if code & (1 << 2) != 0 {
        OuterGraphics::Iterm2
    } else if code & (1 << 0) != 0 {
        OuterGraphics::Sixel
    } else {
        OuterGraphics::Unknown
    }
}

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
        || t.contains("xterm")
    {
        return Some(OuterGraphics::Sixel);
    }
    None
}

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

pub fn child_term_program_for(graphics: OuterGraphics) -> &'static str {
    match graphics {
        OuterGraphics::Kitty => "kitty",
        _ => "venti",
    }
}

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
    OuterGraphics::Unknown
}

pub fn detect() -> OuterGraphics {
    static CACHED: OnceLock<OuterGraphics> = OnceLock::new();
    *CACHED.get_or_init(detect_uncached)
}

pub fn tmux_outer_graphics() -> Option<OuterGraphics> {
    if std::env::var_os("TMUX").is_some() {
        tmux_client_graphics()
    } else {
        None
    }
}

pub fn kitty_supported() -> bool {
    match detect() {
        OuterGraphics::Kitty => true,
        OuterGraphics::Tmux => tmux_outer_graphics() == Some(OuterGraphics::Kitty),
        _ => false,
    }
}

pub fn sixel_supported() -> bool {
    match detect() {
        OuterGraphics::Sixel => true,
        OuterGraphics::Tmux => tmux_outer_graphics() == Some(OuterGraphics::Sixel),
        _ => false,
    }
}

static CELL_PIXELS: AtomicU64 = AtomicU64::new(0);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsSpawn {
    pub term: String,
    pub graphics: OuterGraphics,
}

impl GraphicsSpawn {
    pub fn from_env() -> Self {
        let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
        Self::from_term(&term)
    }

    pub fn from_term(term: &str) -> Self {
        Self {
            graphics: from_client_term(term),
            term: term.to_string(),
        }
    }

    pub fn child_term(&self) -> String {
        child_term_for(&self.term, self.graphics)
    }

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
        assert_eq!(from_term_name("ghostty"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("wezterm"), Some(OuterGraphics::Kitty));
        assert_eq!(from_term_name("foot"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("konsole"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("mlterm"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("mintty"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("contour"), Some(OuterGraphics::Sixel));
        assert_eq!(from_term_name("tmux"), None);
        assert_eq!(from_term_name("xterm-256color"), Some(OuterGraphics::Sixel));
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
}
