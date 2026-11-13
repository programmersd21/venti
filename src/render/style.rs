//! shared building blocks so every screen speaks the same visual language:
//! a quiet surface, one accent, and consistent modal chrome.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::theme::Theme;
use crate::render::glyphs::Glyphs;

/// fills `area` with a single style.
pub fn fill(buf: &mut Buffer, area: Rect, style: Style) {
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            buf[(x, y)].set_char(' ').set_style(style);
        }
    }
}

/// draws a modal frame: accent-colored rounded border, optional centered title
/// in the top border. returns the interior content rect.
///
/// the fill is transparent (Color::Reset) so bright chrome shows through,
/// while pane content behind the dialog is dimmed by scrim. the dialog reads
/// as a distinct surface because of its accent border and badge.
pub fn modal_frame(
    buf: &mut Buffer,
    rect: Rect,
    theme: &Theme,
    ascii: bool,
    title: Option<&str>,
) -> Rect {
    let glyphs = Glyphs::styled(ascii, crate::config::schema::BorderStyle::Rounded);
    let border = Style::default().fg(theme.accent_color()).bg(Color::Reset);
    let fill_style = Style::default().fg(theme.text_color()).bg(Color::Reset);
    fill(buf, rect, fill_style);

    for x in rect.x..rect.x + rect.width {
        buf[(x, rect.y)].set_char(glyphs.h).set_style(border);
        buf[(x, rect.y + rect.height - 1)]
            .set_char(glyphs.h)
            .set_style(border);
    }
    for y in rect.y + 1..rect.y + rect.height.saturating_sub(1) {
        buf[(rect.x, y)].set_char(glyphs.v).set_style(border);
        buf[(rect.x + rect.width - 1, y)]
            .set_char(glyphs.v)
            .set_style(border);
    }
    buf[(rect.x, rect.y)]
        .set_char(glyphs.top_left)
        .set_style(border);
    buf[(rect.x + rect.width - 1, rect.y)]
        .set_char(glyphs.top_right)
        .set_style(border);
    buf[(rect.x, rect.y + rect.height - 1)]
        .set_char(glyphs.bottom_left)
        .set_style(border);
    buf[(rect.x + rect.width - 1, rect.y + rect.height - 1)]
        .set_char(glyphs.bottom_right)
        .set_style(border);

    if let Some(title) = title {
        let label = format!(" {title} ");
        let title_width = label.chars().count() as u16;
        if title_width + 2 < rect.width {
            let title_x = rect.x + (rect.width.saturating_sub(title_width)) / 2;
            let title_style = Style::default()
                .fg(theme.accent_color())
                .bg(Color::Reset)
                .add_modifier(Modifier::BOLD);
            buf.set_stringn(title_x, rect.y, label, title_width as usize, title_style);
        }
    }

    Rect::new(
        rect.x + 1,
        rect.y + 1,
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
    )
}

/// renders a centered shortcut hint line on the modal's bottom border
/// (e.g. "enter apply · esc close").
pub fn modal_footer(buf: &mut Buffer, rect: Rect, theme: &Theme, hints: &str) {
    let label = format!(" {hints} ");
    let width = label.chars().count() as u16;
    if width >= rect.width {
        return;
    }
    let x = rect.x + (rect.width.saturating_sub(width)) / 2;
    let style = Style::default().fg(theme.muted_color()).bg(Color::Reset);
    buf.set_stringn(x, rect.y + rect.height - 1, label, width as usize, style);
}

/// thin vertical separator used inside bars (overlay-colored).
pub fn separator_style(theme: &Theme) -> Style {
    Style::default().fg(theme.overlay_color()).bg(Color::Reset)
}

/// filled mode badge for bars and prompt rows (e.g. ` normal `).
/// returns the x position just past the badge.
pub fn bar_badge(buf: &mut Buffer, x: u16, y: u16, text: &str, fg: Color, theme: &Theme) -> u16 {
    let label = format!(" {text} ");
    let style = Style::default()
        .fg(fg)
        .bg(theme.surface_color())
        .add_modifier(Modifier::BOLD);
    buf.set_stringn(x, y, &label, label.chars().count(), style);
    x + label.chars().count() as u16
}
