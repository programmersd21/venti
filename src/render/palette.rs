use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::theme::Theme;
use crate::config::validate::KNOWN_COMMANDS;
use crate::render::completion::command_doc;
use crate::render::style::{modal_footer, modal_frame};
use crate::util::fuzzy::fuzzy_match;

/// state for the command palette overlay.
#[derive(Debug, Clone, Default)]
pub struct PaletteState {
    pub query: String,
    pub selected_idx: usize,
}

/// palette modal width in cells (before edge clamping).
pub const PALETTE_WIDTH: u16 = 90;
/// palette modal height in cells.
pub const PALETTE_HEIGHT: u16 = 24;
/// column at which the command description starts.
const DOC_COLUMN: u16 = 30;

/// returns the command names matching the given fuzzy query, best first.
/// matching runs on the command name portion only (arguments after the first
/// space are ignored for ranking. pressing enter with typed args executes
/// the typed text directly).
pub fn filtered_commands(query: &str) -> Vec<&'static str> {
    let name_part: String = query.chars().take_while(|c| !c.is_whitespace()).collect();

    if name_part.is_empty() {
        return KNOWN_COMMANDS.to_vec();
    }

    let mut matches: Vec<(&str, i64)> = KNOWN_COMMANDS
        .iter()
        .filter_map(|&cmd| {
            fuzzy_match(&name_part, cmd).map(|score| {
                // prefix matches rank above fuzzy matches so "spl" completes to
                // split_* instead of similarly-fuzzy but surprising names.
                let prefix = cmd.to_lowercase().starts_with(&name_part.to_lowercase());
                (cmd, if prefix { score + 100 } else { score })
            })
        })
        .collect();

    matches.sort_by_key(|a| std::cmp::Reverse(a.1));
    matches.into_iter().map(|(cmd, _)| cmd).collect()
}

/// renders the modal command palette - keyboard-first command launcher with
/// fuzzy search, one-line docs, and a shortcut footer.
pub fn render_palette(
    area: Rect,
    buf: &mut Buffer,
    state: &PaletteState,
    theme: &Theme,
    ascii: bool,
) {
    let width = PALETTE_WIDTH.min(area.width.saturating_sub(4));
    let height = PALETTE_HEIGHT.min(area.height.saturating_sub(4));

    if width < 34 || height < 8 {
        return;
    }

    let start_x = area.x + (area.width.saturating_sub(width)) / 2;
    let start_y = area.y + 2.min(area.height.saturating_sub(height));

    let modal_rect = Rect::new(start_x, start_y, width, height);
    let inner = modal_frame(buf, modal_rect, theme, ascii, Some("commands"));

    // prompt line: accent chevron + query + placeholder or cursor block
    let prompt_style = Style::default().fg(theme.accent_color()).bg(Color::Reset);
    let text_style = Style::default().fg(theme.text_color()).bg(Color::Reset);
    let placeholder_style = Style::default().fg(theme.muted_color()).bg(Color::Reset);

    buf[(inner.x + 1, inner.y)]
        .set_char('›')
        .set_style(prompt_style.add_modifier(Modifier::BOLD));
    let query_len = state.query.chars().count() as u16;
    buf.set_stringn(
        inner.x + 3,
        inner.y,
        &state.query,
        (inner.width.saturating_sub(4)) as usize,
        text_style,
    );
    if state.query.is_empty() {
        buf.set_stringn(
            inner.x + 3,
            inner.y,
            "type a command…",
            (inner.width.saturating_sub(4)) as usize,
            placeholder_style,
        );
    } else {
        // cursor block at end of query
        let cursor_x = inner.x + 3 + query_len;
        if cursor_x < inner.x + inner.width {
            buf[(cursor_x, inner.y)]
                .set_char('▏')
                .set_style(prompt_style);
        }
    }

    // thin divider under the prompt
    let divider_style = Style::default().fg(theme.overlay_color()).bg(Color::Reset);
    for x in inner.x..inner.x + inner.width {
        buf[(x, inner.y + 1)].set_char('─').set_style(divider_style);
    }

    let matches = filtered_commands(&state.query);

    // rows area between divider and footer row
    let first_row = inner.y + 2;
    let last_row = inner.y + inner.height.saturating_sub(1);
    let max_display = (last_row.saturating_sub(first_row) + 1) as usize;

    if matches.is_empty() {
        // empty state: quiet, centered, explains what to do next
        let msg = "no matching commands";
        let msg_w = msg.chars().count() as u16;
        let msg_x = inner.x + inner.width.saturating_sub(msg_w) / 2;
        buf.set_stringn(
            msg_x,
            first_row + (last_row - first_row) / 2,
            msg,
            msg_w as usize,
            Style::default().fg(theme.muted_color()).bg(Color::Reset),
        );
        modal_footer(buf, modal_rect, theme, "esc close");
        return;
    }

    let selected = state.selected_idx.min(matches.len().saturating_sub(1));
    let offset = selected
        .saturating_sub(max_display.saturating_sub(1))
        .min(matches.len().saturating_sub(max_display));

    let show_doc = inner.width > DOC_COLUMN + 14;
    let cmd_width = (DOC_COLUMN - 4) as usize;

    for (row, cmd) in matches.iter().skip(offset).take(max_display).enumerate() {
        let idx = offset + row;
        let y = first_row + row as u16;
        let is_selected = idx == selected;

        let (marker, name_style, doc_style) = if is_selected {
            (
                "› ",
                Style::default()
                    .fg(theme.accent_color())
                    .bg(theme.surface_color())
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(theme.subtext_color())
                    .bg(theme.surface_color()),
            )
        } else {
            (
                "  ",
                Style::default().fg(theme.text_color()).bg(Color::Reset),
                Style::default().fg(theme.muted_color()).bg(Color::Reset),
            )
        };

        if is_selected {
            // selected row: full-width surface wash + accent marker
            for x in inner.x..inner.x + inner.width {
                buf[(x, y)].set_char(' ').set_style(
                    Style::default()
                        .bg(theme.surface_color())
                        .fg(theme.text_color()),
                );
            }
        }

        buf.set_stringn(inner.x, y, marker, 2, name_style);
        buf.set_stringn(inner.x + 2, y, *cmd, cmd_width, name_style);
        if show_doc && let Some(doc) = command_doc(cmd) {
            buf.set_stringn(
                inner.x + DOC_COLUMN,
                y,
                doc,
                (inner.width.saturating_sub(DOC_COLUMN + 1)) as usize,
                doc_style,
            );
        }
    }

    // scroll indicators
    let indicator_style = Style::default().fg(theme.muted_color()).bg(Color::Reset);
    if offset > 0 {
        buf[(inner.x + inner.width - 1, first_row)]
            .set_char('↑')
            .set_style(indicator_style);
    }
    if offset + max_display < matches.len() {
        buf[(inner.x + inner.width - 1, last_row)]
            .set_char('↓')
            .set_style(indicator_style);
    }

    let count = matches.len();
    let footer = format!(
        "↑↓ move · enter run · esc close · {count} match{}",
        if count == 1 { "" } else { "es" }
    );
    modal_footer(buf, modal_rect, theme, &footer);
}
