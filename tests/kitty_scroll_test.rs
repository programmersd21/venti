//! regression: icat-style kitty images must track scrolled content.
//!
//! when the pane's content scrolls (the shell prints more lines than its
//! surface), an anchored kitty image must be re-placed one row per scrolled
//! line, otherwise the image stays pinned at its original cell while the
//! text scrolls away and appears to vanish (the "kitty image goes away on
//! scroll" report).
use std::fs;
use std::io::Read;
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
    #[allow(dead_code)]
    full_redraw: bool,
    gfx: Vec<(u16, u16, u64, Vec<u8>)>,
}

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

fn collect_frames(
    stream: &mut UnixStream,
    read_buf: &mut Vec<u8>,
    deadline: Instant,
    frames: &mut Vec<Frame>,
    mut stop_when: impl FnMut(&Frame) -> bool,
) -> bool {
    let mut stopped = false;
    let mut temp = [0u8; 16384];
    while Instant::now() < deadline {
        match stream.read(&mut temp) {
            Ok(0) => return stopped,
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
                        if stop_when(&frame) {
                            stopped = true;
                        }
                        frames.push(frame);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        if stopped {
            return true;
        }
    }
    stopped
}

fn all_kitty_placements(frames: &[Frame]) -> Vec<(u16, u16, Vec<u8>)> {
    frames
        .iter()
        .flat_map(|f| f.gfx.iter().map(|(r, c, _, d)| (*r, *c, d.clone())))
        .filter(|(_, _, d)| d.windows(3).any(|w| w == b"\x1b_G"))
        .collect()
}

#[test]
fn kitty_image_tracks_content_when_terminal_scrolls() {
    let root = std::env::temp_dir().join(format!("venti-kitty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run_dir = root.join("run");
    let config_dir = root.join("config");
    let home_dir = root.join("home");
    fs::create_dir_all(&run_dir).expect("run dir");
    fs::create_dir_all(config_dir.join("venti")).expect("config dir");
    fs::create_dir_all(&home_dir).expect("home dir");
    fs::write(
        config_dir.join("venti").join("config.toml"),
        "[ui]\nshow_welcome = false\n[general]\ngraphics = true\n",
    )
    .expect("write config");

    let child = Command::new(env!("CARGO_BIN_EXE_venti"))
        .args(["server", "--session", "kittyscrolltest"])
        .env("XDG_RUNTIME_DIR", &run_dir)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("HOME", &home_dir)
        .env("SHELL", "/bin/sh")
        .env("TERM", "xterm-kitty")
        .env("RUST_LOG", "info")
        .env("XDG_STATE_HOME", "/tmp/opencode/venti-state")
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
        .join("kittyscrolltest.sock");
    assert!(
        wait_for_socket(&socket, Duration::from_secs(5)),
        "server socket never appeared at {}",
        socket.display()
    );

    let mut stream = UnixStream::connect(&socket).expect("connect socket");
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("read timeout");

    write_msg(
        &mut stream,
        &ClientMsg::Hello(HelloMsg {
            protocol: PROTOCOL_VERSION,
            term: "xterm-kitty".to_string(),
            colorterm: Some("truecolor".to_string()),
            size: (100, 30),
            env: Vec::new(),
            cell_pixel: Some((10, 20)),
        }),
    )
    .expect("hello");

    let mut frames: Vec<Frame> = Vec::new();
    let mut read_buf: Vec<u8> = Vec::new();
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(3),
        &mut frames,
        |_| false,
    );

    // print a minimal 1x1 PNG as a kitty direct (a=T) image at the cursor
    let payload = b"printf '\\033_Ga=T,f=100,s=1,v=1,i=7;iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==\\033\\\\'\r";
    write_msg(&mut stream, &ClientMsg::Input(payload.to_vec())).expect("input image");

    let mut image_row = None;
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(4),
        &mut frames,
        |frame| {
            for (row, _col, _seq, data) in &frame.gfx {
                if data.windows(3).any(|w| w == b"\x1b_G") {
                    image_row = Some(*row);
                    return true;
                }
            }
            false
        },
    );
    let _first_row = image_row.expect("the kitty image never reached the client");

    // push enough output to scroll the pane past the image's row so it
    // leaves the viewport entirely
    let filler = "for i in $(seq 1 80); do printf 'scroll-line-%02d\n' $i; done\r";
    write_msg(&mut stream, &ClientMsg::Input(filler.as_bytes().to_vec())).expect("input filler");

    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(5),
        &mut frames,
        |_| false,
    );

    // scroll back up through the scrollback with the wheel; the image must be
    // re-placed once its anchored rows come back into view
    for _ in 0..40 {
        write_msg(
            &mut stream,
            &ClientMsg::Mouse(venti::proto::MouseEventMsg {
                col: 80,
                row: 10,
                button: venti::proto::MouseButtonKind::ScrollUp,
                action: venti::proto::MouseActionKind::Down,
                shift: false,
                alt: false,
                ctrl: false,
            }),
        )
        .expect("wheel up");
        std::thread::sleep(Duration::from_millis(80));
    }

    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(3),
        &mut frames,
        |_| false,
    );

    let placements = all_kitty_placements(&frames);
    let replaces = placements
        .iter()
        .filter(|(_, _, d)| d.windows(4).any(|w| w == b"a=p,"))
        .count();
    assert!(
        replaces >= 1,
        "the kitty image was never re-placed after scroll-back into view; placements seen: {placements:?}"
    );

    // The previous assertion only proved recovery from fully hidden state.
    // A placement that remains wholly inside the viewport still has to move
    // when the scrollback viewport changes. In particular, this catches the
    // case where a one-cell icat image stays at its old host-terminal row.
    let last_visible_row = placements
        .iter()
        .rev()
        .find(|(_, _, data)| data.windows(4).any(|w| w == b"a=p,"))
        .map(|(row, _, _)| *row)
        .expect("a visible kitty placement was emitted");
    frames.clear();
    write_msg(
        &mut stream,
        &ClientMsg::Mouse(venti::proto::MouseEventMsg {
            col: 80,
            row: 10,
            button: venti::proto::MouseButtonKind::ScrollDown,
            action: venti::proto::MouseActionKind::Down,
            shift: false,
            alt: false,
            ctrl: false,
        }),
    )
    .expect("wheel down");
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(2),
        &mut frames,
        |_| false,
    );
    let moved_rows: Vec<_> = all_kitty_placements(&frames)
        .into_iter()
        .map(|(row, _, _)| row)
        .collect();
    assert!(
        moved_rows.iter().any(|row| *row != last_visible_row),
        "changing the scrollback viewport left the visible kitty placement at row {last_visible_row}; emitted rows: {moved_rows:?}"
    );
}
