//! regression tests for the tab context menu modal renderer.
//! modelled on iterm/zellij: anchored at the click, no title,
//! right-aligned accelerators, separator before the destructive item.
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use venti::config::theme::Theme;
use venti::render::ContextMenuState;
use venti::render::context_menu::{context_menu_geometry, render_context_menu};

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
fn context_menu_paints_items_accelerators_and_frame() {
    let theme = Theme::builtin("nord").expect("nord");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 30));
    let state = ContextMenuState {
        target_ws: 1,
        selected_idx: 1,
        anchor_col: 40,
        anchor_row: 10,
    };

    render_context_menu(buffer.area, &mut buffer, &state, &theme, false);
    let screen = rendered_lines(&buffer).join("\n");

    for text in ["new workspace", "rename workspace", "close workspace"] {
        assert!(screen.contains(text), "missing {text:?} from:\n{screen}");
    }
    let lines = rendered_lines(&buffer);
    assert!(
        lines[11].contains('╭'),
        "menu should open below the anchor:\n{screen}"
    );
}

#[test]
fn context_menu_anchors_open_below_click_and_flips_when_out_of_room() {
    let area = Rect::new(0, 0, 100, 26);
    let (modal, _) = context_menu_geometry(area, 10, 20);
    assert_eq!(modal.y, 14, "menu flips above the click at the bottom");
    let (modal, _) = context_menu_geometry(area, 90, 4);
    assert_eq!(modal.x + modal.width, 100, "menu shifts left at the edge");
}
