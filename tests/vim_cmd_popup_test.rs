use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use venti::config::theme::Theme;
use venti::render::render_vim_cmd_popup;

#[test]
fn vim_cmd_popup_renders_above_the_prompt_bar() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 24));
    let hint = Rect::new(0, 23, 100, 1);

    render_vim_cmd_popup(hint, &mut buf, "split", 0, &theme, false);

    let rendered: String = (0..24)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("commands"));
    assert!(rendered.contains("split_right"));
}
