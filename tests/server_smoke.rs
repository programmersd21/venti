use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use venti::proto::{
    ClientMsg, HelloMsg, PROTOCOL_VERSION, ServerMsg, decode_from_slice, write_msg,
};

struct ServerGuard {
    child: Child,
    root: PathBuf,
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

fn screen_rows(cells: &HashMap<(u16, u16), String>) -> String {
    let max_y = cells.keys().map(|(_, y)| *y).max().unwrap_or(0);
    let mut out = String::new();
    for y in 0..=max_y {
        let max_x = cells
            .keys()
            .filter(|(_, cy)| *cy == y)
            .map(|(x, _)| *x)
            .max()
            .unwrap_or(0);
        for x in 0..=max_x {
            out.push_str(cells.get(&(x, y)).map(String::as_str).unwrap_or(" "));
        }
        out.push('\n');
    }
    out
}

#[test]
fn daemon_attaches_runs_commands_and_kills() {
    let root = std::env::temp_dir().join(format!("venti-itest-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run_dir = root.join("run");
    let config_dir = root.join("config");
    let home_dir = root.join("home");
    fs::create_dir_all(&run_dir).expect("run dir");
    fs::create_dir_all(&config_dir).expect("config dir");
    fs::create_dir_all(&home_dir).expect("home dir");
    // disable the welcome screen so the first keypress is not swallowed by it
    let venti_config = config_dir.join("venti");
    fs::create_dir_all(&venti_config).expect("config subdir");
    fs::write(
        venti_config.join("config.toml"),
        "[ui]\nshow_welcome = false\n[general]\ngraphics = true\n",
    )
    .expect("write config");

    let child = Command::new(env!("CARGO_BIN_EXE_venti"))
        .args(["server", "--session", "itest"])
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
        .join("itest.sock");

    assert!(
        wait_for_socket(&socket, Duration::from_secs(5)),
        "server socket never appeared at {}",
        socket.display()
    );

    let mut stream = UnixStream::connect(&socket).expect("connect socket");
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("read timeout");

    let hello = ClientMsg::Hello(HelloMsg {
        protocol: PROTOCOL_VERSION,
        term: "xterm-256color".to_string(),
        colorterm: Some("truecolor".to_string()),
        size: (100, 30),
        env: Vec::new(),
        cell_pixel: None,
    });
    write_msg(&mut stream, &hello).expect("write hello");

    let mut read_buf: Vec<u8> = Vec::new();
    let mut temp = [0u8; 8192];
    let mut cells: HashMap<(u16, u16), String> = HashMap::new();
    let mut sent_at: Option<Instant> = None;
    let deadline = Instant::now() + Duration::from_secs(15);

    loop {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for command output; screen:\n{}",
            screen_rows(&cells)
        );

        if sent_at.is_none() {
            std::thread::sleep(Duration::from_millis(300));
            write_msg(
                &mut stream,
                &ClientMsg::Input(b"echo MARKER-9988\r".to_vec()),
            )
            .expect("write input");
            sent_at = Some(Instant::now());
        }

        match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(n) => read_buf.extend_from_slice(&temp[..n]),
            Err(_) => {}
        }

        loop {
            match decode_from_slice::<ServerMsg>(&read_buf) {
                Ok(Some((msg, consumed))) => {
                    read_buf.drain(..consumed);
                    match msg {
                        ServerMsg::HelloAck { .. } => {}
                        ServerMsg::Update(update) => {
                            for cell in update.cells {
                                cells.insert((cell.x, cell.y), cell.symbol);
                            }
                        }
                        ServerMsg::Exit(_) => panic!("server exited unexpectedly"),
                        _ => {}
                    }
                }
                Ok(None) => break,
                Err(err) => panic!("protocol decode error: {err}"),
            }
        }

        if screen_rows(&cells).contains("MARKER-9988") {
            break;
        }

        if let Some(sent) = sent_at
            && sent.elapsed() > Duration::from_secs(2)
        {
            write_msg(
                &mut stream,
                &ClientMsg::Input(b"echo MARKER-9988\r".to_vec()),
            )
            .expect("retry input");
            sent_at = Some(Instant::now());
        }
    }

    assert!(
        screen_rows(&cells).contains("MARKER-9988"),
        "marker missing from screen:\n{}",
        screen_rows(&cells)
    );

    write_msg(&mut stream, &ClientMsg::Command("quit".to_string())).expect("write quit");
    let _ = stream.flush();
    drop(stream);
}
