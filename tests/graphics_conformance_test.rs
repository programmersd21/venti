//! conformance test for the image protocols a terminal multiplexer must
//! survive: kitty graphics (APC), sixel (DCS) and iTerm2 inline images (OSC).
//!
//! the rules come from the protocol specs:
//!   kitty: `ESC _ G <control> ; <payload> ESC \` (also terminated by ST),
//!          payload base64, chunked with `m=1` / `m=0`
//!   sixel: `ESC P q <data> ESC \`
//!   iterm2: `ESC ] 1337 ; File=... BEL`
//!
//! the important property for a multiplexer: the payload must never end up
//! in the text stream. if it does, the terminal prints the image data as
//! garbage ("AAAAAAAA..." filling the screen).
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
        if !std::env::var("VENTI_KEEP_TEST_DIR").is_ok() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

struct Frame {
    cells: Vec<String>,
    gfx: Vec<Vec<u8>>,
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
                        frames.push(Frame {
                            cells: update.cells.iter().map(|c| c.symbol.clone()).collect(),
                            gfx: update.gfx.iter().map(|g| g.data.clone()).collect(),
                        });
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }
}

/// the payload of a graphics sequence is base64; if it leaks into the text
/// stream the terminal shows long runs of base64 characters.
fn looks_like_payload_leak(text: &str) -> bool {
    let mut run = 0usize;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '+' || ch == '/' || ch == '=' {
            run += 1;
            if run >= 40 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn tool_exists(tool: &str) -> bool {
    Command::new(tool)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

struct Harness {
    stream: UnixStream,
    _guard: ServerGuard,
    read_buf: Vec<u8>,
    frames: Vec<Frame>,
    session: String,
}

impl Harness {
    fn start(term: &str, name: &str) -> Harness {
        // the session name and the state directory must be unique per test:
        // the tests run in parallel and a shared session would let one
        // server replace the other one.
        let root = std::env::temp_dir().join(format!("venti-conf-{name}-{}", std::process::id()));
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
        let session = format!("conf{name}-{}", std::process::id());
        let child = Command::new(env!("CARGO_BIN_EXE_venti"))
            .args(["server", "--session", &session])
            .env("XDG_RUNTIME_DIR", &run_dir)
            .env("XDG_CONFIG_HOME", &config_dir)
            .env("HOME", &home_dir)
            .env("SHELL", "/bin/sh")
            .env("TERM", term)
            .env("RUST_LOG", "warn")
            .env("XDG_STATE_HOME", root.join("state"))
            .env("RUST_LOG", "warn")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn server");
        let guard = ServerGuard { child, root };
        let uid = rustix::process::getuid().as_raw();
        let socket = run_dir
            .join("venti")
            .join(uid.to_string())
            .join(format!("{session}.sock"));
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
                term: term.to_string(),
                colorterm: Some("truecolor".to_string()),
                size: (100, 30),
                env: Vec::new(),
                cell_pixel: Some((10, 20)),
            }),
        )
        .expect("hello");
        let mut frames = Vec::new();
        let mut read_buf = Vec::new();
        collect_frames(
            &mut stream,
            &mut read_buf,
            Instant::now() + Duration::from_secs(2),
            &mut frames,
        );
        Harness {
            stream,
            _guard: guard,
            read_buf,
            frames,
            session,
        }
    }

    /// keeps collecting frames until `ready` returns true or `timeout` is
    /// reached. the daemon renders on its own schedule, so a fixed window
    /// makes the test flaky under load.
    fn run_until(
        &mut self,
        command: &str,
        wait: Duration,
        ready: impl Fn(&Self) -> bool,
        timeout: Duration,
    ) {
        write_msg(
            &mut self.stream,
            &ClientMsg::Input(format!("{command}\r").as_bytes().to_vec()),
        )
        .expect("input");
        let deadline = Instant::now() + timeout;
        loop {
            collect_frames(
                &mut self.stream,
                &mut self.read_buf,
                Instant::now() + wait,
                &mut self.frames,
            );
            if ready(self) || Instant::now() >= deadline {
                break;
            }
        }
    }

    /// every text cell the client ever received
    fn all_text(&self) -> String {
        self.frames
            .iter()
            .flat_map(|frame| frame.cells.clone())
            .collect::<Vec<_>>()
            .join("")
    }

    fn all_gfx(&self) -> Vec<Vec<u8>> {
        self.frames
            .iter()
            .flat_map(|frame| frame.gfx.clone())
            .collect::<Vec<_>>()
    }
}

/// builds a valid 16x16 rgb png (zlib stored blocks, correct crc32).
fn build_png() -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
                crc = (crc >> 1) ^ mask;
            }
        }
        !crc
    }
    fn chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        out
    }
    let width = 16u32;
    let height = 16u32;
    let mut raw = Vec::new();
    for y in 0..height {
        raw.push(0u8);
        for x in 0..width {
            raw.push((x * 16) as u8);
            raw.push((y * 16) as u8);
            raw.push(128u8);
        }
    }
    // zlib stream with stored (uncompressed) deflate blocks
    let mut zlib = vec![0x78, 0x01];
    for block in raw.chunks(65535) {
        let last = if (block.as_ptr() as usize + block.len()) == (raw.as_ptr() as usize + raw.len())
        {
            1u8
        } else {
            0u8
        };
        zlib.push(last);
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    let mut adler_a = 1u32;
    let mut adler_b = 0u32;
    for &byte in &raw {
        adler_a = (adler_a + u32::from(byte)) % 65521;
        adler_b = (adler_b + adler_a) % 65521;
    }
    zlib.extend_from_slice(&((adler_b << 16) | adler_a).to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    png.extend_from_slice(&chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&chunk(b"IDAT", &zlib));
    png.extend_from_slice(&chunk(b"IEND", &[]));
    png
}

fn image_path(name: &str) -> String {
    // unique per test: the tests run in parallel and a shared file would be
    // truncated by one test while the other is still reading it.
    let path = std::env::temp_dir().join(format!("venti-conf-image-{name}.png"));
    fs::write(&path, build_png()).expect("write png");
    path.to_string_lossy().into_owned()
}

#[test]
fn kitty_graphics_payload_never_reaches_the_text_stream() {
    let mut h = Harness::start("xterm-kitty", "kitty");
    if !tool_exists("kitten") {
        eprintln!("kitten not available, skipping");
        return;
    }
    let path = image_path("kitty");
    h.run_until(
        &format!("kitten icat --align left {path}"),
        Duration::from_secs(1),
        |h| {
            h.all_gfx()
                .iter()
                .any(|data| data.windows(4).any(|w| w == b"_Ga="))
        },
        Duration::from_secs(25),
    );
    let text = h.all_text();
    assert!(
        !looks_like_payload_leak(&text),
        "the kitty payload leaked into the text stream"
    );
    let gfx = h.all_gfx();
    assert!(
        gfx.iter().any(|data| data.windows(4).any(|w| w == b"_Ga=")),
        "the kitty image never reached the client"
    );
    // every kitty sequence must be terminated: a truncated APC leaves the
    // payload in the text stream
    for data in &gfx {
        let text = String::from_utf8_lossy(data);
        if text.contains("_Ga=") {
            assert!(
                text.ends_with("\x1b\\") || text.ends_with('\u{9c}'),
                "unterminated kitty sequence: {text:?}"
            );
        }
    }
    // the kitty protocol caps a single apc payload at 4096 base64 bytes and
    // requires m=1/m=0 chunking. a host silently drops oversized apcs, so
    // every re-transmitted chunk venti emits must obey the limit.
    for data in &gfx {
        let mut rest = &data[..];
        while let Some(p) = rest.windows(3).position(|w| w == b"\x1b_G") {
            let after = &rest[p + 3..];
            let end = after
                .windows(2)
                .position(|w| w == b"\x1b\\")
                .map(|e| p + 3 + e)
                .unwrap_or(rest.len());
            let apc = &rest[p..end];
            if let Some(semi) = apc.iter().position(|&b| b == b';') {
                let payload_len = apc.len() - semi - 1;
                assert!(
                    payload_len <= 4096,
                    "a kitty chunk carried {payload_len} bytes, the protocol limit is 4096"
                );
            }
            rest = &rest[end + 2..];
        }
    }
    let _ = h.session;
}

#[test]
fn sixel_payload_never_reaches_the_text_stream() {
    let mut h = Harness::start("foot", "sixel");
    if !tool_exists("chafa") {
        eprintln!("chafa not available, skipping");
        return;
    }
    let path = image_path("sixel");
    h.run_until(
        &format!("chafa -f sixels --size 20x10 {path}"),
        Duration::from_secs(1),
        |h| {
            h.all_gfx()
                .iter()
                .any(|data| data.starts_with(b"\x1bP") && data.contains(&b'q'))
        },
        Duration::from_secs(25),
    );
    let text = h.all_text();
    assert!(
        !looks_like_payload_leak(&text),
        "the sixel payload leaked into the text stream"
    );
    let gfx = h.all_gfx();
    assert!(
        gfx.iter()
            .any(|data| data.starts_with(b"\x1bP") && data.contains(&b'q')),
        "the sixel image never reached the client"
    );
    for data in &gfx {
        if data.starts_with(b"\x1bP") {
            assert!(
                data.ends_with(b"\x1b\\") || data.ends_with(&[0x9c]),
                "unterminated sixel sequence"
            );
        }
    }
}
