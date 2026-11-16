use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use venti::config::theme::Theme;
use venti::render::render_welcome;

fn assert_all_welcome_text_is_lowercase(buffer: &Buffer) {
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            for ch in buffer[(x, y)].symbol().chars() {
                assert!(!ch.is_ascii_uppercase(), "uppercase {ch} at {x},{y}");
            }
        }
    }
}

fn rendered_lines(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn full_welcome_contains_every_visible_string() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));

    render_welcome(buffer.area, &mut buffer, &theme, false);
    assert_all_welcome_text_is_lowercase(&buffer);
    let screen = rendered_lines(&buffer).join("\n");

    for text in [
        "venti",
        "a fast, quiet terminal multiplexer",
        "f3",
        "split pane",
        "f4",
        "close pane",
        "f5",
        "toggle zoom",
        "f11",
        "command palette",
        "alt+n",
        "new workspace",
        "alt+1",
        "switch to ws 1",
        "alt+w",
        "close workspace",
        "f2",
        "settings",
        "f1",
        "help",
        "f12",
        "theme picker",
        "f9",
        "copy mode",
        "alt+arrows",
        "focus pane",
        "press any key · f1 for help",
    ] {
        assert!(screen.contains(text), "missing {text:?} from:\n{screen}");
    }
}

#[test]
fn compact_welcome_contains_every_visible_string() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 48, 18));

    render_welcome(buffer.area, &mut buffer, &theme, false);
    assert_all_welcome_text_is_lowercase(&buffer);
    let screen = rendered_lines(&buffer).join("\n");

    for text in [
        "venti",
        "f1",
        "help",
        "f2",
        "settings",
        "f3",
        "split pane",
        "f4",
        "close pane",
        "alt+space",
        "palette",
        "press any key",
    ] {
        assert!(screen.contains(text), "missing {text:?} from:\n{screen}");
    }
}
