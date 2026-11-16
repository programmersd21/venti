//! regression tests for the optional scroll indicator in the pane title.
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use venti::config::schema::BorderStyle;
use venti::config::theme::Theme;
use venti::render::render_pane_border;

fn rendered(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn scroll_indicator_shows_lines_above_the_bottom() {
    let theme = Theme::builtin("nord").expect("nord");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 3));
    render_pane_border(
        buffer.area,
        &mut buffer,
        "zsh",
        true,
        &theme,
        false,
        BorderStyle::Rounded,
        Some(12),
    );
    let screen = rendered(&buffer);
    assert!(
        screen.contains("\u{2191}12"),
        "scroll indicator missing from:\n{screen}"
    );
}

#[test]
fn scroll_indicator_is_absent_without_scroll() {
    let theme = Theme::builtin("nord").expect("nord");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 3));
    render_pane_border(
        buffer.area,
        &mut buffer,
        "zsh",
        true,
        &theme,
        false,
        BorderStyle::Rounded,
        None,
    );
    let screen = rendered(&buffer);
    assert!(
        !screen.contains('\u{2191}'),
        "no indicator expected when not scrolled:\n{screen}"
    );
}
