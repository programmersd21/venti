use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// general session and system options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub shell: String,
    pub scrollback: usize,
    pub osc52: bool,
    /// forward sixel / kitty / iterm2 graphics escape sequences from panes to
    /// clients verbatim. when false, images are dropped instead of drawn.
    ///
    /// off by default: terminal graphics support is experimental and a
    /// misbehaving image protocol can garble the screen. set it to true to
    /// try it, and to false again if images misbehave.
    pub graphics: bool,
    pub default_workspaces: usize,
    pub workspace_names: Vec<String>,
    pub confirm_close_running: bool,
    pub max_clients: usize,
    pub max_panes: usize,
    pub max_workspaces: usize,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            shell: String::new(),
            scrollback: 10000,
            osc52: true,
            graphics: true,
            default_workspaces: 1,
            workspace_names: vec!["ws1".to_string()],
            confirm_close_running: true,
            max_clients: 32,
            max_panes: 1000,
            max_workspaces: 100,
        }
    }
}

impl std::fmt::Display for ProfileMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Beginner => write!(f, "beginner"),
            Self::Hypr => write!(f, "hypr"),
            Self::Vim => write!(f, "vim"),
            Self::Dev => write!(f, "dev"),
        }
    }
}

/// keybinding profile choice: genuinely different ergonomics per profile, not
/// cosmetic renames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileMode {
    #[default]
    Beginner,
    Hypr,
    Vim,
    Dev,
}

/// profile settings block.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileConfig {
    pub mode: ProfileMode,
}

/// input behavior settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputConfig {
    pub copy_on_select: bool,
    pub mouse: bool,
    pub passthrough: Vec<String>,
    pub scroll_lines: usize,
    /// keys to disable/ignore (e.g. ["alt+shift+q"] to override Kitty)
    pub disabled_keys: Vec<String>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            copy_on_select: true,
            mouse: true,
            passthrough: Vec::new(),
            scroll_lines: 3,
            disabled_keys: Vec::new(),
        }
    }
}

impl std::fmt::Display for LayoutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dwindle => write!(f, "dwindle"),
            Self::Monocle => write!(f, "monocle"),
        }
    }
}

/// layout mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    #[default]
    Dwindle,
    Monocle,
}

impl std::fmt::Display for SplitAxisSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Right => write!(f, "right"),
            Self::Down => write!(f, "down"),
        }
    }
}

/// split axis policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitAxisSetting {
    Auto,
    #[default]
    Right,
    Down,
}

/// layout and tiling geometry configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    pub mode: LayoutMode,
    pub split_axis: SplitAxisSetting,
    pub gap_inner: u16,
    pub gap_outer: u16,
    pub resize_step: u8,
    pub min_pane_cols: u16,
    pub min_pane_rows: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Dwindle,
            split_axis: SplitAxisSetting::Right,
            gap_inner: 0,
            gap_outer: 1,
            resize_step: 5,
            min_pane_cols: 6,
            min_pane_rows: 2,
        }
    }
}

impl std::fmt::Display for BorderStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plain => write!(f, "plain"),
            Self::Rounded => write!(f, "rounded"),
            Self::Thick => write!(f, "thick"),
            Self::Double => write!(f, "double"),
            Self::None => write!(f, "none"),
        }
    }
}

/// pane border decoration styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderStyle {
    Plain,
    #[default]
    Rounded,
    Thick,
    Double,
    None,
}

/// visual pane chrome options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DecorationsConfig {
    pub border_style: BorderStyle,
    pub hide_border_when_single: bool,
    pub dim_inactive: bool,
    pub dim_strength: f64,
    pub title: bool,
    /// show a cute pet emoji in the focused pane's title
    pub pet: bool,
    /// emoji used for the pane pet
    pub pet_emoji: String,
    /// show how far the pane is scrolled back (right side of the title bar).
    /// off by default; when on, panes scrolled above the live bottom show e.g.
    /// "↑12" until you return to the bottom.
    pub scroll_indicator: bool,
}

impl Default for DecorationsConfig {
    fn default() -> Self {
        Self {
            border_style: BorderStyle::Rounded,
            hide_border_when_single: false,
            dim_inactive: true,
            dim_strength: 0.18,
            title: true,
            pet: true,
            pet_emoji: "🐱".to_string(),
            scroll_indicator: false,
        }
    }
}

/// animation curve easing types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EasingType {
    Linear,
    #[default]
    EaseOutCubic,
    EaseInOut,
    EaseOutQuint,
}

impl std::fmt::Display for EasingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Linear => write!(f, "linear"),
            Self::EaseOutCubic => write!(f, "ease_out_cubic"),
            Self::EaseInOut => write!(f, "ease_in_out"),
            Self::EaseOutQuint => write!(f, "ease_out_quint"),
        }
    }
}

/// pane layout change animation options (swaps, splits, resizes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationConfig {
    pub enabled: bool,
    /// pane layout change animation duration in ms.
    pub layout_duration_ms: u64,
    pub layout_easing: EasingType,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            layout_duration_ms: 320,
            layout_easing: EasingType::EaseOutCubic,
        }
    }
}

/// theme configuration holding name and optional color overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub base: Option<String>,
    pub mantle: Option<String>,
    pub surface: Option<String>,
    pub overlay: Option<String>,
    pub text: Option<String>,
    pub subtext: Option<String>,
    pub muted: Option<String>,
    pub accent: Option<String>,
    pub info: Option<String>,
    pub success: Option<String>,
    pub warn: Option<String>,
    pub alert: Option<String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "mocha".to_string(),
            base: None,
            mantle: None,
            surface: None,
            overlay: None,
            text: None,
            subtext: None,
            muted: None,
            accent: None,
            info: None,
            success: None,
            warn: None,
            alert: None,
        }
    }
}

impl std::fmt::Display for WorkspaceBarPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Top => write!(f, "top"),
            Self::Bottom => write!(f, "bottom"),
            Self::Left => write!(f, "left"),
            Self::Right => write!(f, "right"),
            Self::Off => write!(f, "off"),
        }
    }
}

/// workspace bar placement position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceBarPosition {
    #[default]
    Top,
    Bottom,
    /// vertical workspace tab list on the left edge.
    Left,
    /// vertical sidebar on the right edge.
    Right,
    Off,
}

impl std::fmt::Display for SidebarDensity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compact => write!(f, "compact"),
            Self::Comfortable => write!(f, "comfortable"),
        }
    }
}

/// density of workspace rows in the vertical sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarDensity {
    /// two rows per workspace: ` [1] name      × ` / `   focused pane title `
    #[default]
    Comfortable,
    /// one row per workspace: ` 1 name        × `
    Compact,
}

/// ui presentation parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// where the workspace bar goes: "top", "bottom", "left", "right", or "off".
    /// "left"/"right" render the vertical workspace tab list with sidebar_* options.
    pub workspace_bar: WorkspaceBarPosition,
    pub hint_bar: bool,
    pub clock: bool,
    pub clock_format: String,
    pub ascii: bool,
    pub show_tab_bar: bool,
    pub show_hostname: bool,
    pub show_time_in_title: bool,
    pub show_pwd: bool,
    pub show_welcome: bool,
    pub frame_interval_ms: u64,
    /// width of the vertical sidebar in character columns (only used when
    /// workspace_bar is "left" or "right").
    pub sidebar_width: u16,
    /// width of the collapsed sidebar (icon rail) in character columns.
    pub sidebar_collapsed_width: u16,
    /// render the session name header row at the top of the sidebar.
    pub sidebar_header: bool,
    /// start with the sidebar collapsed to a narrow icon rail (toggle with the
    /// toggle_sidebar command, alt+s by default).
    pub sidebar_collapsed: bool,
    /// row density: "compact" (one row per tab) or "comfortable" (two rows: tab
    /// + focused pane title).
    pub sidebar_density: SidebarDensity,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            workspace_bar: WorkspaceBarPosition::Top,
            hint_bar: true,
            clock: true,
            clock_format: "%H:%M".to_string(),
            ascii: false,
            show_tab_bar: true,
            show_hostname: true,
            show_time_in_title: false,
            show_pwd: true,
            show_welcome: true,
            frame_interval_ms: 8,
            sidebar_width: 28,
            sidebar_collapsed_width: 4,
            sidebar_header: true,
            sidebar_collapsed: false,
            sidebar_density: SidebarDensity::Comfortable,
        }
    }
}

/// complete configuration schema for venti.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub profile: ProfileConfig,
    pub input: InputConfig,
    pub layout: LayoutConfig,
    pub decorations: DecorationsConfig,
    pub animation: AnimationConfig,
    pub theme: ThemeConfig,
    pub ui: UiConfig,
    pub binds: HashMap<String, String>,
}
