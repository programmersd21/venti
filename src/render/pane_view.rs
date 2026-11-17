use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use vt100::Screen;

use crate::config::theme::{RgbColor, Theme};
use crate::pane::search::SearchState;
use crate::pane::selection::Selection;

/// widget rendering vt100 screen buffer directly onto a ratatui buffer.
pub struct PaneView<'a> {
    pub screen: &'a Screen,
    pub theme: &'a Theme,
    pub is_focused: bool,
    pub dim_inactive: bool,
    pub dim_strength: f64,
    pub selection: Option<&'a Selection>,
    pub search: Option<&'a SearchState>,
    pub scroll_offset: usize,
    /// number of lines in the pane's scrollback (logical-line base).
    pub scrollback_len: usize,
    /// local kitty image id → daemon-wide id, per alternate_screen. kitty
    /// unicode-placeholder cells carry the image id in their foreground
    /// color; venti transmits images under daemon-wide ids, so the cells
    /// must be rewritten to match or the host anchors placeholders to an
    /// image that was never transmitted and nothing renders.
    pub kitty_id_map: HashMap<(bool, u32), u32>,
}

impl<'a> Widget for PaneView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (rows, cols) = self.screen.size();

        // for cells outside the vt100 screen size, use Color::Reset for transparency
        let overshoot_style = Style::default().bg(Color::Reset).fg(Color::Reset);

        // fill rows beyond screen height
        for row in rows..area.height {
            for col in 0..area.width {
                let buf_cell = &mut buf[(area.x + col, area.y + row)];
                buf_cell.set_char(' ');
                buf_cell.set_style(overshoot_style);
            }
        }

        // fill columns beyond screen width
        for row in 0..rows.min(area.height) {
            for col in cols..area.width {
                let buf_cell = &mut buf[(area.x + col, area.y + row)];
                buf_cell.set_char(' ');
                buf_cell.set_style(overshoot_style);
            }
        }

        let buf_area = buf.area;

        for row in 0..area.height {
            if row >= rows {
                break;
            }
            if area.y.saturating_add(row) >= buf_area.y + buf_area.height {
                break;
            }

            for col in 0..area.width {
                if col >= cols {
                    continue;
                }
                if area.x.saturating_add(col) >= buf_area.x + buf_area.width {
                    continue;
                }

                let cell_opt = self.screen.cell(row, col);
                let Some(c) = cell_opt else {
                    // out-of-bounds within the screen rect: use overshoot style
                    let buf_cell = &mut buf[(area.x + col, area.y + row)];
                    buf_cell.set_char(' ');
                    buf_cell.set_style(overshoot_style);
                    continue;
                };

                // skip wide-character continuation cells, ratatui wrote the
                // blank placeholder when we rendered the primary cell; writing
                // a space here would clobber it and shift every char that follows.
                if c.is_wide_continuation() {
                    continue;
                }

                let contents = c.contents();
                let vt_fg = c.fgcolor();
                let vt_bg = c.bgcolor();
                let bold = c.bold();
                let italic = c.italic();
                let underline = c.underline();
                let reverse = c.inverse();
                let wide = c.is_wide();
                let is_kitty_placeholder = contents.starts_with('\u{10EEEE}');

                let mut fg_color = if is_kitty_placeholder {
                    map_vt_color_raw(vt_fg)
                } else {
                    map_vt_color(vt_fg, self.theme)
                };
                let mut bg_color = map_vt_color(vt_bg, self.theme);

                if reverse && !is_kitty_placeholder {
                    std::mem::swap(&mut fg_color, &mut bg_color);
                }

                // selection highlight
                let is_selected = self
                    .selection
                    .is_some_and(|s| s.contains(row as usize, col as usize));
                if is_selected {
                    bg_color = self.theme.surface_color();
                }

                // search highlight
                let search_state = self.search;
                let base = self.scrollback_len as isize - self.scroll_offset as isize;
                let logical = base + row as isize;
                let logical_opt = if logical >= 0 {
                    Some(logical as usize)
                } else {
                    None
                };
                let is_current_search = search_state.is_some_and(|s| {
                    logical_opt.is_some_and(|l| s.is_current_match(l, col as usize))
                });
                let is_search = search_state
                    .is_some_and(|s| logical_opt.is_some_and(|l| s.is_match(l, col as usize)));
                if is_current_search && !is_kitty_placeholder {
                    bg_color = self.theme.accent_color();
                    fg_color = self.theme.base_color();
                } else if is_search && !is_kitty_placeholder {
                    bg_color = self.theme.warn_color();
                    fg_color = self.theme.base_color();
                }

                // dim inactive panes, but never kitty unicode-placeholder cells:
                // their fg carries the image id and dimming it would break the
                // placeholder→image association (and the image would vanish).
                if !self.is_focused && self.dim_inactive && !is_kitty_placeholder {
                    fg_color = dim_color(fg_color, self.theme.muted, self.dim_strength);
                    if bg_color != Color::Reset {
                        bg_color = dim_color(bg_color, self.theme.base, self.dim_strength);
                    }
                }

                // kitty unicode-placeholder cells carry the image id in their
                // foreground color (low 24 bits, plus an optional MSB third
                // diacritic). venti transmits images under daemon-wide ids, so
                // the cells must be rewritten to match or the host anchors
                // placeholders to an image that was never transmitted.
                let (contents, fg_color) = remap_kitty_placeholder(
                    contents,
                    fg_color,
                    self.screen.alternate_screen(),
                    &self.kitty_id_map,
                );

                let vt_underline = c.underlinecolor();
                let underline_color = if is_kitty_placeholder {
                    map_vt_color_raw(vt_underline)
                } else {
                    map_vt_color(vt_underline, self.theme)
                };
                let mut style = Style::default()
                    .fg(fg_color)
                    .bg(bg_color)
                    .underline_color(underline_color);
                if bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if underline {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }

                let buf_cell = &mut buf[(area.x + col, area.y + row)];
                if contents.is_empty() {
                    // empty contents, but btop and other TUIs may set background
                    // colors on empty cells for graphs/bars. Render as space but
                    // preserve all the styling.
                    buf_cell.set_char(' ');
                } else if wide && col.saturating_add(1) >= area.width {
                    // do not emit half of a wide glyph into the adjacent pane
                    // when the terminal screen is wider than this pane area.
                    buf_cell.set_char(' ');
                } else if wide {
                    // set_symbol handles wide chars: ratatui writes the char and
                    // blanks the continuation cell automatically.
                    buf_cell.set_symbol(&contents);
                } else {
                    // use set_symbol to preserve any combining characters attached
                    // to this cell (set_char would silently drop them).
                    buf_cell.set_symbol(&contents);
                }
                buf_cell.set_style(style);
            }
        }
    }
}

/// rewrites a kitty Unicode-placeholder cell (starts with U+10EEEE) from its
/// app-local image id to venti's daemon-wide id. the id rides in the
/// foreground color (low 24 bits) with the most significant byte optionally
/// encoded as a third diacritic; the row/column diacritics are preserved
/// unchanged. returns `(symbol, fg)` with the cell untouched when it is not a
/// placeholder or the pane has no id mapping.
fn remap_kitty_placeholder<'a>(
    symbol: &'a str,
    fg: Color,
    alternate_screen: bool,
    map: &HashMap<(bool, u32), u32>,
) -> (std::borrow::Cow<'a, str>, Color) {
    if map.is_empty() || !symbol.starts_with('\u{10EEEE}') {
        return (std::borrow::Cow::Borrowed(symbol), fg);
    }
    let base_len = '\u{10EEEE}'.len_utf8();
    let low24 = match fg {
        Color::Rgb(r, g, b) => {
            (u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)) & 0x00FF_FFFF
        }
        Color::Indexed(n) => u32::from(n),
        _ => return (std::borrow::Cow::Borrowed(symbol), fg),
    };
    let diacritics: Vec<char> = symbol[base_len..].chars().collect();
    let msb = diacritics
        .get(2)
        .map(|ch| (*ch as u32).saturating_sub(0x0305))
        .unwrap_or(0);
    let source = (msb << 24) | low24;
    let Some(&mapped) = map.get(&(alternate_screen, source)) else {
        return (std::borrow::Cow::Borrowed(symbol), fg);
    };
    let mapped_msb = (mapped >> 24) & 0xFF;
    let mapped_low = mapped & 0x00FF_FFFF;
    let new_fg = Color::Rgb(
        ((mapped_low >> 16) & 0xFF) as u8,
        ((mapped_low >> 8) & 0xFF) as u8,
        (mapped_low & 0xFF) as u8,
    );
    if mapped_msb == 0 && diacritics.len() <= 2 {
        // fast path: daemon ids always fit in 24 bits today
        return (std::borrow::Cow::Borrowed(symbol), new_fg);
    }
    let mut out = String::with_capacity(symbol.len() + 4);
    out.push('\u{10EEEE}');
    for (idx, d) in diacritics.iter().enumerate() {
        if idx == 2 {
            if mapped_msb == 0 {
                break;
            }
            out.push(char::from_u32(0x0305 + mapped_msb).unwrap_or('\u{0305}'));
        } else {
            out.push(*d);
        }
    }
    (std::borrow::Cow::Owned(out), new_fg)
}

fn ansi_slot(theme: &Theme, idx: u8) -> Color {
    let c = match idx {
        0 => theme.base,
        1 => theme.alert,
        2 => theme.success,
        3 => theme.warn,
        4 => theme.info,
        5 => theme.accent,
        6 => theme.accent,
        7 => theme.text,
        8 => theme.muted,
        9 => theme.alert,
        10 => theme.success,
        11 => theme.warn,
        12 => theme.info,
        13 => theme.accent,
        14 => theme.accent,
        15 => theme.text,
        _ => return Color::Indexed(idx),
    };
    c.to_ratatui()
}

fn map_vt_color(c: vt100::Color, theme: &Theme) -> Color {
    match c {
        vt100::Color::Default => {
            // use Color::Reset for default colors to preserve terminal transparency
            Color::Reset
        }
        vt100::Color::Idx(i) => ansi_slot(theme, i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn map_vt_color_raw(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn dim_color(color: Color, base: RgbColor, factor: f64) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let rgb = RgbColor::new(r, g, b).blend(base, factor as f32);
            rgb.to_ratatui()
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::PaneView;
    use crate::config::theme::Theme;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::widgets::Widget;
    use std::collections::HashMap;

    #[test]
    fn underline_color_survives_semicolon_sgr_parse_and_render() {
        let mut parser = vt100::Parser::new(1, 2, 0);
        parser.process(b"\x1b[58;2;17;34;51mX\x1b[59mY");
        let theme = Theme::default_theme();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
        PaneView {
            screen: parser.screen(),
            theme: &theme,
            is_focused: true,
            dim_inactive: false,
            dim_strength: 0.0,
            selection: None,
            search: None,
            scroll_offset: 0,
            scrollback_len: 0,
            kitty_id_map: HashMap::new(),
        }
        .render(Rect::new(0, 0, 2, 1), &mut buffer);

        assert_eq!(buffer[(0, 0)].underline_color, Color::Rgb(17, 34, 51));
        assert_eq!(buffer[(1, 0)].underline_color, Color::Reset);
    }

    #[test]
    fn indexed_underline_color_survives_terminal_parse_and_render() {
        let mut parser = vt100::Parser::new(1, 1, 0);
        parser.process(b"\x1b[58;5;42mX");
        let theme = Theme::default_theme();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        PaneView {
            screen: parser.screen(),
            theme: &theme,
            is_focused: true,
            dim_inactive: false,
            dim_strength: 0.0,
            selection: None,
            search: None,
            scroll_offset: 0,
            scrollback_len: 0,
            kitty_id_map: HashMap::new(),
        }
        .render(Rect::new(0, 0, 1, 1), &mut buffer);

        assert_eq!(buffer[(0, 0)].underline_color, Color::Indexed(42));
    }

    #[test]
    fn underline_color_survives_colon_sgr_parse_and_render() {
        let mut parser = vt100::Parser::new(1, 2, 0);
        parser.process(b"\x1b[58:2::17:34:51mX");
        let theme = Theme::default_theme();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
        PaneView {
            screen: parser.screen(),
            theme: &theme,
            is_focused: true,
            dim_inactive: false,
            dim_strength: 0.0,
            selection: None,
            search: None,
            scroll_offset: 0,
            scrollback_len: 0,
            kitty_id_map: HashMap::new(),
        }
        .render(Rect::new(0, 0, 2, 1), &mut buffer);

        assert_eq!(buffer[(0, 0)].underline_color, Color::Rgb(17, 34, 51));
    }

    #[test]
    fn kitty_placeholder_keeps_placement_id_while_remapping_image_id() {
        let mut parser = vt100::Parser::new(1, 2, 0);
        parser.process("\x1b[38;2;0;0;42m\x1b[58;2;0;0;7m\u{10eeee}\u{0305}\u{0305}".as_bytes());
        let theme = Theme::default_theme();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
        PaneView {
            screen: parser.screen(),
            theme: &theme,
            is_focused: true,
            dim_inactive: false,
            dim_strength: 0.0,
            selection: None,
            search: None,
            scroll_offset: 0,
            scrollback_len: 0,
            kitty_id_map: HashMap::from([((false, 42), 0x0001_0203)]),
        }
        .render(Rect::new(0, 0, 2, 1), &mut buffer);

        assert_eq!(buffer[(0, 0)].fg, Color::Rgb(1, 2, 3));
        assert_eq!(buffer[(0, 0)].underline_color, Color::Rgb(0, 0, 7));
    }

    #[test]
    fn kitty_placeholder_uses_low_index_colors_as_ids_before_theme_mapping() {
        let mut parser = vt100::Parser::new(1, 1, 0);
        parser.process(b"\x1b[38;5;7m\x1b[58;5;5m\xf4\x8e\xbb\xae\xcc\x85\xcc\x85");
        let theme = Theme::default_theme();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        PaneView {
            screen: parser.screen(),
            theme: &theme,
            is_focused: true,
            dim_inactive: false,
            dim_strength: 0.0,
            selection: None,
            search: None,
            scroll_offset: 0,
            scrollback_len: 0,
            kitty_id_map: HashMap::from([((false, 7), 0x0001_0203)]),
        }
        .render(Rect::new(0, 0, 1, 1), &mut buffer);

        assert_eq!(buffer[(0, 0)].fg, Color::Rgb(1, 2, 3));
        assert_eq!(buffer[(0, 0)].underline_color, Color::Indexed(5));
    }
}
