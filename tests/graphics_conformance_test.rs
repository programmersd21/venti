//! conformance test for the image protocols a terminal multiplexer must
//! survive: kitty graphics (APC) and sixel (DCS).
//!
//! the rules come from the protocol specs:
//!   kitty: `ESC _ G <control> ; <payload> ESC \` (also terminated by ST),
//!          payload base64, chunked with `m=1` / `m=0`
//!   sixel: `ESC P q <data> ESC \`
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

use base64::Engine;
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
    gfx: Vec<(u16, u16, Vec<u8>)>,
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
                            gfx: update
                                .gfx
                                .iter()
                                .map(|g| (g.col, g.row, g.data.clone()))
                                .collect(),
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

fn kitty_rgb_payload(data: &[u8]) -> Option<Vec<u8>> {
    let mut rest = data;
    let mut encoded = Vec::new();
    let mut active = false;
    let mut complete = false;
    while let Some(start) = rest.windows(3).position(|window| window == b"\x1b_G") {
        let body = &rest[start + 3..];
        let end = body.windows(2).position(|window| window == b"\x1b\\")?;
        let packet = &body[..end];
        let Some(separator) = packet.iter().position(|byte| *byte == b';') else {
            rest = &rest[start + 3 + end + 2..];
            continue;
        };
        let control = std::str::from_utf8(&packet[..separator]).ok()?;
        if control
            .split(',')
            .any(|field| field == "a=T" && control.contains("f=32"))
        {
            active = true;
        }
        if active {
            encoded.extend_from_slice(&packet[separator + 1..]);
            if control.split(',').any(|field| field == "m=0") {
                complete = true;
                break;
            }
        }
        rest = &rest[start + 3 + end + 2..];
    }
    complete
        .then(|| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
        })
        .flatten()
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
}

impl Harness {
    fn start(term: &str, name: &str) -> Harness {
        Self::start_with_term_program(term, None, name)
    }

    fn start_with_term_program(term: &str, term_program: Option<&str>, name: &str) -> Harness {
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
                env: term_program
                    .map(|value| vec![("TERM_PROGRAM".to_string(), value.to_string())])
                    .unwrap_or_default(),
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
            .flat_map(|frame| frame.gfx.iter().map(|(_, _, data)| data.clone()))
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

#[test]
fn chafa_kitty_output_survives_the_pane_pipeline() {
    if !tool_exists("chafa") {
        return;
    }
    let mut h = Harness::start_with_term_program("xterm-256color", Some("kitty"), "kitty");
    let path = image_path("kitty");
    h.run_until(
        &format!("chafa -f kitty --size 20x10 {path}"),
        Duration::from_secs(1),
        |h| {
            h.all_gfx()
                .iter()
                .any(|data| data.windows(3).any(|w| w == b"\x1b_G"))
        },
        Duration::from_secs(25),
    );
    let image = h.all_gfx().iter().find_map(|data| kitty_rgb_payload(data));
    assert_eq!(
        image.as_ref().map(Vec::len),
        Some(160 * 160 * 4),
        "Chafa Kitty payload was not preserved as raw RGBA bytes"
    );
    assert!(
        !looks_like_payload_leak(&h.all_text()),
        "chafa Kitty payload leaked into terminal text"
    );

    let before_resize = h.frames.len();
    write_msg(&mut h.stream, &ClientMsg::Resize(80, 24)).expect("resize pane client");
    collect_frames(
        &mut h.stream,
        &mut h.read_buf,
        Instant::now() + Duration::from_secs(4),
        &mut h.frames,
    );
    assert!(
        h.frames[before_resize..]
            .iter()
            .flat_map(|frame| frame.gfx.iter())
            .any(|(_, _, data)| kitty_rgb_payload(data)
                .is_some_and(|bytes| bytes.len() == 160 * 160 * 4)),
        "Chafa image was not restored after resize"
    );
}

#[test]
fn fastfetch_kitty_logo_survives_the_pane_pipeline() {
    if !tool_exists("fastfetch") {
        return;
    }
    let mut h = Harness::start_with_term_program("xterm-256color", Some("kitty"), "fastfetch");
    let path = image_path("fastfetch");
    h.run_until(
        &format!(
            "fastfetch --kitty {path} --logo-width 20 --logo-height 10 --pipe false --show-errors"
        ),
        Duration::from_secs(1),
        |h| {
            h.all_gfx()
                .iter()
                .any(|data| data.windows(3).any(|w| w == b"\x1b_G"))
        },
        Duration::from_secs(25),
    );
    assert!(
        h.all_gfx()
            .iter()
            .any(|data| data.windows(3).any(|w| w == b"\x1b_G")),
        "fastfetch Kitty logo did not reach the graphics stream; terminal env was xterm-256color/kitty"
    );
}

#[test]
fn kitten_icat_gif_forwards_animation_frames() {
    if !tool_exists("kitten") || !tool_exists("magick") {
        return;
    }
    let path = std::env::temp_dir().join(format!("venti-animation-{}.gif", std::process::id()));
    let status = Command::new("magick")
        .args([
            "-size", "8x8", "xc:red", "-delay", "10", "-size", "8x8", "xc:blue", "-delay", "10",
            "-loop", "1",
        ])
        .arg(&path)
        .status()
        .expect("create animated gif");
    assert!(status.success(), "ImageMagick failed to create test gif");

    let mut h = Harness::start_with_term_program("xterm-256color", Some("kitty"), "icat-gif");
    h.run_until(
        &format!("kitten icat --loop 1 {}", path.display()),
        Duration::from_secs(1),
        |h| {
            h.all_gfx()
                .iter()
                .any(|data| data.windows(3).any(|window| window == b"\x1b_G"))
        },
        Duration::from_secs(20),
    );
    std::thread::sleep(Duration::from_millis(300));
    collect_frames(
        &mut h.stream,
        &mut h.read_buf,
        Instant::now() + Duration::from_secs(1),
        &mut h.frames,
    );
    let commands = h.all_gfx();
    assert!(
        commands
            .iter()
            .any(|data| data.windows(4).any(|window| window == b"a=f,")),
        "kitten icat did not send animation frame data"
    );
    assert!(
        commands
            .iter()
            .any(|data| data.windows(4).any(|window| window == b"a=a,")),
        "kitten icat did not start terminal animation playback"
    );
    let _ = fs::remove_file(path);
}
