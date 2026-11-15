use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::theme::Theme;
use crate::render::style::{modal_footer, modal_frame};

const ROWS: [(&str, &str, &str, &str); 6] = [
    ("f3", "split pane", "f4", "close pane"),
    ("f5", "toggle zoom", "f11", "command palette"),
    ("alt+n", "new workspace", "alt+1", "switch to ws 1"),
    ("alt+w", "close workspace", "f2", "settings"),
    ("f1", "help", "f12", "theme picker"),
    ("f9", "copy mode", "f10", "resize mode"),
];

const KEY_WIDTH: usize = 10;
const ACTION_WIDTH: usize = 16;
const LEFT_KEY_OFFSET: u16 = 2;
const RIGHT_KEY_OFFSET: u16 = 28;

/// renders the welcome screen.
pub fn render_welcome(area: Rect, buf: &mut Buffer, theme: &Theme, ascii: bool) {
    if area.height < 18 || area.width < 60 {
        return render_welcome_compact(area, buf, theme, ascii);
    }

    let width = 56u16;
    let height = 17u16;
    let start_x = area.x + (area.width.saturating_sub(width)) / 2;
    let start_y = area.y + (area.height.saturating_sub(height)) / 2;

    let modal_rect = Rect::new(start_x, start_y, width, height);
    let inner = modal_frame(buf, modal_rect, theme, ascii, None);

    let mut row = inner.y + 1;
    let label = |buf: &mut Buffer, text: &str, y: u16, style: Style| {
        let x = inner.x + (inner.width.saturating_sub(text.chars().count() as u16)) / 2;
        buf.set_stringn(x, y, text, text.chars().count(), style);
    };

    label(
        buf,
        "venti",
        row,
        Style::default()
            .fg(theme.accent_color())
            .bg(Color::Reset)
            .add_modifier(Modifier::BOLD),
    );
    row += 1;
    label(
        buf,
        "a fast, quiet terminal multiplexer",
        row,
        Style::default().fg(theme.muted_color()).bg(Color::Reset),
    );
    row += 2;

    let left = inner.x + LEFT_KEY_OFFSET;
    let right = inner.x + RIGHT_KEY_OFFSET;
    let text = Style::default().fg(theme.text_color()).bg(Color::Reset);
    let accent = Style::default()
        .fg(theme.accent_color())
        .bg(Color::Reset)
        .add_modifier(Modifier::BOLD);

    for (a_key, a_action, b_key, b_action) in ROWS {
        buf.set_stringn(left, row, a_key, KEY_WIDTH, accent);
        buf.set_stringn(left + KEY_WIDTH as u16, row, a_action, ACTION_WIDTH, text);
        buf.set_stringn(right, row, b_key, KEY_WIDTH, accent);
        buf.set_stringn(right + KEY_WIDTH as u16, row, b_action, ACTION_WIDTH, text);
        row += 1;
    }

    row += 1;
    label(buf, "alt+arrows", row, accent);
    row += 1;
    label(buf, "focus pane", row, text);

    modal_footer(buf, modal_rect, theme, "press any key · f1 for help");
}

/// renders a compact welcome screen for smaller terminals.
fn render_welcome_compact(area: Rect, buf: &mut Buffer, theme: &Theme, ascii: bool) {
    let width = 44u16.min(area.width.saturating_sub(4));
    let height = 12u16.min(area.height.saturating_sub(2));

    if width < 32 || height < 10 {
        return;
    }

    let start_x = area.x + (area.width.saturating_sub(width)) / 2;
    let start_y = area.y + (area.height.saturating_sub(height)) / 2;

    let modal_rect = Rect::new(start_x, start_y, width, height);
    let inner = modal_frame(buf, modal_rect, theme, ascii, None);

    let text = Style::default().fg(theme.text_color()).bg(Color::Reset);
    let accent = Style::default()
        .fg(theme.accent_color())
        .bg(Color::Reset)
        .add_modifier(Modifier::BOLD);

    let mut row = inner.y + 1;
    let center = |buf: &mut Buffer, line: &str, row: u16, style: Style| {
        let x = inner.x + (inner.width.saturating_sub(line.chars().count() as u16)) / 2;
        buf.set_stringn(x, row, line, line.chars().count(), style);
    };

    center(buf, "venti", row, accent);
    row += 2;

    for (key, action) in [
        ("f1", "help"),
        ("f2", "settings"),
        ("f3", "split pane"),
        ("f4", "close pane"),
        ("alt+space", "palette"),
    ] {
        buf.set_stringn(inner.x + 3, row, key, 12, accent);
        buf.set_stringn(
            inner.x + 15,
            row,
            action,
            inner.width.saturating_sub(15) as usize,
            text,
        );
        row += 1;
    }

    modal_footer(buf, modal_rect, theme, "press any key");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_full_welcome_row_fits_its_slot() {
        for (a_key, a_action, b_key, b_action) in ROWS {
            assert!(a_key.chars().count() <= KEY_WIDTH, "{a_key}");
            assert!(a_action.chars().count() <= ACTION_WIDTH, "{a_action}");
            assert!(b_key.chars().count() <= KEY_WIDTH, "{b_key}");
            assert!(b_action.chars().count() <= ACTION_WIDTH, "{b_action}");
            assert!(
                (LEFT_KEY_OFFSET + (2 * KEY_WIDTH as u16) + ACTION_WIDTH as u16) as usize <= 54
            );
            assert!(RIGHT_KEY_OFFSET + KEY_WIDTH as u16 + ACTION_WIDTH as u16 <= 54);
        }
    }
}
