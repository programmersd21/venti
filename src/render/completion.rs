//! command metadata for the vim command line and auto-suggestion popups.

/// one-line description for a known command name.
pub fn command_doc(name: &str) -> Option<&'static str> {
    Some(match name {
        "new_pane" => "open a new pane",
        "split_right" => "split the focused pane to the right",
        "split_down" => "split the focused pane downward",
        "close_pane" => "close the focused pane",
        "focus_left" | "focus_down" | "focus_up" | "focus_right" | "focus_next" | "focus_prev" => {
            "move focus between panes"
        }
        "swap_left" | "swap_down" | "swap_up" | "swap_right" => "swap panes with a neighbor",
        "resize_left" | "resize_down" | "resize_up" | "resize_right" => "resize the focused pane",
        "resize_mode" => "enter resize mode (hjkl, arrows)",
        "equalize" => "equalize all splits",
        "rotate_split" => "rotate split direction",
        "toggle_zoom" => "toggle monocle (zoom) for the focused pane",
        "toggle_sidebar" => "collapse/expand the vertical sidebar",
        "toggle_float" => "toggle floating mode for the focused pane",
        "float_center" => "center the floating pane",
        "workspace_goto" => "switch to workspace(s), e.g. `workspace_goto 2`",
        "workspace_next" => "switch to the next workspace",
        "workspace_prev" => "switch to the previous workspace",
        "new_workspace" => "create a new workspace",
        "close_workspace" => "close current or given workspaces",
        "move_to_workspace" => "move pane to given workspace(s)",
        "move_to_workspace_next" | "move_to_workspace_prev" => {
            "move the focused pane to the next / previous workspace"
        }
        "rename_workspace" => "rename the current workspace",
        "toggle_scratch" => "toggle the scratchpad pane",
        "copy_mode" => "enter copy mode (hjkl move, v select, y yank)",
        "search" => "search the pane (scrollback included)",
        "paste_buffer" => "paste the clipboard into the focused pane",
        "command_palette" => "open the command palette",
        "theme_menu" => "open the theme picker",
        "settings_menu" => "open the settings menu",
        "help" => "toggle this help screen",
        "reload_config" => "reload the configuration file",
        "detach" => "detach the client from the session",
        "kill_session" => "kill the session and stop the server",
        "show_welcome" => "show the welcome screen",
        "cat" => "a tiny ascii cat visits your pane",
        _ => return None,
    })
}

/// ghost text for the top-ranked match: the part of the command name after
/// the typed prefix, if the match starts with the query (case-insensitive).
/// fuzzy matches that do not extend the typed text produce no ghost, since
/// accepting them replaces the whole token rather than completing it.
pub fn ghost_remainder(query: &str) -> Option<String> {
    if query.is_empty() {
        return None;
    }
    let name_part: String = query.chars().take_while(|c| !c.is_whitespace()).collect();
    let arg_part = query[name_part.len()..].to_string();
    if name_part.is_empty() {
        return None;
    }
    let matches = crate::render::filtered_commands(query);
    let top = matches.first()?;
    let q = name_part.to_lowercase();
    let t = top.to_lowercase();
    if t.starts_with(&q) {
        top.char_indices()
            .nth(name_part.chars().count())
            .map(|(byte_pos, _)| format!("{}{}", &top[byte_pos..], arg_part))
    } else {
        None
    }
}
