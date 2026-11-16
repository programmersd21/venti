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

fn read_screen(
    stream: &mut UnixStream,
    cells: &mut HashMap<(u16, u16), String>,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    let mut temp = [0u8; 8192];
    let mut read_buf: Vec<u8> = Vec::new();
    while Instant::now() < deadline {
        match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(n) => read_buf.extend_from_slice(&temp[..n]),
            Err(_) => {}
        }
        loop {
            match decode_from_slice::<ServerMsg>(&read_buf) {
                Ok(Some((msg, consumed))) => {
                    read_buf.drain(..consumed);
                    if let ServerMsg::Update(update) = msg {
                        for cell in update.cells {
                            cells.insert((cell.x, cell.y), cell.symbol);
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => return,
            }
        }
        // drain whatever is pending and keep the accumulated screen
        std::thread::sleep(Duration::from_millis(50));
        if read_buf.is_empty() {
            return;
        }
    }
}

#[test]
fn colon_cat_command_shows_a_blinking_cat() {
    let root = std::env::temp_dir().join(format!("venti-cat-itest-{}", std::process::id()));
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
        .args(["server", "--session", "cat-itest"])
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
        .join("cat-itest.sock");

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

    let mut cells: HashMap<(u16, u16), String> = HashMap::new();
    read_screen(&mut stream, &mut cells, Duration::from_secs(2));

    write_msg(&mut stream, &ClientMsg::Input(b":cat\r".to_vec())).expect("write :cat");

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        read_screen(&mut stream, &mut cells, Duration::from_millis(500));
        let screen = screen_rows(&cells);
        if screen.contains("/\\_/\\") && screen.contains("( o.o )") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cat never appeared; screen:\n{screen}"
        );
    }

    write_msg(&mut stream, &ClientMsg::Command("quit".to_string())).expect("write quit");
    let _ = stream.flush();
    drop(stream);
}
