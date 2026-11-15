use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

pub use crate::config::theme::BUILTIN_THEMES;
use crate::config::theme::Theme;
use crate::render::style::{modal_footer, modal_frame};

/// state for the theme picker modal.
#[derive(Debug, Clone, Default)]
pub struct ThemeMenuState {
    pub selected_idx: usize,
}

/// geometry of the theme modal: outer rect and inner content rect.
/// one tall single-column list with live-palette swatches. the whole
/// built-in set fits, so nothing scrolls.
pub const THEME_MENU_WIDTH: u16 = 46;

pub fn theme_menu_geometry(area: Rect) -> (Rect, Rect) {
    let width = THEME_MENU_WIDTH.min(area.width.saturating_sub(2));
    let height = (BUILTIN_THEMES.len() as u16 + 2).min(area.height.saturating_sub(2));
    let start_x = area.x + (area.width.saturating_sub(width)) / 2;
    let start_y = area.y + (area.height.saturating_sub(height)) / 2;
    let modal_rect = Rect::new(start_x, start_y, width, height);
    let inner = Rect::new(
        modal_rect.x + 1,
        modal_rect.y + 1,
        modal_rect.width.saturating_sub(2),
        modal_rect.height.saturating_sub(2),
    );
    (modal_rect, inner)
}

/// maps a row inside the list to its theme index.
pub fn theme_menu_index_at(inner: Rect, col: u16, row: u16) -> Option<usize> {
    if row >= inner.height || col >= inner.width {
        return None;
    }
    let idx = row as usize;
    (idx < BUILTIN_THEMES.len()).then_some(idx)
}

/// renders the theme picker: a quiet single-column list, each row showing a
/// marker, the theme name, and six live palette dots from that theme.
pub fn render_theme_menu(
    area: Rect,
    buf: &mut Buffer,
    state: &ThemeMenuState,
    current_theme: &Theme,
    ascii: bool,
) {
    let (modal_rect, inner) = theme_menu_geometry(area);
    if inner.width < 40 || inner.height < BUILTIN_THEMES.len() as u16 {
        return;
    }

    let selected = state
        .selected_idx
        .min(BUILTIN_THEMES.len().saturating_sub(1));

    let inner = modal_frame(buf, modal_rect, current_theme, ascii, Some("theme"));

    for (idx, name) in BUILTIN_THEMES.iter().enumerate() {
        let y = inner.y + idx as u16;
        let is_selected = idx == selected;
        let is_active = *name == current_theme.name;

        let row_bg = if is_selected {
            current_theme.surface_color()
        } else {
            Color::Reset
        };

        if is_selected {
            for x in inner.x..inner.x + inner.width {
                buf[(x, y)]
                    .set_char(' ')
                    .set_style(Style::default().bg(row_bg));
            }
        }

        let (marker, marker_fg, name_fg) = if is_selected {
            (
                "›",
                current_theme.accent_color(),
                current_theme.text_color(),
            )
        } else if is_active {
            (
                "•",
                current_theme.success_color(),
                current_theme.success_color(),
            )
        } else {
            (
                "·",
                current_theme.muted_color(),
                current_theme.subtext_color(),
            )
        };

        let marker_style = Style::default()
            .fg(marker_fg)
            .bg(row_bg)
            .add_modifier(Modifier::BOLD);
        buf.set_stringn(inner.x + 1, y, marker, 1, marker_style);

        let name_style =
            Style::default()
                .fg(name_fg)
                .bg(row_bg)
                .add_modifier(if is_selected || is_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                });
        buf.set_stringn(
            inner.x + 3,
            y,
            name,
            (inner.width - 3 - 13) as usize,
            name_style,
        );

        // live palette dots: this theme's own colors, so the list is a gallery
        if let Some(sample) = Theme::builtin(name) {
            let dots = [
                sample.accent,
                sample.info,
                sample.success,
                sample.warn,
                sample.alert,
                sample.text,
            ];
            let dot_x = inner.x + inner.width - 12;
            for (i, c) in dots.iter().enumerate() {
                buf[(dot_x + 2 * i as u16, y)]
                    .set_char('●')
                    .set_style(Style::default().fg(c.to_ratatui()).bg(row_bg));
            }
        }
    }

    modal_footer(
        buf,
        modal_rect,
        current_theme,
        "↑↓ preview · enter apply · esc close",
    );
}
