//! regression tests for the tab context menu opened by right-clicking a
//! workspace tab on the tab bar.
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use venti::config::schema::Config;
use venti::config::theme::Theme;
use venti::input::InputMode;
use venti::proto::{MouseActionKind, MouseButtonKind, MouseEventMsg};
use venti::render::chrome::layout_workspace_bar;
use venti::server::core::{CoreEvent, ServerCore};

fn right_click(col: u16, row: u16) -> MouseEventMsg {
    MouseEventMsg {
        col,
        row,
        button: MouseButtonKind::Right,
        action: MouseActionKind::Down,
        shift: false,
        alt: false,
        ctrl: false,
    }
}

fn left_event(col: u16, row: u16, action: MouseActionKind) -> MouseEventMsg {
    MouseEventMsg {
        col,
        row,
        button: MouseButtonKind::Left,
        action,
        shift: false,
        alt: false,
        ctrl: false,
    }
}

fn key(bytes: &[u8]) -> CoreEvent {
    CoreEvent::ClientInput {
        client_id: 0,
        bytes: bytes.to_vec(),
    }
}

fn open_menu() -> (ServerCore, Arc<AtomicBool>) {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    config.layout.gap_inner = 0;
    config.layout.gap_outer = 0;
    let running = Arc::new(AtomicBool::new(true));
    let (core, _tx) = ServerCore::new(
        "ctx-test",
        config,
        Theme::default_theme(),
        running.clone(),
        None,
    );
    (core, running)
}

fn tab_bar(core: &ServerCore) -> (u16, Vec<(usize, u16, u16)>) {
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let (bar, _sidebar, _hint, _content) = venti::server::core::ServerCore::chrome_areas_for(
        &core.config,
        area,
        core.sidebar_collapsed,
    );
    let bar = bar.expect("top workspace bar must exist in this config");
    let workspaces: Vec<(usize, String, bool)> = core
        .session
        .workspace_ids()
        .into_iter()
        .map(|id| (id, core.session.workspace_name(id), false))
        .collect();
    let layout = layout_workspace_bar(bar, &workspaces, 0);
    (
        bar.y,
        layout
            .tabs
            .iter()
            .map(|t| (t.ws, t.pill_x, t.pill_w))
            .collect(),
    )
}

#[test]
fn right_click_on_tab_opens_context_menu_anchored_at_click() {
    let (mut core, _running) = open_menu();
    let (y, tabs) = tab_bar(&core);
    let (ws, pill_x, _pill_w) = tabs.into_iter().next().expect("one tab");
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: right_click(pill_x, y),
    });
    assert_eq!(core.mode, InputMode::ContextMenu);
    assert_eq!(core.context_menu.target_ws, ws);
    assert_eq!(core.context_menu.selected_idx, 0);
    assert_eq!(core.context_menu.anchor_col, pill_x);
    assert_eq!(core.context_menu.anchor_row, y);
}

#[test]
fn arrows_skip_separator_and_accelerators_run_items() {
    let (mut core, _running) = open_menu();
    // give the session a second workspace so closing one is legal
    core.handle_event(CoreEvent::ClientCommand {
        client_id: 0,
        command: "new_workspace".to_string(),
    });
    let before = core.session.workspace_ids().len();
    let (y, tabs) = tab_bar(&core);
    let (_ws, pill_x, _pill_w) = tabs
        .into_iter()
        .find(|(ws, _, _)| *ws == 1)
        .expect("ws1 tab must exist");
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: right_click(pill_x, y),
    });
    // rows: 0 new, 1 rename, 2 separator, 3 close
    core.handle_event(key(b"\x1b[B"));
    assert_eq!(core.context_menu.selected_idx, 1);
    core.handle_event(key(b"\x1b[B"));
    assert_eq!(
        core.context_menu.selected_idx, 3,
        "separator must be skipped"
    );
    core.handle_event(key(b"\x1b[B"));
    assert_eq!(core.context_menu.selected_idx, 0, "wraps to the first item");
    core.handle_event(key(b"c"));
    assert_eq!(core.mode, InputMode::Normal);
    assert_eq!(core.session.workspace_ids().len(), before - 1);
}

#[test]
fn enter_runs_selected_item() {
    let (mut core, _running) = open_menu();
    let (y, tabs) = tab_bar(&core);
    let (_ws, pill_x, _pill_w) = tabs.into_iter().next().unwrap();
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: right_click(pill_x, y),
    });
    let before = core.session.workspace_ids().len();
    core.handle_event(key(b"\r"));
    assert_eq!(core.mode, InputMode::Normal);
    assert_eq!(core.session.workspace_ids().len(), before + 1);
}

#[test]
fn mouse_hover_selects_and_click_runs() {
    let (mut core, _running) = open_menu();
    // give the session a second workspace so closing one is legal
    core.handle_event(CoreEvent::ClientCommand {
        client_id: 0,
        command: "new_workspace".to_string(),
    });
    let before = core.session.workspace_ids().len();
    let (y, tabs) = tab_bar(&core);
    let (_ws, pill_x, _pill_w) = tabs
        .into_iter()
        .find(|(ws, _, _)| *ws == 1)
        .expect("ws1 tab must exist");
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: right_click(pill_x, y),
    });
    assert_eq!(core.mode, InputMode::ContextMenu);
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let (_modal, inner) = venti::render::context_menu::context_menu_geometry(
        area,
        core.context_menu.anchor_col,
        core.context_menu.anchor_row,
    );
    // hover the last item row (close workspace), then click it
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: left_event(inner.x + 2, inner.y + 3, MouseActionKind::Moved),
    });
    assert_eq!(core.context_menu.selected_idx, 3);
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: left_event(inner.x + 2, inner.y + 3, MouseActionKind::Down),
    });
    assert_eq!(core.mode, InputMode::Normal);
    assert_eq!(core.session.workspace_ids().len(), before - 1);
}

#[test]
fn click_outside_dismisses() {
    let (mut core, _running) = open_menu();
    let (y, tabs) = tab_bar(&core);
    let (_ws, pill_x, _pill_w) = tabs.into_iter().next().unwrap();
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: right_click(pill_x, y),
    });
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: left_event(79, 23, MouseActionKind::Down),
    });
    assert_eq!(core.mode, InputMode::Normal);
}

#[test]
fn esc_cancels_menu() {
    let (mut core, _running) = open_menu();
    let (y, tabs) = tab_bar(&core);
    let (_ws, pill_x, _pill_w) = tabs.into_iter().next().unwrap();
    core.handle_event(CoreEvent::ClientMouse {
        client_id: 0,
        msg: right_click(pill_x, y),
    });
    assert_eq!(core.mode, InputMode::ContextMenu);
    core.handle_event(key(b"\x1b\x1b"));
    assert_eq!(core.mode, InputMode::Normal);
}
