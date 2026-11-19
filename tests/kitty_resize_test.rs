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
    ClientMsg, HelloMsg, PROTOCOL_VERSION, ProtoColor, ServerMsg, decode_from_slice, write_msg,
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
    gfx: Vec<Vec<u8>>,
    cells: Vec<venti::proto::CellDiff>,
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
                            cells: update.cells,
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

fn frame_has_kitty(frame: &Frame) -> bool {
    frame
        .gfx
        .iter()
        .any(|d| d.windows(3).any(|w| w == b"\x1b_G"))
}

fn frame_has_indexed_placeholder_ids(frame: &Frame) -> bool {
    frame.cells.iter().any(|cell| {
        cell.symbol.starts_with('\u{10eeee}')
            && cell.fg != ProtoColor::Indexed(7)
            && cell.underline_color == ProtoColor::Default
    })
}

#[test]
fn kitty_image_reappears_after_resize() {
    let root = std::env::temp_dir().join(format!("venti-kitty-resize-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run_dir = root.join("run");
    let config_dir = root.join("config");
    let home_dir = root.join("home");
    fs::create_dir_all(&run_dir).expect("run dir");
    fs::create_dir_all(config_dir.join("venti")).expect("config dir");
    fs::create_dir_all(&home_dir).expect("home dir");
    fs::write(
        home_dir.join("placeholder.sh"),
        r#"printf '\033_Ga=T,f=100,s=1,v=1,i=7,p=5,U=1,c=1,r=1,q=2;iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==\033\\'; printf '\033[38;5;7m\033[58;5;5m\364\216\273\256\314\205\314\205\033[39m\n'"#,
    )
    .expect("write placeholder script");
    fs::write(
        config_dir.join("venti").join("config.toml"),
        "[ui]\nshow_welcome = false\n[general]\ngraphics = true\n",
    )
    .expect("write config");

    let child = Command::new(env!("CARGO_BIN_EXE_venti"))
        .args(["server", "--session", "kittyresizetest"])
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
        .join("kittyresizetest.sock");
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
        &|_| false,
    );

    let payload = b"printf '\\033_Ga=T,f=100,s=1,v=1,i=7;iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==\\033\\\\'\r";
    write_msg(&mut stream, &ClientMsg::Input(payload.to_vec())).expect("input image");
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(4),
        &mut frames,
        &frame_has_kitty,
    );
    assert!(
        frames.iter().any(frame_has_kitty),
        "the kitty image never reached the client"
    );

    write_msg(
        &mut stream,
        &ClientMsg::Input(b"sh \"$HOME/placeholder.sh\"\r".to_vec()),
    )
    .expect("input placeholder");
    let before_placeholder = frames.len();
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(4),
        &mut frames,
        &frame_has_indexed_placeholder_ids,
    );
    assert!(
        frames[before_placeholder..]
            .iter()
            .any(frame_has_indexed_placeholder_ids),
        "low indexed image or placement ids did not survive the complete screen pipeline"
    );
    assert!(
        frames[before_placeholder..].iter().any(|frame| {
            frame
                .gfx
                .iter()
                .any(|data| data.windows(3).any(|window| window == b"U=1"))
        }),
        "the virtual image placement was not transmitted to the client: {:?}",
        frames[before_placeholder..]
            .iter()
            .flat_map(|frame| frame.gfx.iter())
            .map(|data| String::from_utf8_lossy(data).into_owned())
            .collect::<Vec<_>>()
    );

    // resize the client: the vt100 reflows, ED2 wipes the placement, and the
    // image must be placed again on the host afterwards
    write_msg(&mut stream, &ClientMsg::Resize(88, 22)).expect("resize");
    let before = frames.len();
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(4),
        &mut frames,
        &frame_has_kitty,
    );
    assert!(
        frames[before..].iter().any(frame_has_kitty),
        "the kitty image did not come back after resize; frames after resize: {:?}",
        frames[before..]
            .iter()
            .map(|f| f.gfx.len())
            .collect::<Vec<_>>(),
    );
}
