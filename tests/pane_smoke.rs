use std::time::{Duration, Instant};
use venti::layout::PaneId;
use venti::pane::Pane;

fn screen_text(pane: &Pane) -> String {
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

#[test]
fn pane_runs_commands_and_updates_screen() {
    let (pane, _reader, _waiter) =
        Pane::spawn(PaneId(1), 80, 24, 1000, "/bin/sh", None, "test").expect("spawn pane");

    std::thread::sleep(Duration::from_millis(300));
    pane.write_input(b"echo MARKER-12345\r");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let text = screen_text(&pane);
        if text.contains("MARKER-12345") {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "command output never appeared; screen contents:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
