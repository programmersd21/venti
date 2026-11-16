use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};
use venti::config::schema::Config;
use venti::layout::PaneId;
use venti::proto::{MouseActionKind, MouseButtonKind, MouseEventMsg};
use venti::server::core::{CoreEvent, MouseDragState, ServerCore};
use venti::server::session::Session;

fn mouse(col: u16, row: u16, button: MouseButtonKind, action: MouseActionKind) -> MouseEventMsg {
    MouseEventMsg {
        col,
        row,
        button,
        action,
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

fn command(core: &mut ServerCore, cmd: &str) {
    core.handle_event(CoreEvent::ClientCommand {
        client_id: 0,
        command: cmd.to_string(),
    });
}

fn mouse_event(core: &mut ServerCore, msg: MouseEventMsg) {
    core.handle_event(CoreEvent::ClientMouse { client_id: 0, msg });
}

fn wait_for<F: FnMut() -> bool>(mut pred: F, dur: Duration) -> bool {
    let start = Instant::now();
    loop {
        if pred() {
            return true;
        }
        if start.elapsed() > dur {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn pid_alive(pid: u32) -> bool {
    rustix::process::Pid::from_raw(pid as i32)
        .map(|p| rustix::process::test_kill_process(p).is_ok())
        .unwrap_or(false)
}

fn has_descendant_command(pid: u32, executable: &[u8], argument: &[u8]) -> bool {
    venti::util::process::descendants_of(pid)
        .into_iter()
        .filter_map(|child| std::fs::read(format!("/proc/{child}/cmdline")).ok())
        .any(|cmdline| {
            let mut args = cmdline
                .split(|byte| *byte == 0)
                .filter(|arg| !arg.is_empty());
            let Some(program) = args.next() else {
                return false;
            };
            let Some(arg) = args.next() else {
                return false;
            };
            (program == executable
                || program.rsplit(|byte| *byte == b'/').next() == Some(executable))
                && arg == argument
        })
}

fn float_rect(session: &Session) -> ratatui::layout::Rect {
    let ws = session.active_workspace;
    session
        .floating_panes
        .get(&ws)
        .and_then(|l| l.first().map(|f| f.rect))
        .expect("a floating pane exists")
}

#[test]
fn f6_floats_and_f5_zooms() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "float-toggle-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    // f6 toggles the pane into a float
    input(&mut core, b"\x1b[17~");
    let rect = float_rect(&core.session);
    assert!(
        rect.width < 80 && rect.height < 24,
        "float is smaller than fullscreen"
    );

    // f5 toggles zoom, which is fullscreen but not a float
    input(&mut core, b"\x1b[15~");
    assert_eq!(core.session.zoomed, Some(PaneId(1)), "f5 zooms");

    // f5 again un-zooms
    input(&mut core, b"\x1b[15~");
    assert_eq!(core.session.zoomed, None);

    // f6 again un-floats the pane back into tiling
    input(&mut core, b"\x1b[17~");
    assert!(!core.session.floating_panes.contains_key(&1));
}

#[test]
fn float_drag_and_resize_with_mouse() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "float-mouse-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    // float pane 1
    input(&mut core, b"\x1b[17~");
    let before = float_rect(&core.session);

    // drag the float by the top row
    mouse_event(
        &mut core,
        mouse(
            before.x + 2,
            before.y,
            MouseButtonKind::Left,
            MouseActionKind::Down,
        ),
    );
    assert!(matches!(
        core.mouse_drag,
        MouseDragState::MoveFloating { .. }
    ));

    mouse_event(
        &mut core,
        mouse(
            before.x + 7,
            before.y,
            MouseButtonKind::Left,
            MouseActionKind::Drag,
        ),
    );
    let after = float_rect(&core.session);
    assert_eq!(after.x, before.x + 5, "drag moved the float");
    assert_eq!(after.y, before.y);

    mouse_event(
        &mut core,
        mouse(
            before.x + 7,
            before.y,
            MouseButtonKind::Left,
            MouseActionKind::Up,
        ),
    );
    assert_eq!(core.mouse_drag, MouseDragState::None);

    // resize from the bottom-right corner
    let corner_x = after.x + after.width - 1;
    let corner_y = after.y + after.height - 1;
    mouse_event(
        &mut core,
        mouse(
            corner_x,
            corner_y,
            MouseButtonKind::Left,
            MouseActionKind::Down,
        ),
    );
    assert!(matches!(
        core.mouse_drag,
        MouseDragState::ResizeFloatingCorner {
            corner: venti::server::core::FloatingCorner::BottomRight,
            ..
        }
    ));

    mouse_event(
        &mut core,
        mouse(
            corner_x + 3,
            corner_y + 2,
            MouseButtonKind::Left,
            MouseActionKind::Drag,
        ),
    );
    let resized = float_rect(&core.session);
    assert_eq!(resized.width, after.width + 3);
    assert_eq!(resized.height, after.height + 2);

    mouse_event(
        &mut core,
        mouse(
            corner_x + 3,
            corner_y + 2,
            MouseButtonKind::Left,
            MouseActionKind::Up,
        ),
    );
    assert_eq!(core.mouse_drag, MouseDragState::None);
}

#[test]
fn float_resizes_from_every_corner() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "float-corner-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    input(&mut core, b"\x1b[17~");
    let origin = float_rect(&core.session);

    for (label, (dx, dy)) in [
        ("bottomright", (origin.width - 1, origin.height - 1)),
        ("bottomleft", (0, origin.height - 1)),
        ("topleft", (0, 0)),
        ("topright", (origin.width - 1, 0)),
    ] {
        // press on the corner
        mouse_event(
            &mut core,
            mouse(
                origin.x + dx,
                origin.y + dy,
                MouseButtonKind::Left,
                MouseActionKind::Down,
            ),
        );
        assert!(
            matches!(core.mouse_drag, MouseDragState::ResizeFloatingCorner { .. }),
            "down on {label} must start a corner resize"
        );
        mouse_event(
            &mut core,
            mouse(
                origin.x + dx,
                origin.y + dy,
                MouseButtonKind::Left,
                MouseActionKind::Up,
            ),
        );
    }

    // side rows must not start a move or resize
    mouse_event(
        &mut core,
        mouse(
            origin.x,
            origin.y + 2,
            MouseButtonKind::Left,
            MouseActionKind::Down,
        ),
    );
    assert_eq!(
        core.mouse_drag,
        MouseDragState::None,
        "side row must not drag"
    );

    // exact bottom-left corner starts a bottom-left resize
    mouse_event(
        &mut core,
        mouse(
            origin.x,
            origin.y + origin.height - 1,
            MouseButtonKind::Left,
            MouseActionKind::Down,
        ),
    );
    assert!(
        matches!(
            core.mouse_drag,
            MouseDragState::ResizeFloatingCorner {
                corner: venti::server::core::FloatingCorner::BottomLeft,
                ..
            }
        ),
        "bottom-left corner must start bottom-left resize"
    );
}

#[test]
fn close_pane_kills_its_process_tree() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    config.general.confirm_close_running = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "float-close-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    let shell_pid = core
        .session
        .panes
        .get(&PaneId(1))
        .and_then(|p| p.child_pid)
        .expect("shell has a pid");
    assert!(pid_alive(shell_pid));

    // start a persistent background job
    input(&mut core, b"sleep 31401 &\n");
    let backgrounded = wait_for(
        || {
            std::fs::read_dir("/proc").unwrap().flatten().any(|e| {
                std::fs::read_to_string(e.path().join("cmdline"))
                    .map(|c| c.contains("31401"))
                    .unwrap_or(false)
            })
        },
        Duration::from_secs(5),
    );
    assert!(backgrounded, "background sleep never started");

    // close the pane with f4
    input(&mut core, b"\x1bOS");

    assert!(
        wait_for(
            || !core.session.panes.contains_key(&PaneId(1)),
            Duration::from_secs(2)
        ),
        "pane never closed"
    );
    assert!(
        wait_for(|| !pid_alive(shell_pid), Duration::from_secs(2)),
        "shell process survived the close"
    );
    assert!(
        wait_for(
            || !std::fs::read_dir("/proc").unwrap().flatten().any(|e| {
                std::fs::read_to_string(e.path().join("cmdline"))
                    .map(|c| c.contains("31401"))
                    .unwrap_or(false)
            }),
            Duration::from_secs(2),
        ),
        "background job survived the close"
    );
}

#[test]
fn close_workspace_kills_its_panes() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "ws-close-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    // create workspace 2 and capture its pane's shell pid
    command(&mut core, "new_workspace");
    command(&mut core, "workspace_goto 2");
    let ws_pane = core.session.focused_pane.expect("pane in ws 2");
    let shell_pid = core.session.panes.get(&ws_pane).and_then(|p| p.child_pid);

    command(&mut core, "close_workspace");
    assert!(!core.session.workspaces.contains_key(&2));
    assert!(!core.session.panes.contains_key(&ws_pane));

    if let Some(pid) = shell_pid {
        assert!(
            wait_for(|| !pid_alive(pid), Duration::from_secs(2)),
            "workspace pane's shell survived the workspace close"
        );
    }
}

#[test]
fn close_pane_with_busy_child_requires_double_confirm() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    config.general.confirm_close_running = true;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "confirm-close-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    // make the shell busy with a persistent background job
    input(&mut core, b"sleep 31402 &\n");
    // wait until the pane's busy check itself reports work; scanning /proc
    // for a magic number is slow under parallel test load.
    let busy_found = wait_for(
        || {
            core.session
                .panes
                .get(&PaneId(1))
                .and_then(|pane| pane.child_pid)
                .is_some_and(|pid| has_descendant_command(pid, b"sleep", b"31402"))
        },
        Duration::from_secs(15),
    );
    assert!(busy_found, "background sleep never started");

    // first close attempt must NOT close the pane
    input(&mut core, b"\x1bOS");
    // F4 is the close key; the first press shows the confirm toast.
    assert!(core.session.panes.contains_key(&PaneId(1)));

    // second attempt closes it (within pending_close window)
    input(&mut core, b"\x1bOS");
    assert!(!core.session.panes.contains_key(&PaneId(1)));
}

#[test]
fn can_focus_floating_pane_when_tiled_pane_exists() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "float-focus-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    // pane 1 exists as tiled by default
    assert_eq!(core.session.focused_pane, Some(PaneId(1)));

    // split to create pane 2 as a second tiled pane
    command(&mut core, "split_right");
    let pane2 = core.session.focused_pane;
    assert_ne!(pane2, Some(PaneId(1)), "split creates a new pane");
    assert!(pane2.is_some());

    // float pane 2
    input(&mut core, b"\x1b[17~");
    let floating_pane = core.session.focused_pane;
    assert_eq!(floating_pane, pane2);
    let float_rect = float_rect(&core.session);

    // click on the floating pane to keep it focused (sanity check)
    mouse_event(
        &mut core,
        mouse(
            float_rect.x + 5,
            float_rect.y + 2,
            MouseButtonKind::Left,
            MouseActionKind::Down,
        ),
    );
    assert_eq!(
        core.session.focused_pane, floating_pane,
        "clicking floating pane keeps it focused"
    );

    // click on pane 1 (the tiled pane) to focus it
    mouse_event(
        &mut core,
        mouse(
            5, // somewhere in the tiled pane area
            15,
            MouseButtonKind::Left,
            MouseActionKind::Down,
        ),
    );
    assert_eq!(
        core.session.focused_pane,
        Some(PaneId(1)),
        "clicking tiled pane focuses it"
    );

    // now click on the floating pane again to focus it back
    mouse_event(
        &mut core,
        mouse(
            float_rect.x + 5,
            float_rect.y + 2,
            MouseButtonKind::Left,
            MouseActionKind::Down,
        ),
    );
    assert_eq!(
        core.session.focused_pane, floating_pane,
        "clicking floating pane while tiled is focused should focus floating"
    );
}

#[test]
fn can_focus_floating_pane_that_is_on_top_of_tiled() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "float-z-order-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    // pane 1 is tiled by default
    assert_eq!(core.session.focused_pane, Some(PaneId(1)));

    // split to create pane 2
    command(&mut core, "split_right");
    let pane2 = core.session.focused_pane;

    // split pane 2 down to create pane 3 (under pane 2)
    command(&mut core, "split_down");
    let pane3 = core.session.focused_pane;

    // float pane 2
    core.session.focused_pane = pane2;
    input(&mut core, b"\x1b[17~");
    let float_rect = float_rect(&core.session);

    // Verify the floating pane list is not empty
    let ws = core.session.active_workspace;
    let floating_list = core.session.floating_panes.get(&ws);
    assert!(
        floating_list.is_some_and(|l| !l.is_empty()),
        "floating_panes list should not be empty after making pane 2 float"
    );

    // Verify the floating pane's ID is in the list
    let has_float =
        floating_list.is_some_and(|l| l.iter().any(|f| f.id == pane2.expect("pane2 should exist")));
    assert!(has_float, "pane2 should be in the floating list");

    // Verify workspace_pane_ids includes the floating pane
    let all_pane_ids = core.session.workspace_pane_ids(ws);
    assert!(
        all_pane_ids.contains(&pane2.expect("pane2 should exist")),
        "workspace_pane_ids should include floating pane"
    );

    // focus pane 3 (tiled)
    core.session.focused_pane = pane3;
    assert_eq!(core.session.focused_pane, pane3);

    // click on the floating pane (which should be visually on top)
    mouse_event(
        &mut core,
        mouse(
            float_rect.x + 3,
            float_rect.y + 2,
            MouseButtonKind::Left,
            MouseActionKind::Down,
        ),
    );

    // Should switch focus to the floating pane
    assert_eq!(
        core.session.focused_pane, pane2,
        "clicking on floating pane should focus it, even when tiled panes are underneath"
    );
}

#[test]
fn floating_pane_focus_persists_with_complex_tiled_layout() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "complex-float-focus-test",
        config,
        venti::config::theme::Theme::default_theme(),
        running,
        None,
    );

    // Create a complex tiled layout:
    //   Pane 1 | Pane 2
    //   -----+-----
    //   Pane 3 | Pane 4
    command(&mut core, "split_right"); // pane 2
    core.session.focused_pane = Some(PaneId(1));
    command(&mut core, "split_down"); // pane 3
    command(&mut core, "split_right"); // pane 4

    // Now float pane 2 (which was in the top-right)
    core.session.focused_pane = Some(PaneId(2));
    input(&mut core, b"\x1b[17~");
    let float_rect = float_rect(&core.session);

    // Cycle through tiled panes and verify we can always return to floating
    for _ in 0..3 {
        // Focus pane 1 (tiled)
        core.session.focused_pane = Some(PaneId(1));
        assert_eq!(core.session.focused_pane, Some(PaneId(1)));

        // Click on floating pane
        mouse_event(
            &mut core,
            mouse(
                float_rect.x + 2,
                float_rect.y + 2,
                MouseButtonKind::Left,
                MouseActionKind::Down,
            ),
        );
        assert_eq!(
            core.session.focused_pane,
            Some(PaneId(2)),
            "should be able to focus floating pane from any tiled pane"
        );

        // Focus pane 3 (different part of tiled layout)
        core.session.focused_pane = Some(PaneId(3));
        assert_eq!(core.session.focused_pane, Some(PaneId(3)));

        // Click on floating pane again
        mouse_event(
            &mut core,
            mouse(
                float_rect.x + 3,
                float_rect.y + 3,
                MouseButtonKind::Left,
                MouseActionKind::Down,
            ),
        );
        assert_eq!(
            core.session.focused_pane,
            Some(PaneId(2)),
            "should consistently focus floating pane"
        );
    }
}
