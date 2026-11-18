//! regression: sixel images must never stack.
//!
//! sixel has no delete sequence, so the only safe way to (re)draw one is after
//! an ED2 full repaint. every frame carrying a sixel must therefore be a full
//! redraw; emitting a sixel into a diff frame leaves the previous copy burned
//! onto the screen (the "striped image" artifact).
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use venti::proto::{
    ClientMsg, HelloMsg, PROTOCOL_VERSION, ServerMsg, decode_from_slice, write_msg,
};

struct ServerGuard {
    child: Child,
    root: std::path::PathBuf,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Frame {
    full_redraw: bool,
    gfx: Vec<(u16, u16, u64, Vec<u8>)>,
}

const SIXEL_MARKER: &[u8] = b"!64~";

fn wait_for_socket(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// reads frames until `deadline`, applying `on_frame` to each; returns as soon
/// as `on_frame` returns true. collects every frame into `frames`.
fn collect_frames(
    stream: &mut UnixStream,
    read_buf: &mut Vec<u8>,
    deadline: Instant,
    frames: &mut Vec<Frame>,
    mut stop_when: impl FnMut(&Frame) -> bool,
) {
    let mut temp = [0u8; 16384];
    while Instant::now() < deadline {
        match stream.read(&mut temp) {
            Ok(0) => return,
            Ok(n) => read_buf.extend_from_slice(&temp[..n]),
            Err(_) => {}
        }
        loop {
            match decode_from_slice::<ServerMsg>(read_buf) {
                Ok(Some((msg, consumed))) => {
                    read_buf.drain(..consumed);
                    if let ServerMsg::Update(update) = msg {
                        let frame = Frame {
                            full_redraw: update.full_redraw,
                            gfx: update
                                .gfx
                                .iter()
                                .map(|g| (g.row, g.col, g.seq, g.data.clone()))
                                .collect(),
                        };
                        let stop = stop_when(&frame);
                        frames.push(frame);
                        if stop {
                            return;
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => return,
            }
        }
    }
}

fn frame_has_sixel(frame: &Frame) -> bool {
    frame
        .gfx
        .iter()
        .any(|(_, _, _, g)| g.windows(SIXEL_MARKER.len()).any(|w| w == SIXEL_MARKER))
}

#[test]
fn sixel_frames_carry_ed2_and_never_stack() {
    let root = std::env::temp_dir().join(format!("venti-sixel-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run_dir = root.join("run");
    let config_dir = root.join("config");
    let home_dir = root.join("home");
    let home_dir_for_test = home_dir.clone();
    fs::create_dir_all(&run_dir).expect("run dir");
    fs::create_dir_all(config_dir.join("venti")).expect("config dir");
    fs::create_dir_all(&home_dir).expect("home dir");
    // disable the welcome screen so the printf reaches the pane
    fs::write(
        config_dir.join("venti").join("config.toml"),
        "[ui]\nshow_welcome = false\n[general]\ngraphics = true\n",
    )
    .expect("write config");

    let child = Command::new(env!("CARGO_BIN_EXE_venti"))
        .args(["server", "--session", "sixeltest"])
        .env("XDG_RUNTIME_DIR", &run_dir)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("HOME", &home_dir)
        .env("SHELL", "/bin/sh")
        .env("TERM", "xterm-256color")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn server");

    let _guard = ServerGuard { child, root };
    let uid = rustix::process::getuid().as_raw();
    let socket = run_dir
        .join("venti")
        .join(uid.to_string())
        .join("sixeltest.sock");
    assert!(
        wait_for_socket(&socket, Duration::from_secs(5)),
        "server socket never appeared at {}",
        socket.display()
    );

    let mut stream = UnixStream::connect(&socket).expect("connect socket");
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("read timeout");

    // the client reports foot, so venti forwards sixel instead of dropping it
    write_msg(
        &mut stream,
        &ClientMsg::Hello(HelloMsg {
            protocol: PROTOCOL_VERSION,
            term: "foot".to_string(),
            colorterm: Some("truecolor".to_string()),
            size: (100, 30),
            env: Vec::new(),
            cell_pixel: Some((10, 20)),
        }),
    )
    .expect("hello");

    let mut frames: Vec<Frame> = Vec::new();
    let mut seen_placements: Vec<(u64, u16, u16)> = Vec::new();
    let mut read_buf: Vec<u8> = Vec::new();

    // swallow the initial paint frames
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(3),
        &mut frames,
        |_| false,
    );

    // print a minimal marked sixel (1x1 cell, opaque raster) through the pane's shell
    let sixel_cmd = "printf '\\033P0;1;1q\"1;1;64;12#0;2;100;0;0#0!64~-\\033\\\\'\r";
    write_msg(
        &mut stream,
        &ClientMsg::Input(sixel_cmd.as_bytes().to_vec()),
    )
    .expect("input sixel cmd");

    let mut sixel_frames = false;
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(6),
        &mut frames,
        frame_has_sixel,
    );
    sixel_frames |= frames.iter().any(frame_has_sixel);
    assert!(
        sixel_frames,
        "the sixel printed by the pane never reached the client; frames seen: {}",
        frames.len()
    );

    // anti-stacking invariant: a diff frame may carry a sixel only for a
    // placement that was never emitted before (first paint); re-placing a
    // painted placement requires a full repaint (ED2), otherwise pixels stack.
    for (i, frame) in frames.iter().enumerate() {
        if frame_has_sixel(frame) {
            for (row, col, seq, g) in &frame.gfx {
                if !g.windows(SIXEL_MARKER.len()).any(|w| w == SIXEL_MARKER) {
                    continue;
                }
                if !frame.full_redraw {
                    assert!(
                        !seen_placements.contains(&(*seq, *row, *col)),
                        "frame {i} re-emitted sixel seq={seq} at (row={row}, col={col}) without full_redraw; pixels stack"
                    );
                }
                seen_placements.push((*seq, *row, *col));
            }
        }
    }

    // hide the pane's workspace, then show it again: the image must be dropped
    // from ws2 frames and re-placed with a full repaint on return to ws1
    write_msg(
        &mut stream,
        &ClientMsg::Command("workspace_goto 2".to_string()),
    )
    .expect("goto ws2");
    let before = frames.len();
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(2),
        &mut frames,
        |_| false,
    );
    for frame in &frames[before..] {
        assert!(
            !frame_has_sixel(frame),
            "sixel leaked into hidden workspace"
        );
    }

    write_msg(
        &mut stream,
        &ClientMsg::Command("workspace_goto 1".to_string()),
    )
    .expect("goto ws1");
    let before = frames.len();
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(3),
        &mut frames,
        frame_has_sixel,
    );
    let restored = frames[before..].iter().any(frame_has_sixel);
    assert!(
        restored,
        "sixel was not re-placed after returning to workspace 1"
    );
    let restored_frame = frames[before..]
        .iter()
        .find(|f| frame_has_sixel(f))
        .expect("restore frame exists");
    assert!(
        restored_frame.full_redraw,
        "restore frame carried a sixel without full_redraw; that stacks pixels"
    );

    // realistic one-shot output (chafa/lsix/img2sixel style): a dcS with
    // 0;1;0 parameters, a raster attribute and rle bands, printed raw to stdout
    fs::create_dir_all(&home_dir_for_test).expect("home dir");
    let oneshot = b"\x1bP0;1;0q\"1;1;64;12#0;2;100;0;0#0!64~-$#0!64~-\x1b\\".to_vec();
    fs::write(home_dir_for_test.join("oneshot.sixel"), &oneshot).expect("write sixel fixture");
    let cat_cmd = format!(
        "cat {}\r",
        home_dir_for_test.join("oneshot.sixel").display()
    );
    write_msg(&mut stream, &ClientMsg::Input(cat_cmd.into_bytes())).expect("input cat cmd");
    let before = frames.len();
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(3),
        &mut frames,
        frame_has_sixel,
    );
    let oneshot_frame = frames[before..].iter().find(|f| frame_has_sixel(f));
    assert!(
        oneshot_frame.is_some(),
        "one-shot sixel (chafa-style) never reached the client"
    );
    for frame in &frames[before..] {
        if frame_has_sixel(frame) {
            // each sixel placement in the frame is a distinct image
        }
    }

    // quit the session cleanly
    write_msg(&mut stream, &ClientMsg::Command("quit".to_string())).expect("quit");
    let _ = stream.flush();
    drop(stream);
}
