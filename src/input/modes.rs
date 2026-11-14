use serde::{Deserialize, Serialize};
use std::fmt;

/// operational mode of the multiplexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum InputMode {
    #[default]
    Normal,
    Resize,
    Copy,
    Search,
    Palette,
    ThemeMenu,
    SettingsMenu,
    Help,
    /// vim-style `:command` prompt at bottom of screen.
    VimCmd,
    /// tab context menu (triggered by right-click on a workspace tab).
    ContextMenu,
}

impl fmt::Display for InputMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Normal => "normal",
            Self::Resize => "resize",
            Self::Copy => "copy",
            Self::Search => "search",
            Self::Palette => "palette",
            Self::ThemeMenu => "theme",
            Self::SettingsMenu => "settings",
            Self::Help => "help",
            Self::VimCmd => ":",
            Self::ContextMenu => "menu",
        };
        write!(f, "{s}")
    }
}
