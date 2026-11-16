//! regression tests for theme-picker mouse geometry: clicks must land on the
//! row the renderer painted for each theme.
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use venti::config::schema::Config;
use venti::config::theme::{BUILTIN_THEMES, Theme};
use venti::input::InputMode;
use venti::proto::{MouseActionKind, MouseButtonKind, MouseEventMsg};
use venti::render::theme_menu::{theme_menu_geometry, theme_menu_index_at};
use venti::server::core::{CoreEvent, ServerCore};

fn mouse(col: u16, row: u16) -> MouseEventMsg {
    MouseEventMsg {
        col,
        row,
        button: MouseButtonKind::Left,
        action: MouseActionKind::Down,
        shift: false,
        alt: false,
        ctrl: false,
    }
}

fn open_menu() -> (ServerCore, Arc<AtomicBool>) {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "theme-mouse-test",
        config,
        Theme::default_theme(),
        running.clone(),
        None,
    );
    core.handle_event(CoreEvent::ClientInput {
        client_id: 0,
        bytes: b"\x1b[24~".to_vec(), // F12
    });
    (core, running)
}

#[test]
fn clicking_a_theme_row_applies_that_theme() {
    let (mut core, _running) = open_menu();
    assert_eq!(core.mode, InputMode::ThemeMenu);

    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let (_modal, inner) = theme_menu_geometry(area);

    // row 4 = the fifth theme in the list
    let row = inner.y + 4;
    core.theme_menu.selected_idx = 0;
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: mouse(inner.x + 3, row),
    });

    let expected = BUILTIN_THEMES[4];
    assert_eq!(core.config.theme.name, expected);
    // menu closes and the new theme is active
    assert_eq!(core.mode, InputMode::Normal);
}

#[test]
fn theme_index_at_matches_rendered_rows() {
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let (_modal, inner) = theme_menu_geometry(area);

    for idx in 0..BUILTIN_THEMES.len() {
        let found = theme_menu_index_at(inner, 0, idx as u16);
        assert_eq!(found, Some(idx));
    }
    // beyond the list: nothing maps
    assert_eq!(
        theme_menu_index_at(inner, 0, BUILTIN_THEMES.len() as u16),
        None
    );
}
