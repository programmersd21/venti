//! a tiny ASCII cat that visits the focused pane via `:cat`.
//!
//! the cat lives inside the pane's content rect, bottom-right, drawn over the
//! pane view. eyes blink on a fixed cadence with an occasional double-blink,
//! whiskers twitch, and a soft one-word thought bubble drifts by now and then.
//! clicking the cat boops it (mrrp). all animation is a pure function of the
//! spawn time so the renderer stays stateless.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::config::theme::Theme;
use crate::render::glyphs::Glyphs;
use crate::render::style::fill;

/// total size of the cat box, including its border.
pub const CAT_WIDTH: u16 = 22;
pub const CAT_HEIGHT: u16 = 5;

/// returns the rect the cat box occupies inside `inner` (top-right corner,
/// one cell of breathing room under the border), or none when the pane is too
/// small to host it. anchored to the top because the pane's bottom edge is the
/// prompt/cursor row and must never be covered.
pub fn cat_rect(inner: Rect) -> Option<Rect> {
    if inner.width < CAT_WIDTH + 1 || inner.height < CAT_HEIGHT + 1 {
        return None;
    }
    Some(Rect::new(
        inner.x + inner.width - CAT_WIDTH - 1,
        inner.y + 1,
        CAT_WIDTH,
        CAT_HEIGHT,
    ))
}

/// renders the cat box over the focused pane's content area.
///
/// `elapsed` is how long the cat has been here; `boop_age` is how long ago it
/// was booped (if recently), which summons a heart.
pub fn render_cat(
    inner: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    elapsed: std::time::Duration,
    boop_age: Option<std::time::Duration>,
    ascii: bool,
) {
    let Some(rect) = cat_rect(inner) else {
        return;
    };
    let surface = theme.surface_color();
    let text = theme.text_color();
    let accent = theme.accent_color();
    let muted = theme.muted_color();

    let glyphs = Glyphs::styled(ascii, crate::config::schema::BorderStyle::Rounded);
    fill(buf, rect, Style::default().fg(text).bg(surface));

    // border
    for x in rect.x..rect.x + rect.width {
        buf[(x, rect.y)]
            .set_char(glyphs.h)
            .set_style(Style::default().fg(accent).bg(surface));
        buf[(x, rect.y + rect.height - 1)]
            .set_char(glyphs.h)
            .set_style(Style::default().fg(accent).bg(surface));
    }
    for y in rect.y + 1..rect.y + rect.height - 1 {
        buf[(rect.x, y)]
            .set_char(glyphs.v)
            .set_style(Style::default().fg(accent).bg(surface));
        buf[(rect.x + rect.width - 1, y)]
            .set_char(glyphs.v)
            .set_style(Style::default().fg(accent).bg(surface));
    }
    buf[(rect.x, rect.y)]
        .set_char(glyphs.top_left)
        .set_style(Style::default().fg(accent).bg(surface));
    buf[(rect.x + rect.width - 1, rect.y)]
        .set_char(glyphs.top_right)
        .set_style(Style::default().fg(accent).bg(surface));
    buf[(rect.x, rect.y + rect.height - 1)]
        .set_char(glyphs.bottom_left)
        .set_style(Style::default().fg(accent).bg(surface));
    buf[(rect.x + rect.width - 1, rect.y + rect.height - 1)]
        .set_char(glyphs.bottom_right)
        .set_style(Style::default().fg(accent).bg(surface));

    // title
    let label = " cat ";
    let label_x = rect.x + (rect.width.saturating_sub(label.chars().count() as u16)) / 2;
    buf.set_stringn(
        label_x,
        rect.y,
        label,
        label.chars().count(),
        Style::default()
            .fg(accent)
            .bg(surface)
            .add_modifier(Modifier::BOLD),
    );

    let t = elapsed.as_secs_f64();
    // blink: a quick close every 3.7s, plus a dreamy double-blink occasionally.
    let single = t % 3.7 > 3.42;
    let double = t % 12.0 > 8.1 && t % 12.0 < 8.55;
    let blink = single || double;
    // whisker twitch: every ~0.8s the whiskers sharpen for a beat.
    let twitch = (t * 1.25).fract() > 0.82;
    // thought bubble: drifts by for ~2.2s every 8s.
    let cycle = t / 8.0;
    let drifting = cycle.fract() < 0.275;
    const THOUGHTS: [&str; 5] = ["purr~", "mrrp", "mew", "*kneads*", "nya~"];
    let thought = drifting.then(|| THOUGHTS[(cycle as usize) % THOUGHTS.len()]);

    let art_x = rect.x + 2;
    let ears_style = Style::default()
        .fg(accent)
        .bg(surface)
        .add_modifier(Modifier::BOLD);
    let face_style = Style::default().fg(text).bg(surface);
    let eye_style = Style::default()
        .fg(accent)
        .bg(surface)
        .add_modifier(Modifier::BOLD);
    let muted_style = Style::default().fg(muted).bg(surface);

    // ears
    buf.set_stringn(art_x, rect.y + 1, " /\\_/\\", 6, ears_style);

    // face: parens in text color, eyes in accent
    let eyes = if blink { "-.-" } else { "o.o" };
    buf.set_stringn(art_x, rect.y + 2, "( ", 2, face_style);
    buf.set_stringn(art_x + 2, rect.y + 2, eyes, 3, eye_style);
    buf.set_stringn(art_x + 5, rect.y + 2, " )", 2, face_style);

    // whiskers / paws line
    let paws = if twitch { " >-^-<" } else { " > ^ <" };
    buf.set_stringn(art_x, rect.y + 3, paws, 6, face_style);

    // thought bubble
    if let Some(thought) = thought {
        buf.set_stringn(
            art_x + 9,
            rect.y + 2,
            thought,
            (rect.width.saturating_sub(11)) as usize,
            muted_style.add_modifier(Modifier::ITALIC),
        );
    }

    // boop heart, top-right corner of the box
    if boop_age.is_some_and(|age| age < std::time::Duration::from_secs_f64(1.2)) {
        buf.set_stringn(
            rect.x + rect.width - 3,
            rect.y,
            if ascii { "<3" } else { "♥" },
            2,
            Style::default()
                .fg(theme.alert_color())
                .bg(surface)
                .add_modifier(Modifier::BOLD),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    fn render_at(t: f64) -> String {
        let area = Rect::new(0, 0, 40, 12);
        let mut buf = Buffer::empty(area);
        let theme = crate::config::theme::Theme::default_theme();
        render_cat(
            area,
            &mut buf,
            &theme,
            std::time::Duration::from_secs_f64(t),
            None,
            false,
        );
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn cat_draws_its_face_with_open_eyes() {
        let screen = render_at(0.0);
        assert!(screen.contains("/\\_/\\"));
        assert!(screen.contains("( o.o )"));
        assert!(screen.contains("> ^ <"));
    }

    #[test]
    fn cat_blinks_its_eyes() {
        let screen = render_at(3.5);
        assert!(screen.contains("( -.- )"));
        assert!(!screen.contains("( o.o )"));
    }

    #[test]
    fn cat_too_small_pane_renders_nothing() {
        let area = Rect::new(0, 0, 10, 3);
        let mut buf = Buffer::empty(area);
        let theme = crate::config::theme::Theme::default_theme();
        render_cat(
            area,
            &mut buf,
            &theme,
            std::time::Duration::from_secs(0),
            None,
            false,
        );
        for cell in buf.content.iter() {
            assert_eq!(cell.symbol(), " ");
        }
    }

    #[test]
    fn boop_summons_a_heart() {
        let area = Rect::new(0, 0, 40, 12);
        let mut buf = Buffer::empty(area);
        let theme = crate::config::theme::Theme::default_theme();
        render_cat(
            area,
            &mut buf,
            &theme,
            std::time::Duration::from_secs(1),
            Some(std::time::Duration::from_secs(0)),
            false,
        );
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(out.contains('♥'));
    }
}
