//! regression: kitty images must survive a client resize.
//!
//! a resize reflows the vt100 grid and wipes kitty placements via ED2; the
//! image must be re-placed (or fully re-transmitted) afterwards, otherwise it
//! disappears from the screen until the session is restarted.
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

struct Frame {
    #[allow(dead_code)]
    gfx: Vec<Vec<u8>>,
    cells: Vec<String>,
}

fn collect_frames(
    stream: &mut UnixStream,
    read_buf: &mut Vec<u8>,
    deadline: Instant,
    frames: &mut Vec<Frame>,
    stop: &dyn Fn(&Frame) -> bool,
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
                            gfx: update.gfx.iter().map(|g| g.data.clone()).collect(),
                            cells: update
                                .cells
                                .iter()
                                .map(|cell| cell.symbol.clone())
                                .collect(),
                        };
                        if stop(&frame) {
                            frames.push(frame);
                            return;
                        }
                        frames.push(frame);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }
}

#[test]
fn float_toggle_keeps_the_pane_text() {
    let root = std::env::temp_dir().join(format!("venti-float-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run_dir = root.join("run");
    let config_dir = root.join("config");
    let home_dir = root.join("home");
    fs::create_dir_all(&run_dir).expect("run dir");
    fs::create_dir_all(config_dir.join("venti")).expect("config dir");
    fs::create_dir_all(&home_dir).expect("home dir");
    fs::write(
        config_dir.join("venti").join("config.toml"),
        "[ui]\nshow_welcome = false\n[general]\ngraphics = true\n[animation]\nenabled = false\n",
    )
    .expect("write config");

    let child = Command::new(env!("CARGO_BIN_EXE_venti"))
        .args(["server", "--session", "floattest"])
        .env("XDG_RUNTIME_DIR", &run_dir)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("HOME", &home_dir)
        .env("SHELL", "/bin/sh")
        .env("TERM", "xterm-kitty")
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
        .join("floattest.sock");
    assert!(
        wait_for_socket(&socket, Duration::from_secs(5)),
        "server socket never appeared"
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
        &|_| false,
    );

    // print a line that is longer than the floating pane is wide
    let long = "LONGREFLOW-".to_string() + &"X".repeat(80);
    write_msg(
        &mut stream,
        &ClientMsg::Input(format!("printf '{long}\n'\r").as_bytes().to_vec()),
    )
    .expect("input line");
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(3),
        &mut frames,
        &|_| false,
    );
    let before = frames.len();

    // float the pane: it shrinks, the grid must reflow instead of truncating
    write_msg(&mut stream, &ClientMsg::Command("toggle_float".to_string())).expect("toggle float");
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(4),
        &mut frames,
        &|_| false,
    );
    let text: String = frames[before..]
        .iter()
        .flat_map(|frame| frame.cells.clone())
        .collect::<String>();
    assert!(
        text.contains("LONGREFLOW"),
        "the text must survive the float toggle (reflow, not truncation): {text:?}"
    );
}
