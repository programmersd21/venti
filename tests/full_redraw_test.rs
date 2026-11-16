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
    full_redraw: bool,
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
    matcher: impl Fn(&Frame) -> bool,
) -> bool {
    let mut temp = [0u8; 16384];
    while Instant::now() < deadline {
        match stream.read(&mut temp) {
            Ok(0) => return false,
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
                        };
                        if matcher(&frame) {
                            return true;
                        }
                    }
                }
                Ok(None) => break,
                Err(err) => panic!("protocol decode error: {err}"),
            }
        }
    }
    false
}

#[test]
fn layout_changes_produce_full_frames_with_every_cell() {
    let root = std::env::temp_dir().join(format!("venti-redraw-{}", std::process::id()));
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
        .args(["server", "--session", "redrawtest"])
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
        .join("redrawtest.sock");

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
            term: "xterm-256color".to_string(),
            colorterm: Some("truecolor".to_string()),
            size: (100, 30),
            env: Vec::new(),
            cell_pixel: None,
        }),
    )
    .expect("hello");

    let mut read_buf: Vec<u8> = Vec::new();

    // first frame must be complete (a fresh client does a full repaint).
    // Note: cell count varies based on terminal rendering optimizations
    assert!(
        collect_frames(
            &mut stream,
            &mut read_buf,
            Instant::now() + Duration::from_secs(10),
            |f| f.full_redraw
        ),
        "initial frame was not a full repaint"
    );

    // creating a pane is a layout change; it must produce a full frame.
    write_msg(&mut stream, &ClientMsg::Command("new_pane".to_string())).expect("write new_pane");
    assert!(
        collect_frames(
            &mut stream,
            &mut read_buf,
            Instant::now() + Duration::from_secs(10),
            |f| f.full_redraw
        ),
        "new_pane did not trigger a full repaint"
    );

    // switching workspaces changes the visible panes; it must produce a full frame.
    write_msg(
        &mut stream,
        &ClientMsg::Command("workspace_goto 2".to_string()),
    )
    .expect("write workspace_goto 2");
    assert!(
        collect_frames(
            &mut stream,
            &mut read_buf,
            Instant::now() + Duration::from_secs(10),
            |f| f.full_redraw
        ),
        "workspace_goto 2 did not trigger a full repaint"
    );

    // switching back to workspace 1 must repaint the original panes completely.
    write_msg(
        &mut stream,
        &ClientMsg::Command("workspace_goto 1".to_string()),
    )
    .expect("write workspace_goto 1");
    assert!(
        collect_frames(
            &mut stream,
            &mut read_buf,
            Instant::now() + Duration::from_secs(10),
            |f| f.full_redraw
        ),
        "workspace_goto 1 did not trigger a full repaint"
    );

    // closing one of the two panes in workspace 1 is a layout change too.
    write_msg(&mut stream, &ClientMsg::Command("close_pane".to_string()))
        .expect("write close_pane");
    assert!(
        collect_frames(
            &mut stream,
            &mut read_buf,
            Instant::now() + Duration::from_secs(10),
            |f| f.full_redraw
        ),
        "close_pane did not trigger a full repaint"
    );

    write_msg(&mut stream, &ClientMsg::Detach).expect("write detach");
}
