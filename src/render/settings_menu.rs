use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::schema::Config;
use crate::config::theme::Theme;
use crate::render::style::{modal_footer, modal_frame};

/// geometry of the settings modal: outer rect, inner content rect, and the
/// number of setting rows that fit in the list. single source of truth for
/// the renderer and the mouse hit-tester.
pub const SETTINGS_MENU_WIDTH: u16 = 124;
pub const SETTINGS_MENU_HEIGHT: u16 = 32;
/// smallest terminal size at which the modal is drawn.
pub const SETTINGS_MENU_MIN_WIDTH: u16 = 60;
pub const SETTINGS_MENU_MIN_HEIGHT: u16 = 14;

pub fn settings_menu_geometry(area: Rect) -> (Rect, Rect, usize) {
    let width = SETTINGS_MENU_WIDTH.min(area.width.saturating_sub(2));
    let height = SETTINGS_MENU_HEIGHT.min(area.height.saturating_sub(2));
    let start_x = area.x + (area.width.saturating_sub(width)) / 2;
    let start_y = area.y + (area.height.saturating_sub(height)) / 2;
    let modal_rect = Rect::new(start_x, start_y, width, height);
    let inner = Rect::new(
        modal_rect.x + 1,
        modal_rect.y + 1,
        modal_rect.width.saturating_sub(2),
        modal_rect.height.saturating_sub(2),
    );
    let list_start = inner.y + 4;
    let last_row = inner.y + inner.height.saturating_sub(1);
    let max_display = last_row.saturating_sub(list_start).saturating_add(1) as usize;
    (modal_rect, inner, max_display)
}

/// tab bar layout: each tab's x position and width in cells, derived from the
/// same unicode width function the renderer uses.
pub fn settings_tab_layout(inner: Rect) -> Vec<(SettingTab, u16, u16)> {
    use unicode_width::UnicodeWidthStr;
    let mut tabs = Vec::with_capacity(4);
    let mut x = inner.x + 1;
    for tab in [
        SettingTab::Quick,
        SettingTab::Visuals,
        SettingTab::Layout,
        SettingTab::Advanced,
    ]
    .into_iter()
    {
        let label = format!(" {} ", tab.name());
        let w = UnicodeWidthStr::width(label.as_str()) as u16;
        tabs.push((tab, x, w));
        x += w + 1;
    }
    tabs
}

/// category tab in the settings menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingTab {
    Quick = 0,
    Visuals = 1,
    Layout = 2,
    Advanced = 3,
}

impl SettingTab {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Quick => "⚡ quick",
            Self::Visuals => "🎨 visuals",
            Self::Layout => "📐 layout",
            Self::Advanced => "⚙️ advanced",
        }
    }

    pub fn filtered_settings(&self) -> Vec<Setting> {
        let all = Setting::all();
        match self {
            Self::Quick => all
                .into_iter()
                .filter(|s| {
                    matches!(
                        s,
                        Setting::AnimationEnabled
                            | Setting::DimInactive
                            | Setting::HintBar
                            | Setting::ShowTabBar
                            | Setting::Clock
                            | Setting::Mouse
                            | Setting::CopyOnSelect
                            | Setting::Ascii
                            | Setting::Pet
                            | Setting::ShowWelcome
                            | Setting::Title
                            | Setting::Graphics
                            | Setting::SidebarCollapsed
                            | Setting::ShowHostname
                            | Setting::ShowPwd
                            | Setting::BorderStyle
                            | Setting::LayoutMode
                            | Setting::WorkspaceBar
                    )
                })
                .collect(),
            Self::Visuals => all
                .into_iter()
                .filter(|s| {
                    matches!(
                        s,
                        Setting::DimStrength
                            | Setting::Pet
                            | Setting::PetEmoji
                            | Setting::ScrollIndicator
                            | Setting::BorderStyle
                            | Setting::FrameIntervalMs
                            | Setting::SidebarWidth
                            | Setting::SidebarCollapsedWidth
                            | Setting::SidebarDensity
                            | Setting::ThemeName
                            | Setting::AnimationEnabled
                            | Setting::DimInactive
                            | Setting::HideBorderWhenSingle
                    )
                })
                .collect(),
            Self::Layout => all
                .into_iter()
                .filter(|s| {
                    matches!(
                        s,
                        Setting::ResizeStep
                            | Setting::GapInner
                            | Setting::GapOuter
                            | Setting::MinPaneCols
                            | Setting::MinPaneRows
                            | Setting::LayoutMode
                            | Setting::SplitAxis
                            | Setting::SidebarWidth
                    )
                })
                .collect(),
            Self::Advanced => all
                .into_iter()
                .filter(|s| {
                    matches!(
                        s,
                        Setting::Shell
                            | Setting::Scrollback
                            | Setting::Osc52
                            | Setting::DefaultWorkspaces
                            | Setting::ConfirmCloseRunning
                            | Setting::MaxClients
                            | Setting::MaxPanes
                            | Setting::MaxWorkspaces
                            | Setting::ProfileMode
                            | Setting::ScrollLines
                            | Setting::Title
                            | Setting::ClockFormat
                            | Setting::SidebarHeader
                            | Setting::ShowHostname
                            | Setting::LayoutDurationMs
                            | Setting::LayoutEasing
                            | Setting::ShowTimeInTitle
                    )
                })
                .collect(),
        }
    }
}

/// state for the settings menu overlay.
#[derive(Debug, Clone)]
pub struct SettingsMenuState {
    pub selected_idx: usize,
    pub editing: Option<SettingEdit>,
    pub scroll_offset: usize,
    pub filter: String,
    pub active_tab: SettingTab,
}

impl SettingsMenuState {
    /// settings of the active tab that match the current text filter.
    /// the renderer, the keyboard handler and the mouse handler all need the
    /// same list, so the filter lives here.
    pub fn visible_settings(&self) -> Vec<Setting> {
        let filter = self.filter.to_lowercase();
        let settings = self.active_tab.filtered_settings();
        if filter.is_empty() {
            return settings;
        }
        settings
            .into_iter()
            .filter(|st| {
                st.name().to_lowercase().contains(&filter)
                    || st.description().to_lowercase().contains(&filter)
                    || st.category().to_lowercase().contains(&filter)
            })
            .collect()
    }
}

impl Default for SettingsMenuState {
    fn default() -> Self {
        Self {
            selected_idx: 0,
            editing: None,
            scroll_offset: 0,
            filter: String::new(),
            active_tab: SettingTab::Quick,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingEdit {
    pub setting: Setting,
    pub buffer: String,
    pub is_numeric: bool, // true if using arrow keys for increment/decrement
}

impl SettingEdit {
    pub fn new(setting: Setting, buffer: String, is_numeric: bool) -> Self {
        Self {
            setting,
            buffer,
            is_numeric,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    Float,
    Enum,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    // general
    Shell,
    Scrollback,
    Osc52,
    Graphics,
    DefaultWorkspaces,
    ConfirmCloseRunning,
    MaxClients,
    MaxPanes,
    MaxWorkspaces,
    // profile
    ProfileMode,
    // input
    CopyOnSelect,
    Mouse,
    ScrollLines,
    // layout
    LayoutMode,
    SplitAxis,
    GapInner,
    GapOuter,
    ResizeStep,
    MinPaneCols,
    MinPaneRows,
    // decorations
    BorderStyle,
    HideBorderWhenSingle,
    DimInactive,
    DimStrength,
    Title,
    Pet,
    PetEmoji,
    ScrollIndicator,
    // animation
    AnimationEnabled,
    LayoutDurationMs,
    LayoutEasing,
    // theme
    ThemeName,
    // ui
    WorkspaceBar,
    HintBar,
    Clock,
    ClockFormat,
    Ascii,
    ShowWelcome,
    FrameIntervalMs,
    SidebarWidth,
    SidebarCollapsedWidth,
    SidebarHeader,
    SidebarCollapsed,
    SidebarDensity,
    ShowTabBar,
    ShowHostname,
    ShowTimeInTitle,
    ShowPwd,
}

impl Setting {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Scrollback => "scrollback",
            Self::Osc52 => "osc52",
            Self::Graphics => "graphics (experimental)",
            Self::DefaultWorkspaces => "default workspaces",
            Self::ConfirmCloseRunning => "confirm close running",
            Self::MaxClients => "max clients",
            Self::MaxPanes => "max panes",
            Self::MaxWorkspaces => "max workspaces",
            Self::ProfileMode => "profile mode",
            Self::CopyOnSelect => "copy on select",
            Self::Mouse => "mouse",
            Self::ScrollLines => "scroll lines",
            Self::LayoutMode => "layout mode",
            Self::SplitAxis => "split axis",
            Self::GapInner => "gap inner",
            Self::GapOuter => "gap outer",
            Self::ResizeStep => "resize step",
            Self::MinPaneCols => "min pane cols",
            Self::MinPaneRows => "min pane rows",
            Self::BorderStyle => "border style",
            Self::HideBorderWhenSingle => "hide border when single",
            Self::DimInactive => "dim inactive",
            Self::DimStrength => "dim strength",
            Self::Title => "title",
            Self::Pet => "pet",
            Self::PetEmoji => "pet emoji",
            Self::ScrollIndicator => "scroll indicator",
            Self::AnimationEnabled => "animations enabled",
            Self::LayoutDurationMs => "layout duration ms",
            Self::LayoutEasing => "layout easing",
            Self::ThemeName => "theme name",
            Self::WorkspaceBar => "workspace bar",
            Self::HintBar => "hint bar",
            Self::Clock => "clock",
            Self::ClockFormat => "clock format",
            Self::Ascii => "ascii",
            Self::ShowWelcome => "show welcome",
            Self::FrameIntervalMs => "frame interval ms",
            Self::SidebarWidth => "sidebar width",
            Self::SidebarCollapsedWidth => "sidebar collapsed width",
            Self::SidebarHeader => "sidebar header",
            Self::SidebarCollapsed => "sidebar collapsed",
            Self::SidebarDensity => "sidebar density",
            Self::ShowTabBar => "show tab bar",
            Self::ShowHostname => "show hostname",
            Self::ShowTimeInTitle => "show time in title",
            Self::ShowPwd => "show pwd",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Shell => "shell program",
            Self::Scrollback => "scrollback lines",
            Self::Osc52 => "osc52 clipboard",
            Self::Graphics => "experimental, set false if it glitches",
            Self::DefaultWorkspaces => "default workspace count",
            Self::ConfirmCloseRunning => "confirm close running",
            Self::MaxClients => "max clients",
            Self::MaxPanes => "max panes",
            Self::MaxWorkspaces => "max workspaces",
            Self::ProfileMode => "keybinding profile",
            Self::CopyOnSelect => "copy on select",
            Self::Mouse => "mouse support",
            Self::ScrollLines => "scroll lines",
            Self::LayoutMode => "layout mode",
            Self::SplitAxis => "split axis",
            Self::GapInner => "inner gap",
            Self::GapOuter => "outer gap",
            Self::ResizeStep => "resize step",
            Self::MinPaneCols => "min pane cols",
            Self::MinPaneRows => "min pane rows",
            Self::BorderStyle => "border style",
            Self::HideBorderWhenSingle => "hide border when single",
            Self::DimInactive => "dim inactive",
            Self::DimStrength => "dim strength",
            Self::Title => "pane title",
            Self::Pet => "show cute pet in focused pane title",
            Self::ScrollIndicator => "show scroll position in the pane title",
            Self::PetEmoji => "pet emoji (🐱🐶🦊🐰🐼🐻🐸🐵 chick etc)",
            Self::AnimationEnabled => "animations enabled",
            Self::LayoutDurationMs => "layout duration",
            Self::LayoutEasing => "layout easing",
            Self::ThemeName => "theme name",
            Self::WorkspaceBar => "workspace bar position",
            Self::HintBar => "hint bar",
            Self::Clock => "clock",
            Self::ClockFormat => "clock format",
            Self::Ascii => "ascii only",
            Self::ShowWelcome => "show welcome",
            Self::FrameIntervalMs => "frame interval ms",
            Self::SidebarWidth => "sidebar width",
            Self::SidebarCollapsedWidth => "sidebar collapsed width",
            Self::SidebarHeader => "sidebar header",
            Self::SidebarCollapsed => "sidebar collapsed",
            Self::SidebarDensity => "sidebar density",
            Self::ShowTabBar => "show tab bar",
            Self::ShowHostname => "show hostname",
            Self::ShowTimeInTitle => "show time in pane title",
            Self::ShowPwd => "show working directory",
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Shell
            | Self::Scrollback
            | Self::Osc52
            | Self::Graphics
            | Self::DefaultWorkspaces
            | Self::ConfirmCloseRunning
            | Self::MaxClients
            | Self::MaxPanes
            | Self::MaxWorkspaces => "general",
            Self::ProfileMode | Self::CopyOnSelect | Self::Mouse | Self::ScrollLines => "input",
            Self::LayoutMode
            | Self::SplitAxis
            | Self::GapInner
            | Self::GapOuter
            | Self::ResizeStep
            | Self::MinPaneCols
            | Self::MinPaneRows => "layout",
            Self::BorderStyle
            | Self::HideBorderWhenSingle
            | Self::DimInactive
            | Self::DimStrength
            | Self::Title => "decorations",
            Self::AnimationEnabled | Self::LayoutDurationMs | Self::LayoutEasing => "animation",
            Self::ThemeName => "theme",
            _ => "interface",
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Self::Scrollback
            | Self::DefaultWorkspaces
            | Self::MaxClients
            | Self::MaxPanes
            | Self::MaxWorkspaces
            | Self::ScrollLines
            | Self::GapInner
            | Self::GapOuter
            | Self::ResizeStep
            | Self::MinPaneCols
            | Self::MinPaneRows
            | Self::FrameIntervalMs
            | Self::SidebarWidth
            | Self::SidebarCollapsedWidth
            | Self::LayoutDurationMs => Kind::Int,
            Self::DimStrength => Kind::Float,
            Self::Osc52
            | Self::Graphics
            | Self::ConfirmCloseRunning
            | Self::CopyOnSelect
            | Self::Mouse
            | Self::HideBorderWhenSingle
            | Self::DimInactive
            | Self::Title
            | Self::Pet
            | Self::ScrollIndicator
            | Self::AnimationEnabled
            | Self::HintBar
            | Self::Clock
            | Self::Ascii
            | Self::ShowWelcome
            | Self::SidebarHeader
            | Self::SidebarCollapsed
            | Self::ShowTabBar
            | Self::ShowHostname
            | Self::ShowTimeInTitle
            | Self::ShowPwd => Kind::Bool,
            Self::ProfileMode
            | Self::LayoutMode
            | Self::SplitAxis
            | Self::BorderStyle
            | Self::LayoutEasing
            | Self::WorkspaceBar
            | Self::SidebarDensity
            | Self::PetEmoji => Kind::Enum,
            _ => Kind::Text,
        }
    }

    pub fn enum_options(&self) -> &'static [&'static str] {
        match self {
            Self::ProfileMode => &["beginner", "hypr", "vim", "dev"],
            Self::LayoutMode => &["dwindle", "monocle"],
            Self::SplitAxis => &["auto", "right", "down"],
            Self::BorderStyle => &["plain", "rounded", "thick", "double", "none"],
            Self::LayoutEasing => &["linear", "ease_out_cubic", "ease_in_out", "ease_out_quint"],
            Self::WorkspaceBar => &["top", "bottom", "left", "right", "off"],
            Self::SidebarDensity => &["compact", "comfortable"],
            Self::PetEmoji => &[
                "🐱", "🐶", "🦊", "🐰", "🐼", "🐻", "🐸", "🐵", "chicks", "custom",
            ],
            _ => &[],
        }
    }

    pub fn current_value(&self, config: &Config) -> String {
        match self {
            Self::Shell => {
                if config.general.shell.is_empty() {
                    "auto".to_string()
                } else {
                    config.general.shell.clone()
                }
            }
            Self::Scrollback => config.general.scrollback.to_string(),
            Self::Osc52 => config.general.osc52.to_string(),
            Self::Graphics => config.general.graphics.to_string(),
            Self::DefaultWorkspaces => config.general.default_workspaces.to_string(),
            Self::ConfirmCloseRunning => config.general.confirm_close_running.to_string(),
            Self::MaxClients => config.general.max_clients.to_string(),
            Self::MaxPanes => config.general.max_panes.to_string(),
            Self::MaxWorkspaces => config.general.max_workspaces.to_string(),
            Self::ProfileMode => config.profile.mode.to_string(),
            Self::CopyOnSelect => config.input.copy_on_select.to_string(),
            Self::Mouse => config.input.mouse.to_string(),
            Self::ScrollLines => config.input.scroll_lines.to_string(),
            Self::LayoutMode => config.layout.mode.to_string(),
            Self::SplitAxis => config.layout.split_axis.to_string(),
            Self::GapInner => config.layout.gap_inner.to_string(),
            Self::GapOuter => config.layout.gap_outer.to_string(),
            Self::ResizeStep => config.layout.resize_step.to_string(),
            Self::MinPaneCols => config.layout.min_pane_cols.to_string(),
            Self::MinPaneRows => config.layout.min_pane_rows.to_string(),
            Self::BorderStyle => config.decorations.border_style.to_string(),
            Self::HideBorderWhenSingle => config.decorations.hide_border_when_single.to_string(),
            Self::DimInactive => config.decorations.dim_inactive.to_string(),
            Self::DimStrength => format!("{:.2}", config.decorations.dim_strength),
            Self::Title => config.decorations.title.to_string(),
            Self::Pet => config.decorations.pet.to_string(),
            Self::ScrollIndicator => config.decorations.scroll_indicator.to_string(),
            Self::PetEmoji => {
                let presets = ["🐱", "🐶", "🦊", "🐰", "🐼", "🐻", "🐸", "🐵", "chicks"];
                if presets.contains(&config.decorations.pet_emoji.as_str()) {
                    config.decorations.pet_emoji.clone()
                } else {
                    "custom".to_string()
                }
            }
            Self::AnimationEnabled => config.animation.enabled.to_string(),
            Self::LayoutDurationMs => config.animation.layout_duration_ms.to_string(),
            Self::LayoutEasing => config.animation.layout_easing.to_string(),
            Self::ThemeName => config.theme.name.clone(),
            Self::WorkspaceBar => config.ui.workspace_bar.to_string(),
            Self::HintBar => config.ui.hint_bar.to_string(),
            Self::Clock => config.ui.clock.to_string(),
            Self::ClockFormat => config.ui.clock_format.clone(),
            Self::Ascii => config.ui.ascii.to_string(),
            Self::ShowWelcome => config.ui.show_welcome.to_string(),
            Self::FrameIntervalMs => config.ui.frame_interval_ms.to_string(),
            Self::SidebarWidth => config.ui.sidebar_width.to_string(),
            Self::SidebarCollapsedWidth => config.ui.sidebar_collapsed_width.to_string(),
            Self::SidebarHeader => config.ui.sidebar_header.to_string(),
            Self::SidebarCollapsed => config.ui.sidebar_collapsed.to_string(),
            Self::SidebarDensity => config.ui.sidebar_density.to_string(),
            Self::ShowTabBar => config.ui.show_tab_bar.to_string(),
            Self::ShowHostname => config.ui.show_hostname.to_string(),
            Self::ShowTimeInTitle => config.ui.show_time_in_title.to_string(),
            Self::ShowPwd => config.ui.show_pwd.to_string(),
        }
    }

    pub fn is_bool(&self) -> bool {
        self.kind() == Kind::Bool
    }

    pub fn is_enum(&self) -> bool {
        self.kind() == Kind::Enum
    }

    /// returns true if this setting can be incremented/decremented with arrow keys.
    pub fn supports_arrow_increment(&self) -> bool {
        matches!(
            self,
            Self::FrameIntervalMs
                | Self::ResizeStep
                | Self::DimStrength
                | Self::LayoutDurationMs
                | Self::GapInner
                | Self::GapOuter
                | Self::MinPaneCols
                | Self::MinPaneRows
                | Self::SidebarWidth
        )
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::Shell,
            Self::Scrollback,
            Self::Osc52,
            Self::Graphics,
            Self::DefaultWorkspaces,
            Self::ConfirmCloseRunning,
            Self::MaxClients,
            Self::MaxPanes,
            Self::MaxWorkspaces,
            Self::ProfileMode,
            Self::CopyOnSelect,
            Self::Mouse,
            Self::ScrollLines,
            Self::LayoutMode,
            Self::SplitAxis,
            Self::GapInner,
            Self::GapOuter,
            Self::ResizeStep,
            Self::MinPaneCols,
            Self::MinPaneRows,
            Self::BorderStyle,
            Self::HideBorderWhenSingle,
            Self::DimInactive,
            Self::DimStrength,
            Self::Title,
            Self::Pet,
            Self::PetEmoji,
            Self::AnimationEnabled,
            Self::LayoutDurationMs,
            Self::LayoutEasing,
            Self::ThemeName,
            Self::WorkspaceBar,
            Self::HintBar,
            Self::Clock,
            Self::ClockFormat,
            Self::Ascii,
            Self::ShowWelcome,
            Self::FrameIntervalMs,
            Self::SidebarWidth,
            Self::SidebarCollapsedWidth,
            Self::SidebarHeader,
            Self::SidebarCollapsed,
            Self::SidebarDensity,
            Self::ShowTabBar,
            Self::ShowHostname,
            Self::ShowTimeInTitle,
            Self::ShowPwd,
        ]
    }

    /// apply a text value; returns Ok(()) or an error message.
    pub fn apply(&self, config: &mut Config, value: &str) -> Result<(), String> {
        let v = value.trim();
        match self {
            Self::Shell => {
                config.general.shell = if v.eq_ignore_ascii_case("auto") {
                    String::new()
                } else {
                    v.to_string()
                };
                Ok(())
            }
            Self::Scrollback => {
                let val: usize = v.parse().map_err(|_| "invalid number")?;
                config.general.scrollback = val;
                Ok(())
            }
            Self::Osc52 => parse_bool(v, &mut config.general.osc52),
            Self::Graphics => parse_bool(v, &mut config.general.graphics),
            Self::DefaultWorkspaces => {
                let val: usize = v.parse().map_err(|_| "invalid number")?;
                config.general.default_workspaces = val;
                Ok(())
            }
            Self::ConfirmCloseRunning => parse_bool(v, &mut config.general.confirm_close_running),
            Self::MaxClients => {
                let val: usize = v.parse().map_err(|_| "invalid number")?;
                config.general.max_clients = val;
                Ok(())
            }
            Self::MaxPanes => {
                let val: usize = v.parse().map_err(|_| "invalid number")?;
                config.general.max_panes = val;
                Ok(())
            }
            Self::MaxWorkspaces => {
                let val: usize = v.parse().map_err(|_| "invalid number")?;
                config.general.max_workspaces = val;
                Ok(())
            }
            Self::ProfileMode => {
                let lower = v.to_lowercase();
                let m = match lower.as_str() {
                    "beginner" => crate::config::schema::ProfileMode::Beginner,
                    "hypr" => crate::config::schema::ProfileMode::Hypr,
                    "vim" => crate::config::schema::ProfileMode::Vim,
                    "dev" => crate::config::schema::ProfileMode::Dev,
                    _ => return Err("must be: beginner, hypr, vim, dev".into()),
                };
                config.profile.mode = m;
                Ok(())
            }
            Self::CopyOnSelect => parse_bool(v, &mut config.input.copy_on_select),
            Self::Mouse => parse_bool(v, &mut config.input.mouse),
            Self::ScrollLines => {
                let val: usize = v.parse().map_err(|_| "invalid number")?;
                config.input.scroll_lines = val;
                Ok(())
            }
            Self::LayoutMode => {
                let lower = v.to_lowercase();
                let m = match lower.as_str() {
                    "dwindle" => crate::config::schema::LayoutMode::Dwindle,
                    "monocle" => crate::config::schema::LayoutMode::Monocle,
                    _ => return Err("must be: dwindle, monocle".into()),
                };
                config.layout.mode = m;
                Ok(())
            }
            Self::SplitAxis => {
                let lower = v.to_lowercase();
                let m = match lower.as_str() {
                    "auto" => crate::config::schema::SplitAxisSetting::Auto,
                    "right" => crate::config::schema::SplitAxisSetting::Right,
                    "down" => crate::config::schema::SplitAxisSetting::Down,
                    _ => return Err("must be: auto, right, down".into()),
                };
                config.layout.split_axis = m;
                Ok(())
            }
            Self::GapInner => {
                let val: u16 = v.parse().map_err(|_| "invalid number")?;
                config.layout.gap_inner = val;
                Ok(())
            }
            Self::GapOuter => {
                let val: u16 = v.parse().map_err(|_| "invalid number")?;
                config.layout.gap_outer = val;
                Ok(())
            }
            Self::ResizeStep => {
                let val: u8 = v.parse().map_err(|_| "invalid number")?;
                if (1..=100).contains(&val) {
                    config.layout.resize_step = val;
                    Ok(())
                } else {
                    Err("must be 1-100".into())
                }
            }
            Self::MinPaneCols => {
                let val: u16 = v.parse().map_err(|_| "invalid number")?;
                if val >= 6 {
                    config.layout.min_pane_cols = val;
                    Ok(())
                } else {
                    Err("must be >=6".into())
                }
            }
            Self::MinPaneRows => {
                let val: u16 = v.parse().map_err(|_| "invalid number")?;
                if val >= 2 {
                    config.layout.min_pane_rows = val;
                    Ok(())
                } else {
                    Err("must be >=2".into())
                }
            }
            Self::BorderStyle => {
                let lower = v.to_lowercase();
                let m = match lower.as_str() {
                    "plain" => crate::config::schema::BorderStyle::Plain,
                    "rounded" => crate::config::schema::BorderStyle::Rounded,
                    "thick" => crate::config::schema::BorderStyle::Thick,
                    "double" => crate::config::schema::BorderStyle::Double,
                    "none" => crate::config::schema::BorderStyle::None,
                    _ => return Err("must be: plain, rounded, thick, double, none".into()),
                };
                config.decorations.border_style = m;
                Ok(())
            }
            Self::HideBorderWhenSingle => {
                parse_bool(v, &mut config.decorations.hide_border_when_single)
            }
            Self::DimInactive => parse_bool(v, &mut config.decorations.dim_inactive),
            Self::DimStrength => {
                let val: f64 = v.parse().map_err(|_| "invalid number")?;
                if (0.0..=1.0).contains(&val) {
                    config.decorations.dim_strength = val;
                    Ok(())
                } else {
                    Err("must be 0.0-1.0".into())
                }
            }
            Self::Title => parse_bool(v, &mut config.decorations.title),
            Self::Pet => parse_bool(v, &mut config.decorations.pet),
            Self::ScrollIndicator => parse_bool(v, &mut config.decorations.scroll_indicator),
            Self::PetEmoji => {
                if v == "custom" {
                    // keep the current emoji; the user can edit it with enter
                    Ok(())
                } else {
                    config.decorations.pet_emoji = v.to_string();
                    Ok(())
                }
            }
            Self::AnimationEnabled => parse_bool(v, &mut config.animation.enabled),
            Self::LayoutDurationMs => {
                let val: u64 = v.parse().map_err(|_| "invalid number")?;
                config.animation.layout_duration_ms = val;
                Ok(())
            }
            Self::LayoutEasing => parse_easing(v, &mut config.animation.layout_easing),
            Self::ThemeName => {
                if crate::config::theme::Theme::builtin(v).is_some() {
                    config.theme.name = v.to_string();
                    Ok(())
                } else {
                    Err("unknown theme".into())
                }
            }
            Self::WorkspaceBar => {
                let lower = v.to_lowercase();
                let m = match lower.as_str() {
                    "top" => crate::config::schema::WorkspaceBarPosition::Top,
                    "bottom" => crate::config::schema::WorkspaceBarPosition::Bottom,
                    "left" => crate::config::schema::WorkspaceBarPosition::Left,
                    "right" => crate::config::schema::WorkspaceBarPosition::Right,
                    "off" => crate::config::schema::WorkspaceBarPosition::Off,
                    _ => return Err("must be: top, bottom, left, right, off".into()),
                };
                config.ui.workspace_bar = m;
                Ok(())
            }
            Self::HintBar => parse_bool(v, &mut config.ui.hint_bar),
            Self::Clock => parse_bool(v, &mut config.ui.clock),
            Self::ClockFormat => {
                config.ui.clock_format = v.to_string();
                Ok(())
            }
            Self::Ascii => parse_bool(v, &mut config.ui.ascii),
            Self::ShowWelcome => parse_bool(v, &mut config.ui.show_welcome),
            Self::FrameIntervalMs => {
                let val: u64 = v.parse().map_err(|_| "invalid number")?;
                if (4..=100).contains(&val) {
                    config.ui.frame_interval_ms = val;
                    Ok(())
                } else {
                    Err("must be 4-100".into())
                }
            }
            Self::SidebarWidth => {
                let val: u16 = v.parse().map_err(|_| "invalid number")?;
                config.ui.sidebar_width = val;
                Ok(())
            }
            Self::SidebarCollapsedWidth => {
                let val: u16 = v.parse().map_err(|_| "invalid number")?;
                config.ui.sidebar_collapsed_width = val;
                Ok(())
            }
            Self::SidebarHeader => parse_bool(v, &mut config.ui.sidebar_header),
            Self::SidebarCollapsed => parse_bool(v, &mut config.ui.sidebar_collapsed),
            Self::SidebarDensity => {
                let lower = v.to_lowercase();
                let m = match lower.as_str() {
                    "compact" => crate::config::schema::SidebarDensity::Compact,
                    "comfortable" => crate::config::schema::SidebarDensity::Comfortable,
                    _ => return Err("must be: compact, comfortable".into()),
                };
                config.ui.sidebar_density = m;
                Ok(())
            }
            Self::ShowTabBar => parse_bool(v, &mut config.ui.show_tab_bar),
            Self::ShowHostname => parse_bool(v, &mut config.ui.show_hostname),
            Self::ShowTimeInTitle => parse_bool(v, &mut config.ui.show_time_in_title),
            Self::ShowPwd => parse_bool(v, &mut config.ui.show_pwd),
        }
    }
}

fn parse_bool(v: &str, target: &mut bool) -> Result<(), String> {
    let lower = v.to_lowercase();
    match lower.as_str() {
        "true" | "1" | "yes" | "on" => {
            *target = true;
            Ok(())
        }
        "false" | "0" | "no" | "off" => {
            *target = false;
            Ok(())
        }
        _ => Err("must be: true/false".into()),
    }
}

fn parse_easing(v: &str, target: &mut crate::config::schema::EasingType) -> Result<(), String> {
    let lower = v.to_lowercase();
    let e = match lower.as_str() {
        "linear" => crate::config::schema::EasingType::Linear,
        "ease_out_cubic" | "ease_out" => crate::config::schema::EasingType::EaseOutCubic,
        "ease_in_out" => crate::config::schema::EasingType::EaseInOut,
        "ease_out_quint" => crate::config::schema::EasingType::EaseOutQuint,
        _ => return Err("must be: linear, ease_out_cubic, ease_in_out, ease_out_quint".into()),
    };
    *target = e;
    Ok(())
}

/// renders the settings menu modal.
pub fn render_settings_menu(
    area: Rect,
    buf: &mut Buffer,
    state: &SettingsMenuState,
    config: &Config,
    theme: &Theme,
    ascii: bool,
) {
    let (modal_rect, inner, max_display) = settings_menu_geometry(area);
    if inner.width < SETTINGS_MENU_MIN_WIDTH || inner.height < SETTINGS_MENU_MIN_HEIGHT {
        return;
    }

    let inner = modal_frame(buf, modal_rect, theme, ascii, Some("settings"));

    let tab_y = inner.y;
    for (tab, tab_x, tab_w) in settings_tab_layout(inner) {
        let is_active = tab == state.active_tab;
        let tab_style = if is_active {
            Style::default()
                .fg(theme.accent_color())
                .bg(Color::Reset)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted_color()).bg(Color::Reset)
        };
        let tab_label = format!(" {} ", tab.name());
        buf.set_stringn(tab_x, tab_y, &tab_label, usize::from(tab_w), tab_style);
    }

    for x in inner.x..inner.x + inner.width {
        buf[(x, tab_y + 1)]
            .set_char('─')
            .set_style(Style::default().fg(theme.overlay_color()).bg(Color::Reset));
    }

    let cat_color = |cat: &str| -> Color {
        match cat {
            "general" => theme.info_color(),
            "input" => theme.success_color(),
            "layout" => theme.accent_color(),
            "decorations" => theme.warn_color(),
            "animation" => theme.alert_color(),
            "interface" => theme.accent_color(),
            "theme" => theme.success_color(),
            _ => theme.text_color(),
        }
    };

    let settings = state.visible_settings();

    // filter input row (below tabs)
    let filter_row_y = inner.y + 2;
    {
        let prompt_style = Style::default().fg(theme.accent_color()).bg(Color::Reset);
        buf[(inner.x, filter_row_y)]
            .set_char('›')
            .set_style(prompt_style.add_modifier(Modifier::BOLD));
        if state.filter.is_empty() {
            buf.set_stringn(
                inner.x + 2,
                filter_row_y,
                "type to filter…",
                (inner.width.saturating_sub(2)) as usize,
                Style::default().fg(theme.muted_color()).bg(Color::Reset),
            );
        } else {
            buf.set_stringn(
                inner.x + 2,
                filter_row_y,
                &state.filter,
                (inner.width.saturating_sub(2)) as usize,
                Style::default().fg(theme.text_color()).bg(Color::Reset),
            );
        }
    }
    for x in inner.x..inner.x + inner.width {
        buf[(x, filter_row_y + 1)]
            .set_char('─')
            .set_style(Style::default().fg(theme.overlay_color()).bg(Color::Reset));
    }

    let list_start = inner.y + 4;
    let last_row = inner.y + inner.height.saturating_sub(1);

    let selected = if settings.is_empty() {
        0
    } else {
        state.selected_idx.min(settings.len().saturating_sub(1))
    };
    let offset = if settings.is_empty() {
        0
    } else {
        state
            .scroll_offset
            .min(settings.len().saturating_sub(max_display.max(1)))
    };

    for (row, setting) in settings.iter().skip(offset).take(max_display).enumerate() {
        let idx = offset + row;
        let y = list_start + row as u16;
        let is_selected = idx == selected;
        let is_editing = state
            .editing
            .as_ref()
            .is_some_and(|e| e.setting == *setting);

        let (name_style, value_style, desc_style, bg) = if is_editing {
            (
                Style::default()
                    .fg(theme.accent_color())
                    .bg(theme.surface_color())
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(theme.warn_color())
                    .bg(theme.surface_color())
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                Style::default()
                    .fg(theme.subtext_color())
                    .bg(theme.surface_color()),
                theme.surface_color(),
            )
        } else if is_selected {
            (
                Style::default()
                    .fg(theme.accent_color())
                    .bg(theme.surface_color())
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(theme.text_color())
                    .bg(theme.surface_color()),
                Style::default()
                    .fg(theme.subtext_color())
                    .bg(theme.surface_color()),
                theme.surface_color(),
            )
        } else {
            (
                Style::default().fg(theme.accent_color()).bg(Color::Reset),
                Style::default().fg(theme.text_color()).bg(Color::Reset),
                Style::default().fg(theme.subtext_color()).bg(Color::Reset),
                Color::Reset,
            )
        };

        if is_selected || is_editing {
            for x in inner.x..inner.x + inner.width {
                buf[(x, y)].set_char(' ').set_style(Style::default().bg(bg));
            }
        }

        let marker = if is_editing {
            "✎ "
        } else if is_selected {
            "› "
        } else {
            "  "
        };
        buf.set_stringn(inner.x, y, marker, 2, name_style);

        let cat = setting.category();
        let cat_w = 11usize;
        buf.set_stringn(
            inner.x + 2,
            y,
            cat,
            cat_w,
            Style::default().fg(cat_color(cat)).bg(bg),
        );

        let name_w = 30usize;
        buf.set_stringn(
            inner.x + 2 + cat_w as u16 + 1,
            y,
            setting.name(),
            name_w,
            name_style,
        );

        let value_str = if is_editing {
            match &state.editing {
                Some(edit) => format!("{}▏", edit.buffer),
                None => String::new(),
            }
        } else {
            setting.current_value(config)
        };
        let value_col = inner.x + 2 + cat_w as u16 + 1 + name_w as u16 + 1;
        let value_w = 26usize;
        buf.set_stringn(value_col, y, &value_str, value_w, value_style);

        if inner.width > 80 {
            buf.set_stringn(
                value_col + value_w as u16 + 1,
                y,
                setting.description(),
                (inner.x + inner.width).saturating_sub(value_col + value_w as u16 + 2) as usize,
                desc_style,
            );
        }
    }

    let indicator_style = Style::default().fg(theme.muted_color()).bg(Color::Reset);
    if offset > 0 {
        buf[(inner.x + inner.width - 1, list_start)]
            .set_char('↑')
            .set_style(indicator_style);
    }
    if offset + max_display < settings.len() {
        buf[(inner.x + inner.width - 1, last_row)]
            .set_char('↓')
            .set_style(indicator_style);
    }

    let footer = if state.editing.is_some() {
        "↵ apply  ⎋ cancel  ← →   step"
    } else if !state.filter.is_empty() {
        "↑ ↓ navigate  space toggle  enter edit  ⎋ clear"
    } else {
        "↑ ↓ navigate  tab/shift+tab/alt+1-4  space toggle  enter edit  ← → cycle  ⎋ exit"
    };
    modal_footer(buf, modal_rect, theme, footer);
}
