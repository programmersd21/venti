//! rendering for the vim-style `:` command prompt and its suggestions.
//!
//! the prompt replaces the hint bar while active; the completion popup floats
//! just above it, VSCode-style, listing fuzzy matches with one-line docs.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::theme::Theme;
use crate::render::completion::{command_doc, ghost_remainder};
use crate::render::filtered_commands;
use crate::render::style::{bar_badge, fill, modal_footer, modal_frame, separator_style};

/// maximum number of completion rows shown in the popup.
const MAX_ROWS: usize = 8;

/// renders the `:` prompt line (used in place of the hint bar in VimCmd mode).
/// shows the badge, the typed query, and a dimmed ghost of the best
/// completion; accepts `clock` for the right-aligned time. falls back to the
/// bottom row of `area` when the hint bar is disabled.
pub fn render_vim_cmd_bar(
    area: Rect,
    hint_rect: Option<Rect>,
    buf: &mut Buffer,
    query: &str,
    theme: &Theme,
    clock: bool,
    clock_format: &str,
) -> (u16, u16) {
    let rect = hint_rect.unwrap_or(Rect::new(
        area.x,
        area.bottom().saturating_sub(1),
        area.width,
        1,
    ));
    if rect.width == 0 || rect.height == 0 {
        return (rect.x, rect.y);
    }

    fill(
        buf,
        rect,
        Style::default().bg(Color::Reset).fg(theme.muted_color()),
    );

    // mode badge: accent pill matching the other modes
    let mut cur_x = bar_badge(buf, rect.x + 1, rect.y, ":", theme.accent_color(), theme) + 1;
    buf[(cur_x, rect.y)]
        .set_char('│')
        .set_style(separator_style(theme));
    cur_x += 2;

    // query text
    let text_style = Style::default().bg(Color::Reset).fg(theme.text_color());
    for ch in query.chars() {
        if cur_x < rect.x + rect.width {
            buf[(cur_x, rect.y)].set_char(ch).set_style(text_style);
            cur_x += 1;
        }
    }

    if query.is_empty() {
        // placeholder while empty
        for ch in "type a command…".chars() {
            if cur_x < rect.x + rect.width.saturating_sub(12) {
                buf[(cur_x, rect.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(theme.muted_color())
                        .bg(Color::Reset)
                        .add_modifier(Modifier::DIM),
                );
                cur_x += 1;
            }
        }
    } else if let Some(ghost) = ghost_remainder(query.trim_start_matches(':')) {
        // ghost suggestion - faint continuation of the best completion
        for ch in ghost.chars() {
            if cur_x < rect.x + rect.width.saturating_sub(12) {
                buf[(cur_x, rect.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(theme.muted_color())
                        .bg(Color::Reset)
                        .add_modifier(Modifier::DIM),
                );
                cur_x += 1;
            }
        }
    }

    // right-aligned clock
    if clock {
        let time_disp = format!("{} ", crate::render::clock_text(clock_format));
        let time_len = time_disp.chars().count() as u16;
        let start = rect
            .x
            .saturating_add(rect.width.saturating_sub(time_len + 1));
        for (i, ch) in time_disp.chars().enumerate() {
            let x = start + i as u16;
            if x < rect.x + rect.width {
                buf[(x, rect.y)]
                    .set_char(ch)
                    .set_style(Style::default().fg(theme.muted_color()).bg(Color::Reset));
            }
        }
    }

    let cursor_col = rect.x + 1 + 4 + 2 + query.chars().count() as u16;
    (
        cursor_col.min(rect.x + rect.width.saturating_sub(1)),
        rect.y,
    )
}

/// renders the fuzzy completion popup above the prompt bar. does nothing when
/// the query is empty; shows a muted note when nothing matches.
pub fn render_vim_cmd_popup(
    hint_rect: Rect,
    buf: &mut Buffer,
    query: &str,
    selected: usize,
    theme: &Theme,
    ascii: bool,
) {
    let q = query.trim_start_matches(':').trim();
    if q.is_empty() {
        return;
    }
    let matches = filtered_commands(q);

    let width = 72u16.min(hint_rect.width.saturating_sub(4));
    if width < 40 {
        return;
    }
    let rows = matches.len().clamp(1, MAX_ROWS) as u16;
    let height = rows + 2; // rounded frame; rows fill the interior
    if hint_rect.y <= height {
        return; // Not enough space above hint bar
    }
    // center horizontally; sit directly above the prompt bar
    let start_x = hint_rect.x + (hint_rect.width.saturating_sub(width)) / 2;
    let start_y = hint_rect.y.saturating_sub(height);

    let modal_rect = Rect::new(start_x, start_y, width, height);
    let inner = modal_frame(buf, modal_rect, theme, ascii, Some("commands"));

    let name_col: u16 = 28;
    let doc_x = inner.x + 2 + name_col + 1;
    let doc_width = (inner.x + inner.width).saturating_sub(doc_x);

    if matches.is_empty() {
        buf.set_stringn(
            inner.x + 2,
            inner.y,
            "no matching command",
            (inner.width.saturating_sub(4)) as usize,
            Style::default().fg(theme.muted_color()).bg(Color::Reset),
        );
        modal_footer(buf, modal_rect, theme, "esc close");
        return;
    }

    let name_style = Style::default().fg(theme.text_color()).bg(Color::Reset);
    let sel_name_style = Style::default()
        .fg(theme.accent_color())
        .bg(theme.surface_color())
        .add_modifier(Modifier::BOLD);
    let sel_doc_style = Style::default()
        .fg(theme.subtext_color())
        .bg(theme.surface_color());
    let doc_style = Style::default().fg(theme.muted_color()).bg(Color::Reset);

    let max_display = rows as usize;
    let selected = selected.min(matches.len().saturating_sub(1));
    // keep the selection visible
    let offset = selected
        .saturating_sub(max_display.saturating_sub(1))
        .min(matches.len().saturating_sub(max_display));

    for (row, cmd) in matches.iter().skip(offset).take(max_display).enumerate() {
        let idx = offset + row;
        let y = inner.y + row as u16;
        let is_selected = idx == selected;

        if is_selected {
            for x in inner.x..inner.x + inner.width {
                buf[(x, y)].set_char(' ').set_style(
                    Style::default()
                        .bg(theme.surface_color())
                        .fg(theme.text_color()),
                );
            }
        }

        let (prefix, name_style, doc_style) = if is_selected {
            ("› ", sel_name_style, sel_doc_style)
        } else {
            ("  ", name_style, doc_style)
        };

        buf.set_stringn(inner.x, y, prefix, 2, name_style);
        buf.set_stringn(inner.x + 2, y, *cmd, name_col as usize, name_style);
        if let Some(doc) = command_doc(cmd) {
            buf.set_stringn(doc_x, y, doc, doc_width as usize, doc_style);
        }
    }

    modal_footer(buf, modal_rect, theme, "tab complete · enter run");
}
