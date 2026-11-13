pub mod apply_update;
pub mod input_forward;
pub mod terminal;

use std::io::stdout;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::proto::{ClientMsg, FrameDecoder, HelloMsg, PROTOCOL_VERSION, ServerMsg, write_msg};

pub use apply_update::{ApplyState, apply_update};
pub use input_forward::{spawn_input_forwarder, spawn_resize_watcher};
pub use terminal::TerminalGuard;

/// builds the Hello handshake payload every venti client sends on connect.
pub fn hello_msg() -> HelloMsg {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cell_pixel = crossterm::terminal::window_size()
        .ok()
        .filter(|size| size.columns > 0 && size.rows > 0 && size.width > 0 && size.height > 0)
        .map(|size| {
            (
                size.width.div_ceil(size.columns),
                size.height.div_ceil(size.rows),
            )
        });
    HelloMsg {
        protocol: PROTOCOL_VERSION,
        term: std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()),
        colorterm: std::env::var("COLORTERM").ok(),
        size: (cols, rows),
        env: Vec::new(),
        cell_pixel,
    }
}

/// connects to the venti server unix socket and enters interactive client session.
pub fn run_client(socket_path: &Path, _session_name: &str) -> Result<(), String> {
    let stream = UnixStream::connect(socket_path)
        .map_err(|e| format!("failed to connect to socket {}: {e}", socket_path.display()))?;

    let hello = ClientMsg::Hello(hello_msg());

    let mut writer_stream = stream
        .try_clone()
        .map_err(|e| format!("failed to clone stream: {e}"))?;
    write_msg(&mut writer_stream, &hello).map_err(|e| format!("failed to send handshake: {e}"))?;

    let client_config =
        crate::config::load_from_file_or_default(&crate::util::paths::config_path())
            .unwrap_or_default();
    let _guard = TerminalGuard::enter_with_mouse(client_config.input.mouse)
        .map_err(|e| format!("terminal setup failed: {e}"))?;

    let running = Arc::new(AtomicBool::new(true));
    let resize_stream = writer_stream
        .try_clone()
        .map_err(|e| format!("failed to clone stream: {e}"))?;
    let _forwarder = spawn_input_forwarder(writer_stream, Arc::clone(&running));
    let _resize_watcher = spawn_resize_watcher(resize_stream, Arc::clone(&running));

    let mut reader_stream = stream;
    let mut decoder = FrameDecoder::<ServerMsg>::new();
    let mut temp = [0u8; 8192];
    let mut out = stdout();
    let mut apply_state = ApplyState::default();
    apply_state.gfx_passthrough = crate::util::outer_term::detect();
    apply_state.mouse_enabled = client_config.input.mouse;
    apply_state.kitty_images = _guard.kitty_images();

    use std::io::{Read, Write};
    let mut saw_server_msg = false;
    let mut saw_update = false;
    let mut corrupt = false;
    while running.load(Ordering::Relaxed) {
        match reader_stream.read(&mut temp) {
            Ok(0) => {
                // the server closed the connection without ever sending a
                // message: the daemon is stale or broken (e.g. an old build
                // holding the socket). surface this instead of restoring a
                // blank terminal silently.
                if !saw_server_msg {
                    return Err(format!(
                        "server at {} dropped the connection during the handshake; \
                         the daemon appears stale or broken - run `venti kill -t \
                         <session>` or remove the socket and .lock files, then retry",
                        socket_path.display()
                    ));
                }
                break;
            }
            Ok(n) => {
                decoder.push(&temp[..n]);
                loop {
                    match decoder.decode_next() {
                        Ok(Some(msg)) => {
                            match msg {
                                ServerMsg::Update(update) => {
                                    saw_server_msg = true;
                                    saw_update = true;
                                    if let Err(error) =
                                        apply_update(&mut out, &update, &mut apply_state)
                                    {
                                        running.store(false, Ordering::Relaxed);
                                        return Err(format!(
                                            "failed to render server update: {error}"
                                        ));
                                    }
                                }
                                ServerMsg::Exit(_) => {
                                    saw_server_msg = true;
                                    running.store(false, Ordering::Relaxed);
                                    break;
                                }
                                ServerMsg::Clipboard(data) => {
                                    saw_server_msg = true;
                                    if let Ok(s) = std::str::from_utf8(&data) {
                                        let _ = crate::util::clipboard::copy_external(s);
                                        if client_config.general.osc52 {
                                            // emit osc 52 so ssh/remote terminals can copy
                                            use base64::engine::{
                                                Engine, general_purpose::STANDARD,
                                            };
                                            let _ = write!(
                                                out,
                                                "\x1b]52;c;{}\x07",
                                                STANDARD.encode(&data)
                                            );
                                            let _ = out.flush();
                                        }
                                    }
                                }
                                ServerMsg::SetTitle(title) => {
                                    saw_server_msg = true;
                                    if let Err(error) = crossterm::execute!(
                                        out,
                                        crossterm::terminal::SetTitle(
                                            apply_update::safe_terminal_title(&title).into_owned()
                                        )
                                    ) {
                                        running.store(false, Ordering::Relaxed);
                                        return Err(format!(
                                            "failed to set terminal title: {error}"
                                        ));
                                    }
                                }
                                ServerMsg::RawOutput(bytes) => {
                                    saw_server_msg = true;
                                    let _ = out.write_all(&bytes);
                                    let _ = out.flush();
                                }
                                ServerMsg::HelloAck { .. } => {
                                    saw_server_msg = true;
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            corrupt = true;
                            running.store(false, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                if !saw_server_msg {
                    return Err(format!(
                        "server at {} dropped the connection during the handshake ({e}); \
                         the daemon appears stale or broken - run `venti kill -t \
                         <session>` or remove the socket and .lock files, then retry",
                        socket_path.display()
                    ));
                }
                break;
            }
        }
    }

    running.store(false, Ordering::Relaxed);
    // the stdin forwarder blocks on read until input arrives; do not join it.
    // dropping the join handle detaches the thread and process teardown reaps it.

    if corrupt {
        return Err(format!(
            "server at {} sent a frame that failed to decode; the connection was \
             dropped",
            socket_path.display()
        ));
    }

    if !saw_update {
        return Err(format!(
            "server at {} closed the connection before the first update; the daemon \
             appears stale or broken - run `venti kill -t <session>` or remove the \
             socket and .lock files, then retry",
            socket_path.display()
        ));
    }

    Ok(())
}
