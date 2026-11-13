use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::schema::ProfileMode;
use crate::config::theme::Theme;
use crate::input::{Command, InputMode, Key, KeyBindings, KeyCode};
use crate::render::style::{modal_footer, modal_frame};

/// hard bound for the per-profile binding table (modal scroll range).
pub const HELP_ITEMS: usize = 96;

/// interior content rows of the help modal for the given render area.
fn help_content_rows(area: Rect) -> usize {
    let height = 37u16.min(area.height.saturating_sub(2));
    (height as usize).saturating_sub(2)
}

/// section rank for ordering rows. f keys first, then panes, then
/// workspaces, then extras.
fn section_of(cmd: &Command, key: &Key) -> u8 {
    if matches!(key.code, KeyCode::F(_)) {
        return 0;
    }
    match cmd {
        Command::NewPane
        | Command::SplitRight
        | Command::SplitDown
        | Command::ClosePane
        | Command::FocusLeft
        | Command::FocusDown
        | Command::FocusUp
        | Command::FocusRight
        | Command::FocusNext
        | Command::FocusPrev
        | Command::SwapLeft
        | Command::SwapDown
        | Command::SwapUp
        | Command::SwapRight
        | Command::ResizeLeft
        | Command::ResizeDown
        | Command::ResizeUp
        | Command::ResizeRight
        | Command::ResizeMode
        | Command::Equalize
        | Command::RotateSplit
        | Command::ToggleZoom
        | Command::ToggleFloat
        | Command::FloatCenter
        | Command::ToggleScratch => 1,
        Command::WorkspaceGoto(_)
        | Command::WorkspaceNext
        | Command::WorkspacePrev
        | Command::NewWorkspace
        | Command::CloseWorkspace(_)
        | Command::MoveToWorkspace(_)
        | Command::MoveToWorkspaceNext
        | Command::MoveToWorkspacePrev
        | Command::RenameWorkspace(_)
        | Command::ToggleSidebar => 2,
        _ => 3,
    }
}

fn section_header(rank: u8) -> &'static str {
    match rank {
        0 => "f keys (always work)",
        1 => "panes",
        2 => "workspaces",
        _ => "extras",
    }
}

/// renders a command into a short verb phrase. parameterized commands
/// (workspace numbers, renames) render their common form.
fn action_label(cmd: &Command) -> String {
    match cmd {
        Command::NewPane => "new pane".into(),
        Command::SplitRight => "split right".into(),
        Command::SplitDown => "split down".into(),
        Command::ClosePane => "close pane".into(),
        Command::FocusLeft => "focus left".into(),
        Command::FocusDown => "focus down".into(),
        Command::FocusUp => "focus up".into(),
        Command::FocusRight => "focus right".into(),
        Command::FocusNext => "next pane".into(),
        Command::FocusPrev => "prev pane".into(),
        Command::SwapLeft => "swap left".into(),
        Command::SwapDown => "swap down".into(),
        Command::SwapUp => "swap up".into(),
        Command::SwapRight => "swap right".into(),
        Command::ResizeLeft | Command::ResizeDown | Command::ResizeUp | Command::ResizeRight => {
            "resize pane".into()
        }
        Command::ResizeMode => "resize mode".into(),
        Command::Equalize => "equalize panes".into(),
        Command::RotateSplit => "rotate split".into(),
        Command::ToggleZoom => "toggle zoom".into(),
        Command::ToggleSidebar => "toggle sidebar".into(),
        Command::ToggleFloat => "toggle float".into(),
        Command::FloatCenter => "center float".into(),
        Command::WorkspaceGoto(ns) => match ns.as_slice() {
            [n] => format!("workspace {n}"),
            _ => "goto workspace".into(),
        },
        Command::WorkspaceNext => "next workspace".into(),
        Command::WorkspacePrev => "prev workspace".into(),
        Command::NewWorkspace => "new workspace".into(),
        Command::CloseWorkspace(_) => "close workspace".into(),
        Command::MoveToWorkspace(ns) => match ns.as_slice() {
            [n] => format!("move pane to ws {n}"),
            _ => "move pane to workspace".into(),
        },
        Command::MoveToWorkspaceNext => "move pane to next workspace".into(),
        Command::MoveToWorkspacePrev => "move pane to prev workspace".into(),
        Command::RenameWorkspace(_) => "rename workspace".into(),
        Command::ToggleScratch => "scratchpad".into(),
        Command::CopyMode => "copy mode".into(),
        Command::Search => "search scrollback".into(),
        Command::PasteBuffer => "paste".into(),
        Command::CommandPalette => "command palette".into(),
        Command::ThemeMenu => "theme picker".into(),
        Command::SettingsMenu => "settings menu".into(),
        Command::Help => "this help".into(),
        Command::ReloadConfig => "reload config".into(),
        Command::Detach => "detach".into(),
        Command::KillSession => "kill session".into(),
        Command::ShowWelcome => "welcome".into(),
        Command::Cat => "cat".into(),
    }
}

/// display for a key using the binding's canonical spelling.
/// `alt+h`, `alt+shift+left`, `f3`, as parsed.
fn key_display(k: &Key) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if k.modifiers.ctrl {
        parts.push("ctrl");
    }
    if k.modifiers.alt {
        parts.push("alt");
    }
    if k.modifiers.shift {
        parts.push("shift");
    }
    parts.push(match &k.code {
        KeyCode::Char(c) => {
            let mut out = parts.join("+");
            if !out.is_empty() {
                out.push('+');
            }
            out.push(c.to_ascii_lowercase());
            return out;
        }
        KeyCode::Enter => "enter",
        KeyCode::Backspace => "backspace",
        KeyCode::Tab => "tab",
        KeyCode::Esc => "esc",
        KeyCode::Up => "up",
        KeyCode::Down => "down",
        KeyCode::Left => "left",
        KeyCode::Right => "right",
        KeyCode::Home => "home",
        KeyCode::End => "end",
        KeyCode::PageUp => "pageup",
        KeyCode::PageDown => "pagedown",
        KeyCode::Delete => "delete",
        KeyCode::Insert => "insert",
        KeyCode::Space => "space",
        KeyCode::F(n) => return format!("f{n}"),
    });
    parts.join("+")
}

/// builds a row set from the profile's actual binding map. the cheat-sheet
/// can never drift from the map: what you see is what is bound. the digit
/// workspaces (alt+1..9 / alt+shift+1..9 for every profile) are collapsed
/// into one row each to keep the list minimal.
fn items_for(profile: ProfileMode) -> Vec<(String, String)> {
    let bindings = KeyBindings::new(profile, &[]);
    let mut rows: Vec<(u8, String, String)> = bindings
        .entries(InputMode::Normal)
        .into_iter()
        .map(|(key, cmd)| {
            let section = section_of(cmd, key);
            (section, key_display(key), action_label(cmd))
        })
        .collect();

    // f keys first (sorted numerically), then sections sorted by key text
    rows.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| {
            match (
                a.1.strip_prefix('f').and_then(|r| r.parse::<u32>().ok()),
                b.1.strip_prefix('f').and_then(|r| r.parse::<u32>().ok()),
            ) {
                (Some(x), Some(y)) => x.cmp(&y),
                _ => a.1.cmp(&b.1),
            }
        })
    });

    let mut v: Vec<(String, String)> = Vec::with_capacity(rows.len() + 8);
    let mut last_section = u8::MAX;
    let mut i = 0;
    while i < rows.len() {
        // collapse a full alt+digit / alt+shift+digit run of workspaces 1..=9
        let collapse_len = collapse_digit_run(&rows[i..]);
        if collapse_len > 0 {
            if rows[i].0 != last_section {
                if last_section != u8::MAX {
                    v.push((String::new(), String::new()));
                }
                v.push((section_header(rows[i].0).to_string(), String::new()));
                last_section = rows[i].0;
            }
            v.push((
                collapsed_digit_key(&rows[i].1),
                collapsed_digit_action(&rows[i].1),
            ));
            i += collapse_len;
            continue;
        }
        let (section, key, desc) = &rows[i];
        if *section != last_section {
            if last_section != u8::MAX {
                v.push((String::new(), String::new()));
            }
            v.push((section_header(*section).to_string(), String::new()));
            last_section = *section;
        }
        v.push((key.clone(), desc.clone()));
        i += 1;
    }
    v
}

/// if a run of nine consecutive `alt+N` / `alt+shift+N` single-digit rows
/// for workspaces 1..=9 starts at the slice start, returns 9; else 0.
fn collapse_digit_run(rows: &[(u8, String, String)]) -> usize {
    if rows.len() < 9 || rows[0].0 != 2 {
        return 0;
    }
    let prefix = match rows[0].1.as_str() {
        s if s.starts_with("alt+shift+") && s.len() == "alt+shift+9".len() => "alt+shift+",
        s if s.starts_with("alt+") && s.len() == "alt+9".len() => "alt+",
        _ => return 0,
    };
    for n in 1..=9u8 {
        let expect = format!("{prefix}{n}");
        if rows[n as usize - 1].1 != expect || rows[n as usize - 1].0 != 2 {
            return 0;
        }
    }
    9
}

fn collapsed_digit_key(sample: &str) -> String {
    if sample.starts_with("alt+shift+") {
        "alt+shift+1..9".to_string()
    } else {
        "alt+1..9".to_string()
    }
}

fn collapsed_digit_action(sample: &str) -> String {
    if sample.starts_with("alt+shift+") {
        "move pane to workspace N".to_string()
    } else {
        "switch to workspace N".to_string()
    }
}

/// renders the profile-aware help overlay. `scroll` is a row offset into the
/// items table; the table length depends on the profile, clamp `scroll`
/// accordingly.
pub fn render_help(
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    ascii: bool,
    scroll: usize,
    profile: ProfileMode,
) {
    let width = 78u16.min(area.width.saturating_sub(4));
    let height = 37u16.min(area.height.saturating_sub(2));

    if width < 36 || height < 10 {
        return;
    }

    let start_x = area.x + (area.width.saturating_sub(width)) / 2;
    let start_y = area.y + (area.height.saturating_sub(height)) / 2;

    let modal_rect = Rect::new(start_x, start_y, width, height);
    let title = match profile {
        ProfileMode::Beginner => Some("keybindings · beginner"),
        ProfileMode::Hypr => Some("keybindings · hypr"),
        ProfileMode::Vim => Some("keybindings · vim"),
        ProfileMode::Dev => Some("keybindings · dev"),
    };
    let inner = modal_frame(buf, modal_rect, theme, ascii, title);

    let items = items_for(profile);
    debug_assert!(items.len() <= HELP_ITEMS);
    let desc_x = inner.x + (inner.width * 5 / 10).clamp(28, 54);

    let visible_rows = help_content_rows(area).max(1);
    let start_idx = scroll.min(items.len().saturating_sub(visible_rows));
    let end_idx = (start_idx + visible_rows).min(items.len());
    let visible = &items[start_idx..end_idx];

    for (idx, (key, desc)) in visible.iter().enumerate() {
        let y = inner.y + idx as u16;
        if y >= inner.y + inner.height {
            break;
        }

        if key.is_empty() && desc.is_empty() {
            continue;
        }

        if desc.is_empty() {
            // section header - accent text
            buf.set_stringn(
                inner.x + 1,
                y,
                key,
                (inner.width.saturating_sub(2)) as usize,
                Style::default()
                    .fg(theme.accent_color())
                    .bg(Color::Reset)
                    .add_modifier(Modifier::BOLD),
            );
        } else {
            // key in accent, description in bright text for readability
            buf.set_stringn(
                inner.x + 2,
                y,
                key,
                (desc_x.saturating_sub(inner.x).saturating_sub(2)) as usize,
                Style::default().fg(theme.accent_color()).bg(Color::Reset),
            );
            buf.set_stringn(
                desc_x,
                y,
                desc,
                (inner.x + inner.width).saturating_sub(desc_x) as usize,
                Style::default()
                    .fg(theme.text_color())
                    .bg(Color::Reset)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }

    // scroll position indicator on the right edge: only the thumb is drawn,
    // without a filled track, so it reads as a marker rather than shading.
    if items.len() > visible_rows {
        let max_start = items.len().saturating_sub(visible_rows).max(1);
        let track = inner.height as usize;
        let visible_frac = visible_rows as f32 / items.len() as f32;
        let thumb_len = ((track as f32 * visible_frac).ceil() as usize).max(1);
        let travel = track.saturating_sub(thumb_len);
        let thumb_start = if max_start > 1 {
            (start_idx * travel) / max_start
        } else {
            0
        };
        for i in 0..thumb_len {
            buf[(
                inner.x + inner.width - 1,
                inner.y + (thumb_start + i) as u16,
            )]
                .set_char('▐')
                .set_style(Style::default().fg(theme.muted_color()).bg(Color::Reset));
        }
    }

    modal_footer(buf, modal_rect, theme, "↑ ↓ scroll  any key close");
}
