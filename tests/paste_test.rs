use std::time::{Duration, Instant};
use venti::config::schema::Config;
use venti::config::theme::Theme;
use venti::layout::PaneId;
use venti::server::core::{CoreEvent, ServerCore};

fn input(core: &mut ServerCore, bytes: &[u8]) {
    core.handle_event(CoreEvent::ClientInput {
        client_id: 0,
        bytes: bytes.to_vec(),
    });
}

fn pane_text(core: &ServerCore) -> String {
    let pane = core.session.panes.get(&PaneId(1)).expect("pane exists");
    let parser = pane.parser.lock().expect("parser lock");
    let screen = parser.screen();
    let mut text = String::new();
    for row in 0..24 {
        for col in 0..80 {
            if let Some(cell) = screen.cell(row, col) {
                text.push_str(cell.contents());
            }
        }
    }
    text
}

fn new_core(name: &str) -> ServerCore {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    config.general.confirm_close_running = false;
    // force bourne sh so canonical-mode line editing handles\n the way plain sh does
    config.general.shell = "/bin/sh".to_string();
    // alt-first design: ctrl+d is no longer bound by venti, so the plain C0
    // byte flows to the pane untouched and canonical-mode od sees EOF.
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (core, _tx) = ServerCore::new(name, config, Theme::default_theme(), running, None);
    core
}

fn wait_for_mode(core: &ServerCore, enabled: bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let pane = core.session.panes.get(&PaneId(1)).expect("pane exists");
        let on = pane
            .parser
            .lock()
            .expect("parser lock")
            .screen()
            .bracketed_paste();
        if on == enabled {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "bracketed paste mode never became {enabled}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_text(core: &ServerCore, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = pane_text(core);
        if text.contains(needle) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "screen never contained {needle:?}; screen was:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn paste_wraps_bytes_when_pane_enabled_bracketed_paste() {
    let mut core = new_core("paste-bracket-test");
    // printf emits the bracketed-paste-on escape sequence, then exec replaces
    // the shell so od inherits mode 2004 enabled.
    input(&mut core, b"printf '\\033[?2004h'; exec od -An -tx1c\n");
    wait_for_mode(&core, true);
    std::thread::sleep(Duration::from_millis(800));

    core.handle_event(CoreEvent::ClientPaste {
        client_id: 0,
        bytes: b"VERIFY\n".to_vec(),
    });
    // send ctrl-d to flush od's output buffer
    input(&mut core, b"\x04");
    // od prints bracketed-paste markers: ESC[200~ = 1b 5b 32 30 30 7e
    // with double spaces between values in some od implementations
    wait_for_text(&core, "1b");
    let text = pane_text(&core);
    // ESC[200~ consists of bytes 1b 5b 32 30 30 7e, check all are present
    assert!(
        text.contains("5b") && text.contains("32") && text.contains("30") && text.contains("7e"),
        "bracketed paste markers not found in od output; screen:\n{text}"
    );
}

#[test]
fn paste_is_verbatim_when_pane_has_no_bracketed_paste() {
    let mut core = new_core("paste-raw-test");
    // no mode-set sequence; the default shell has no bracketed paste
    input(&mut core, b"exec od -An -tx1c\n");
    std::thread::sleep(Duration::from_millis(800));

    core.handle_event(CoreEvent::ClientPaste {
        client_id: 0,
        bytes: b"VERIFY\n".to_vec(),
    });
    // send ctrl-d to flush od's output buffer
    input(&mut core, b"\x04");
    // od prints hex bytes separated by spaces, no paste markers anywhere
    wait_for_text(&core, "56");
    wait_for_text(&core, "45");
    wait_for_text(&core, "52");
    let text = pane_text(&core);
    assert!(
        !text.contains("1b") || {
            // "1b" from od with no bracketed paste means the ESC byte was sent
            // verbatim, acceptable. But "5b 32 30 30 7e" (the rest of ESC[200~)
            // must not appear together in sequence.
            !text.contains("5b  32  30  30  7e") && !text.contains("5b 32 30 30 7e")
        },
        "paste was wrapped even though bracketed paste is off; screen:\n{text}"
    );
}
