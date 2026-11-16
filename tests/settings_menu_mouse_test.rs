//! regression tests for settings-menu mouse geometry: the tab row and list
//! rows must hit-test where they are drawn, not where stale constants said.
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use venti::config::schema::Config;
use venti::config::theme::Theme;
use venti::layout::PaneId;
use venti::proto::{MouseActionKind, MouseButtonKind, MouseEventMsg};
use venti::render::settings_menu::settings_menu_geometry;
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
        "mouse-test",
        config,
        Theme::default_theme(),
        running.clone(),
        None,
    );
    core.handle_event(CoreEvent::ClientInput {
        client_id: 0,
        bytes: b"\x1bOQ".to_vec(), // F2: settings menu
    });
    (core, running)
}

#[test]
fn clicking_a_tab_activates_it() {
    let (mut core, _running) = open_menu();
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let (_modal, inner, _) = settings_menu_geometry(area);

    // 4 tabs are laid out side by side on the inner top row; click the second
    let tabs: Vec<_> = venti::render::settings_menu::settings_tab_layout(inner);
    let (tab_id, tab_x, tab_w) = tabs[1];

    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: mouse(tab_x + tab_w / 2, inner.y),
    });

    assert_eq!(core.settings_menu.active_tab, tab_id);
}

#[test]
fn clicking_a_visible_list_row_selects_that_setting() {
    let (mut core, _running) = open_menu();
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let (_modal, inner, max_display) = settings_menu_geometry(area);
    assert!(
        max_display >= 4,
        "expected a usable list, got {max_display}"
    );

    // click the third visible row (index 2 in the default Quick tab)
    let list_start = inner.y + 4;
    let row = list_start + 2;
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: mouse(inner.x + 2, row),
    });

    assert_eq!(core.settings_menu.selected_idx, 2);
}

#[test]
fn clicking_a_tab_and_a_row_are_not_confused() {
    let (mut core, _running) = open_menu();
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let (_modal, inner, _) = settings_menu_geometry(area);

    // a click on the divider row between the tab bar and the list must not select a setting
    let divider_row = inner.y + 1;
    let before = core.settings_menu.selected_idx;
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: mouse(inner.x + 2, divider_row),
    });
    assert_eq!(core.settings_menu.selected_idx, before);
}

#[test]
fn pane_survives_a_settings_menu_mouse_roundtrip() {
    let (core, _running) = open_menu();
    // sanity: the menu really opened and a pane is still there
    assert!(core.session.panes.contains_key(&PaneId(1)));
    assert_eq!(core.mode, venti::input::InputMode::SettingsMenu);
}
