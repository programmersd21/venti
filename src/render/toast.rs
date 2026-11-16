use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::config::theme::Theme;

/// renders a toast message: single-line surface pill, bottom-right, one
/// accent tick on the leading edge.
pub fn render_toast(area: Rect, buf: &mut Buffer, message: &str, theme: &Theme) {
    let msg_len = message.chars().count() as u16;
    let width = (msg_len + 5).min(area.width);
    let height = 1u16;

    if width < 6 || height > area.height {
        return;
    }

    let toast_x = area.x + area.width.saturating_sub(width + 2);
    let toast_y = area.y + area.height.saturating_sub(2);

    let pill_style = Style::default()
        .fg(theme.text_color())
        .bg(theme.surface_color());
    for x in toast_x..toast_x + width {
        buf[(x, toast_y)].set_char(' ').set_style(pill_style);
    }

    // leading accent tick
    buf[(toast_x, toast_y)].set_char('▎').set_style(
        Style::default()
            .fg(theme.accent_color())
            .bg(theme.surface_color()),
    );

    let max_msg = width.saturating_sub(4) as usize;
    for (i, ch) in message.chars().take(max_msg).enumerate() {
        buf[(toast_x + 2 + i as u16, toast_y)]
            .set_char(ch)
            .set_style(
                Style::default()
                    .fg(theme.text_color())
                    .bg(theme.surface_color()),
            );
    }
}
