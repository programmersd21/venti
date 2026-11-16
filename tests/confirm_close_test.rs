use std::time::{Duration, Instant};
use venti::layout::PaneId;
use venti::pane::Pane;
use venti::util::process::has_busy_children;

#[test]
fn busy_detection_tracks_foreground_children() {
    let (pane, _reader, _waiter) =
        Pane::spawn(PaneId(1), 80, 24, 100, "/bin/sh", None, "busy-test").expect("spawn pane");
    let pid = pane.child_pid.expect("child pid");

    // shell at the prompt has no foreground children
    assert!(!has_busy_children(pid));

    // launch a foreground pipeline; the shell gains a child
    pane.write_input(b"cat\n");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if has_busy_children(pid) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "child process never appeared under the shell"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    // ctrl+c kills cat; children disappear
    pane.write_input(b"\x03");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if !has_busy_children(pid) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "foreground child never exited after ctrl+c"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
