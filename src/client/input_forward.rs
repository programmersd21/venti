use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::proto::{ClientMsg, encode, write_msg};

/// forwards raw stdin bytes directly to server connection.
pub fn spawn_input_forwarder(
    mut stream: UnixStream,
    running: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = [0u8; 1024];

        while running.load(Ordering::Relaxed) {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let msg = ClientMsg::Input(buf[..n].to_vec());
                    match encode(&msg) {
                        Ok(bytes) => {
                            if stream.write_all(&bytes).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// watches terminal dimensions and forwards resize events to the server.
/// uses signal-hook for instant SIGWINCH reaction, with a fast fallback poll.
pub fn spawn_resize_watcher(
    mut stream: UnixStream,
    running: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut last = crossterm::terminal::size().ok();
        let mut signals = signal_hook::iterator::Signals::new([signal_hook::consts::SIGWINCH]).ok();

        while running.load(Ordering::Relaxed) {
            let mut got_resize = false;
            if let Some(ref mut sigs) = signals {
                for _ in sigs.pending() {
                    got_resize = true;
                }
            }

            let cur = crossterm::terminal::size();
            if let Ok(size) = cur
                && (got_resize || Some(size) != last)
            {
                last = Some(size);
                let msg = ClientMsg::Resize(size.0, size.1);
                if write_msg(&mut stream, &msg).is_err() {
                    break;
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    })
}
