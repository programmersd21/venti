use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use venti::config::schema::Config;
use venti::config::theme::Theme;
use venti::proto::{MouseActionKind, MouseButtonKind, MouseEventMsg};
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

fn input(core: &mut ServerCore, bytes: &[u8]) {
    core.handle_event(CoreEvent::ClientInput {
        client_id: 0,
        bytes: bytes.to_vec(),
    });
}

fn mouse_event(core: &mut ServerCore, msg: MouseEventMsg) {
    core.handle_event(CoreEvent::ClientMouse { client_id: 0, msg });
}

#[test]
fn alt_keys_work_with_and_without_focus() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "ctrl-passthrough-test",
        config,
        Theme::default_theme(),
        running,
        None,
    );

    // alt+n is bound to NewWorkspace
    input(&mut core, b"\x1bn");
    assert_eq!(core.session.workspace_ids().len(), 2);

    // click outside to unfocus; alt keys still drive venti (only passthrough overrides)
    mouse_event(&mut core, mouse(500, 500));
    assert_eq!(core.session.focused_pane, None);

    // with nothing focused, alt+n is still a venti new-workspace key
    input(&mut core, b"\x1bn");
    assert_eq!(core.session.workspace_ids().len(), 3);
}

#[test]
fn alt_q_closes_even_with_pane_focused() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    // close is immediate only when the confirm prompt is off; the default
    // (confirm when a child has busy descendants) depends on how fast the
    // freshly spawned shell happens to run right now.
    config.general.confirm_close_running = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "ctrl-close-test",
        config,
        Theme::default_theme(),
        running,
        None,
    );
    assert_eq!(core.session.panes.len(), 1);

    // passthrough is empty -> alt+q is venti's close pane, even with pane focused
    input(&mut core, b"\x1bq"); // esc + q = alt+q
    assert_eq!(core.session.panes.len(), 0);
}

#[test]
fn passthrough_alt_reaches_app() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    config.input.passthrough = vec!["alt+n".to_string()];
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "ctrl-passthrough-test2",
        config,
        Theme::default_theme(),
        running,
        None,
    );

    // alt+n is in passthrough, so venti must not intercept it
    input(&mut core, b"\x1bn");
    assert_eq!(
        core.session.workspace_ids().len(),
        1,
        "passthrough alt+n is not a venti binding"
    );
}
