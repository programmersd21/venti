use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// all executable commands in venti.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Command {
    NewPane,
    SplitRight,
    SplitDown,
    ClosePane,
    FocusLeft,
    FocusDown,
    FocusUp,
    FocusRight,
    FocusNext,
    FocusPrev,
    SwapLeft,
    SwapDown,
    SwapUp,
    SwapRight,
    ResizeLeft,
    ResizeDown,
    ResizeUp,
    ResizeRight,
    ResizeMode,
    Equalize,
    RotateSplit,
    ToggleZoom,
    ToggleSidebar,
    ToggleFloat,
    FloatCenter,
    WorkspaceGoto(Vec<usize>),
    WorkspaceNext,
    WorkspacePrev,
    NewWorkspace,
    CloseWorkspace(Vec<usize>),
    MoveToWorkspace(Vec<usize>),
    MoveToWorkspaceNext,
    MoveToWorkspacePrev,
    RenameWorkspace(String),
    ToggleScratch,
    CopyMode,
    Search,
    PasteBuffer,
    CommandPalette,
    ThemeMenu,
    SettingsMenu,
    Help,
    ReloadConfig,
    Detach,
    KillSession,
    ShowWelcome,
    Cat,
}

impl Command {
    /// returns true if `name` is a known command name, aliases included.
    /// probes [`Command::from_str`] with a synthetic argument for the
    /// parameterized commands, so this can never drift from the parser.
    pub fn is_known_name(name: &str) -> bool {
        Self::from_str(name).is_ok()
            || Self::from_str(&format!("{name} 1")).is_ok()
            || Self::from_str(&format!("{name} word")).is_ok()
    }
}

fn parse_workspace_list(parts: std::str::SplitWhitespace<'_>) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for tok in parts {
        let n: usize = tok
            .parse()
            .map_err(|_| format!("invalid workspace number: {tok}"))?;
        if n == 0 {
            return Err("workspace numbers start at 1".to_string());
        }
        out.push(n);
    }
    Ok(out)
}

impl FromStr for Command {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split_whitespace();
        let name = parts.next().ok_or_else(|| "empty command".to_string())?;

        match name {
            "new_pane" => Ok(Command::NewPane),
            "split_right" => Ok(Command::SplitRight),
            "split_down" => Ok(Command::SplitDown),
            "close_pane" => Ok(Command::ClosePane),
            "focus_left" => Ok(Command::FocusLeft),
            "focus_down" => Ok(Command::FocusDown),
            "focus_up" => Ok(Command::FocusUp),
            "focus_right" => Ok(Command::FocusRight),
            "focus_next" => Ok(Command::FocusNext),
            "focus_prev" => Ok(Command::FocusPrev),
            "swap_left" => Ok(Command::SwapLeft),
            "swap_down" => Ok(Command::SwapDown),
            "swap_up" => Ok(Command::SwapUp),
            "swap_right" => Ok(Command::SwapRight),
            "resize_left" => Ok(Command::ResizeLeft),
            "resize_down" => Ok(Command::ResizeDown),
            "resize_up" => Ok(Command::ResizeUp),
            "resize_right" => Ok(Command::ResizeRight),
            "resize_mode" => Ok(Command::ResizeMode),
            "equalize" => Ok(Command::Equalize),
            "rotate_split" => Ok(Command::RotateSplit),
            "toggle_zoom" => Ok(Command::ToggleZoom),
            "toggle_sidebar" => Ok(Command::ToggleSidebar),
            "toggle_float" => Ok(Command::ToggleFloat),
            "float_center" => Ok(Command::FloatCenter),
            "workspace_goto" => {
                let nums = parse_workspace_list(parts)?;
                if nums.is_empty() {
                    return Err("workspace_goto requires at least one workspace number".to_string());
                }
                Ok(Command::WorkspaceGoto(nums))
            }
            "workspace_next" => Ok(Command::WorkspaceNext),
            "workspace_prev" => Ok(Command::WorkspacePrev),
            "new_workspace" => Ok(Command::NewWorkspace),
            "close_workspace" => Ok(Command::CloseWorkspace(parse_workspace_list(parts)?)),
            "move_to_workspace" => {
                let nums = parse_workspace_list(parts)?;
                if nums.is_empty() {
                    return Err(
                        "move_to_workspace requires at least one workspace number".to_string()
                    );
                }
                Ok(Command::MoveToWorkspace(nums))
            }
            "move_to_workspace_next" => Ok(Command::MoveToWorkspaceNext),
            "move_to_workspace_prev" => Ok(Command::MoveToWorkspacePrev),
            "rename_workspace" => {
                let arg: Vec<&str> = parts.collect();
                let name = arg.join(" ").trim().to_string();
                if name.is_empty() {
                    return Err("rename_workspace requires a name".to_string());
                }
                Ok(Command::RenameWorkspace(name))
            }
            "toggle_scratch" => Ok(Command::ToggleScratch),
            "copy_mode" => Ok(Command::CopyMode),
            "search" => Ok(Command::Search),
            "paste_buffer" => Ok(Command::PasteBuffer),
            "command_palette" => Ok(Command::CommandPalette),
            "theme_menu" => Ok(Command::ThemeMenu),
            "settings_menu" | "settings" => Ok(Command::SettingsMenu),
            "help" => Ok(Command::Help),
            "reload_config" => Ok(Command::ReloadConfig),
            "detach" => Ok(Command::Detach),
            "kill_session" | "quit" | "q" | "qa" | "q!" | "qa!" => Ok(Command::KillSession),
            "close" => Ok(Command::ClosePane),
            "split" | "sp" => Ok(Command::SplitDown),
            "vsplit" | "vs" => Ok(Command::SplitRight),
            "tabnew" => Ok(Command::NewWorkspace),
            "tabclose" | "tabc" => Ok(Command::CloseWorkspace(parse_workspace_list(parts)?)),
            "tabnext" | "tabn" => {
                // "tabnext 3" / "tabn 3" / "buffer 3" / "b 3" jump straight to that
                // workspace (alias for workspace_goto); no arg means next
                let rest: Vec<usize> = parts
                    .map(|tok| {
                        tok.parse::<usize>()
                            .map_err(|_| format!("invalid workspace number: {tok}"))
                    })
                    .collect::<Result<Vec<usize>, String>>()?;
                if rest.is_empty() {
                    Ok(Command::WorkspaceNext)
                } else if rest.contains(&0) {
                    Err("workspace numbers start at 1".to_string())
                } else {
                    Ok(Command::WorkspaceGoto(rest))
                }
            }
            "buffer" | "b" => {
                let rest: Vec<usize> = parts
                    .map(|tok| {
                        tok.parse::<usize>()
                            .map_err(|_| format!("invalid workspace number: {tok}"))
                    })
                    .collect::<Result<Vec<usize>, String>>()?;
                if rest.is_empty() {
                    return Err("buffer requires a workspace number".to_string());
                }
                if rest.contains(&0) {
                    return Err("workspace numbers start at 1".to_string());
                }
                Ok(Command::WorkspaceGoto(rest))
            }
            "tabprev" | "tabp" => Ok(Command::WorkspacePrev),
            "only" => Ok(Command::ToggleZoom),
            "w" | "write" => Ok(Command::PasteBuffer),
            "show_welcome" => Ok(Command::ShowWelcome),
            "cat" => Ok(Command::Cat),
            other => Err(format!("unknown command \"{other}\"")),
        }
    }
}
