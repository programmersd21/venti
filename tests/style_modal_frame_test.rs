use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use venti::config::theme::Theme;
use venti::render::style::{modal_footer, modal_frame};

#[test]
fn modal_frame_renders_border_and_footer() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let area = Rect::new(0, 0, 20, 8);
    let mut buffer = Buffer::empty(area);

    let inner = modal_frame(&mut buffer, area, &theme, false, Some("title"));
    modal_footer(&mut buffer, area, &theme, "hint");

    assert_ne!(buffer[(0, 0)].symbol(), " ");
    assert_ne!(buffer[(area.width - 1, 0)].symbol(), " ");
    assert_ne!(buffer[(0, area.height - 1)].symbol(), " ");
    assert_ne!(buffer[(area.width - 1, area.height - 1)].symbol(), " ");
    assert_eq!(inner, Rect::new(1, 1, 18, 6));
}
