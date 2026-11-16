//! regression: a kitty image must stay on the row of the text it was printed
//! with, also after the pane is floated (the grid shrinks and re-wraps).
//!
//! the image used to stay on the old row: it was drawn over the pane border,
//! outside the pane, or twice.
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
    /// (row, col, symbol) of every cell in the frame
    cells: Vec<(u16, u16, String)>,
    /// (row, col) of every graphics placement
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
                            cells: update
                                .cells
                                .iter()
                                .map(|c| (c.y, c.x, c.symbol.clone()))
                                .collect(),
                            gfx: update
                                .gfx
                                .iter()
                                .map(|g| (g.row, g.col, g.data.clone()))
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

#[test]
fn a_floated_pane_keeps_the_image_on_the_text_row() {
    let root = std::env::temp_dir().join(format!("venti-floatimg-{}", std::process::id()));
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
        .args(["server", "--session", "floatimgtest"])
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
        .join("floatimgtest.sock");
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
        Instant::now() + Duration::from_secs(2),
        &mut frames,
    );

    // filler lines first, so the image sits near the bottom and stays
    // visible when the pane shrinks
    write_msg(
        &mut stream,
        &ClientMsg::Input(b"for i in $(seq 1 12); do echo FILLER-$i; done\r".to_vec()),
    )
    .expect("input filler");
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(2),
        &mut frames,
    );
    // print a marker line and a kitty image right below it
    write_msg(
        &mut stream,
        &ClientMsg::Input(
            b"printf 'MARKERROW\\n\\033_Ga=T,f=100,s=8,v=8,i=7;AAAA\\033\\\\\\n'\r".to_vec(),
        ),
    )
    .expect("input");
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(3),
        &mut frames,
    );

    // float the pane: the grid shrinks, the content moves
    write_msg(&mut stream, &ClientMsg::Command("toggle_float".to_string())).expect("toggle float");
    let before = frames.len();
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(4),
        &mut frames,
    );

    // rebuild the virtual screen from the cell diffs, then compare the
    // marker row with the row the image was placed on.
    let mut screen: std::collections::HashMap<(u16, u16), String> =
        std::collections::HashMap::new();
    let mut image_rows: Vec<u16> = Vec::new();
    for frame in &frames[before..] {
        for (y, x, symbol) in &frame.cells {
            screen.insert((*y, *x), symbol.clone());
        }
        for (row, _, data) in &frame.gfx {
            if data.windows(4).any(|w| w == b"a=p,") {
                image_rows.push(*row);
            }
        }
    }
    let marker_row = screen
        .iter()
        .find(|(_, symbol)| symbol.as_str() == "M")
        .map(|((y, _), _)| *y)
        .expect("the marker text is on screen");
    // scroll back: the image that scrolled into the history must come back
    // when it is visible again (it is deleted from the host while it is out
    // of view, so it needs a fresh placement).
    for _ in 0..12 {
        write_msg(
            &mut stream,
            &ClientMsg::Mouse(venti::proto::MouseEventMsg {
                col: 60,
                row: 12,
                button: venti::proto::MouseButtonKind::ScrollUp,
                action: venti::proto::MouseActionKind::Down,
                shift: false,
                alt: false,
                ctrl: false,
            }),
        )
        .expect("wheel up");
        std::thread::sleep(Duration::from_millis(60));
    }
    let before_scroll = frames.len();
    collect_frames(
        &mut stream,
        &mut read_buf,
        Instant::now() + Duration::from_secs(3),
        &mut frames,
    );
    let revealed: Vec<u16> = frames[before_scroll..]
        .iter()
        .flat_map(|frame| frame.gfx.iter())
        .filter(|(_, _, data)| data.windows(4).any(|w| w == b"a=p,"))
        .map(|(row, _, _)| *row)
        .collect();
    assert!(
        !revealed.is_empty(),
        "the image must be re-placed when it is scrolled back into view"
    );

    // every placement must sit inside the pane: an image drawn over the
    // border or outside the pane is the "image in the wrong place" bug.
    let pane_bottom = 30u16;
    for row in &image_rows {
        assert!(
            *row < pane_bottom,
            "the image was placed at row {row}, outside the pane (marker row {marker_row})"
        );
        if *row >= marker_row {
            assert_eq!(
                *row,
                marker_row + 1,
                "the image must stay directly under the text it was printed with \
                 (marker row {marker_row}, image row {row})"
            );
        }
    }
}
