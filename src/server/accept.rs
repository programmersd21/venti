use crossbeam_channel::Sender;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::proto::{
    ClientMsg, FrameDecoder, MAX_FRAME_SIZE, PROTOCOL_VERSION, ServerMsg, write_msg,
};
use crate::server::core::CoreEvent;

fn send_event_while_running(
    event_tx: &Sender<CoreEvent>,
    running: &AtomicBool,
    mut event: CoreEvent,
) -> bool {
    loop {
        if !running.load(Ordering::Relaxed) {
            return false;
        }
        match event_tx.send_timeout(event, std::time::Duration::from_millis(100)) {
            Ok(()) => return true,
            Err(crossbeam_channel::SendTimeoutError::Timeout(returned)) => {
                if !running.load(Ordering::Relaxed) {
                    return false;
                }
                event = returned;
            }
            Err(crossbeam_channel::SendTimeoutError::Disconnected(_)) => return false,
        }
    }
}

/// lock file path guarding a session's socket against two servers starting at
/// once. the file is persistent; an advisory lock protects its inode.
pub fn lock_path_for(socket_path: &Path) -> PathBuf {
    socket_path.with_extension("lock")
}

/// claims the right to serve `socket_path` and retains the lock descriptor.
fn acquire_server_lock(socket_path: &Path) -> Result<(PathBuf, File), String> {
    let lock = lock_path_for(socket_path);
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock)
        .map_err(|e| format!("failed to open server lock {}: {e}", lock.display()))?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(|e| format!("another server already owns {}: {e}", socket_path.display()))?;
    Ok((lock, file))
}

/// spawns unix socket listener thread accepting incoming client connections.
pub fn spawn_accept_loop(
    socket_path: &Path,
    event_tx: Sender<CoreEvent>,
    running: Arc<AtomicBool>,
    max_clients: usize,
) -> Result<std::thread::JoinHandle<()>, String> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create socket dir: {e}"))?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("failed to secure socket dir {}: {e}", parent.display()))?;
    }

    let (_lock_path, lock_file) = acquire_server_lock(socket_path)?;

    // only remove a stale socket, never an unrelated file.
    if socket_path.exists() {
        use std::os::unix::fs::FileTypeExt;
        let is_socket = fs::metadata(socket_path)
            .map(|m| m.file_type().is_socket())
            .unwrap_or(false);
        if is_socket {
            let _ = fs::remove_file(socket_path);
        } else {
            return Err(format!(
                "refusing to remove non-socket file at {}",
                socket_path.display()
            ));
        }
    }

    let listener = match UnixListener::bind(socket_path) {
        Ok(l) => l,
        Err(e) => {
            return Err(format!(
                "failed to bind unix socket {}: {e}",
                socket_path.display()
            ));
        }
    };
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("failed to secure socket {}: {e}", socket_path.display()))?;

    listener
        .set_nonblocking(true)
        .map_err(|e| format!("failed to configure unix socket: {e}"))?;
    let handle = std::thread::spawn(move || {
        let _lock_file = lock_file;
        let mut client_counter = 0usize;
        let active_clients = Arc::new(AtomicUsize::new(0));
        let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();
        while running.load(Ordering::Relaxed) {
            let mut active_workers = Vec::with_capacity(workers.len());
            for worker in workers.drain(..) {
                if worker.is_finished() {
                    let _ = worker.join();
                } else {
                    active_workers.push(worker);
                }
            }
            workers = active_workers;
            match listener.accept() {
                Ok((stream, _)) => {
                    let current = active_clients.load(Ordering::Relaxed);

                    // enforce connection limit to prevent thread exhaustion dos
                    if current >= max_clients {
                        tracing::warn!(
                            "rejecting client connection: limit of {max_clients} active clients reached"
                        );
                        let _ = write_msg(
                            &mut &stream,
                            &ServerMsg::Exit(crate::proto::ExitReason::Error(
                                "server at client connection limit".to_string(),
                            )),
                        );
                        continue;
                    }

                    active_clients.fetch_add(1, Ordering::Relaxed);
                    client_counter = client_counter.wrapping_add(1);
                    let client_id = client_counter;
                    let tx = event_tx.clone();
                    let run = Arc::clone(&running);
                    let active = Arc::clone(&active_clients);
                    workers.push(std::thread::spawn(move || {
                        handle_client(client_id, stream, tx, run);
                        active.fetch_sub(1, Ordering::Relaxed);
                    }));
                }
                Err(error) => {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    if error.kind() != std::io::ErrorKind::WouldBlock {
                        tracing::warn!("unix socket accept failed: {error}");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        for worker in workers {
            let _ = worker.join();
        }
    });

    Ok(handle)
}

fn handle_client(
    client_id: usize,
    mut stream: UnixStream,
    event_tx: Sender<CoreEvent>,
    running: Arc<AtomicBool>,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let hello = match read_client_message_before(&mut stream, deadline) {
        Ok(msg) => msg,
        Err(_) => return,
    };
    if stream
        .set_read_timeout(Some(std::time::Duration::from_millis(250)))
        .is_err()
    {
        tracing::warn!(
            "client {client_id}: failed to set read timeout; a wedged client may never be deregistered"
        );
    }

    let (cols, rows, term, term_program, cell_pixel) = match hello {
        ClientMsg::Hello(hello) => {
            if hello.protocol != PROTOCOL_VERSION {
                let _ = write_msg(
                    &mut stream,
                    &ServerMsg::Exit(crate::proto::ExitReason::Error(
                        "protocol version mismatch".to_string(),
                    )),
                );
                return;
            }
            let term_program = hello
                .env
                .into_iter()
                .find(|(key, _)| key == "TERM_PROGRAM")
                .map(|(_, value)| value);
            (
                hello.size.0,
                hello.size.1,
                hello.term,
                term_program,
                hello.cell_pixel,
            )
        }
        _ => return,
    };
    let cols = cols.clamp(1, 1000);
    let rows = rows.clamp(1, 500);

    let writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    if writer_stream
        .set_write_timeout(Some(std::time::Duration::from_millis(100)))
        .is_err()
    {
        return;
    }

    if !send_event_while_running(
        &event_tx,
        &running,
        CoreEvent::NewClient {
            client_id,
            stream: writer_stream,
            cols,
            rows,
            term,
            term_program,
            cell_pixel,
        },
    ) {
        return;
    }

    // registered: read until exit and deregister exactly once.
    pump_client_messages(client_id, &mut stream, &event_tx, &running);
}

fn read_client_message_before(
    stream: &mut UnixStream,
    deadline: std::time::Instant,
) -> Result<ClientMsg, ()> {
    let mut header = [0u8; 4];
    read_exact_before(stream, &mut header, deadline)?;
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(());
    }
    let mut payload = vec![0u8; len];
    read_exact_before(stream, &mut payload, deadline)?;
    postcard::from_bytes(&payload).map_err(|_| ())
}

fn read_exact_before(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    deadline: std::time::Instant,
) -> Result<(), ()> {
    while !bytes.is_empty() {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() || stream.set_read_timeout(Some(remaining)).is_err() {
            return Err(());
        }
        match stream.read(bytes) {
            Ok(0) => return Err(()),
            Ok(read) => bytes = &mut bytes[read..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && std::time::Instant::now() < deadline =>
            {
                continue;
            }
            Err(_) => return Err(()),
        }
    }
    Ok(())
}

/// reads frames from a registered client until eof, error, detach, or shutdown,
/// then deregisters the client exactly once.
fn pump_client_messages(
    client_id: usize,
    stream: &mut UnixStream,
    event_tx: &Sender<CoreEvent>,
    running: &Arc<AtomicBool>,
) {
    let mut decoder = FrameDecoder::<ClientMsg>::new();
    let mut temp = [0u8; 8192];

    'outer: while running.load(Ordering::Relaxed) {
        let n = match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && running.load(Ordering::Relaxed) =>
            {
                continue;
            }
            Err(_) => break,
        };
        decoder.push(&temp[..n]);

        loop {
            match decoder.decode_next() {
                Ok(Some(msg)) => {
                    let (forward, detach) = match msg {
                        ClientMsg::Input(bytes) => {
                            (Some(CoreEvent::ClientInput { client_id, bytes }), false)
                        }
                        ClientMsg::Resize(cols, rows) => (
                            Some(CoreEvent::ClientResize {
                                client_id,
                                cols,
                                rows,
                            }),
                            false,
                        ),
                        ClientMsg::Mouse(msg) => {
                            (Some(CoreEvent::ClientMouse { client_id, msg }), false)
                        }
                        ClientMsg::Paste(bytes) => {
                            (Some(CoreEvent::ClientPaste { client_id, bytes }), false)
                        }
                        ClientMsg::Command(command) => {
                            (Some(CoreEvent::ClientCommand { client_id, command }), false)
                        }
                        ClientMsg::Detach => (None, true),
                        ClientMsg::Hello(_) => break 'outer,
                    };
                    if let Some(event) = forward
                        && !send_event_while_running(event_tx, running, event)
                    {
                        break 'outer;
                    }
                    if detach {
                        break 'outer;
                    }
                }
                Ok(None) => break,
                Err(_) => break 'outer,
            }
        }
    }

    let _ = send_event_while_running(event_tx, running, CoreEvent::ClientDetach { client_id });
}

#[cfg(test)]
mod tests {
    use super::{
        acquire_server_lock, pump_client_messages, read_client_message_before,
        send_event_while_running,
    };
    use crate::proto::{ClientMsg, HelloMsg, PROTOCOL_VERSION, encode};
    use crate::server::core::CoreEvent;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn server_lock_excludes_second_owner_until_descriptor_drops() {
        let root = std::env::temp_dir().join(format!(
            "venti-lock-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::create_dir_all(&root).expect("create lock test directory");
        let socket = root.join("session.sock");

        let first = acquire_server_lock(&socket).expect("first lock owner");
        assert!(acquire_server_lock(&socket).is_err());
        drop(first);
        assert!(acquire_server_lock(&socket).is_ok());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn handshake_deadline_limits_a_trickling_peer() {
        let (mut reader, mut writer) = UnixStream::pair().expect("socket pair");
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(120);
        let sender = std::thread::spawn(move || {
            for byte in encode(&ClientMsg::Hello(HelloMsg {
                protocol: PROTOCOL_VERSION,
                term: "xterm".into(),
                colorterm: None,
                size: (80, 24),
                env: Vec::new(),
                cell_pixel: None,
            }))
            .expect("encode hello")
            {
                if writer.write_all(&[byte]).is_err() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        });

        let start = std::time::Instant::now();
        assert!(read_client_message_before(&mut reader, deadline).is_err());
        assert!(start.elapsed() < std::time::Duration::from_millis(500));
        drop(reader);
        let _ = sender.join();
    }

    #[test]
    fn full_event_queue_send_stops_promptly_during_shutdown() {
        let (sender, _receiver) = crossbeam_channel::bounded(1);
        sender
            .try_send(CoreEvent::ConfigChanged)
            .expect("fill queue");
        let running = Arc::new(AtomicBool::new(false));
        let start = std::time::Instant::now();

        assert!(!send_event_while_running(
            &sender,
            &running,
            CoreEvent::ConfigChanged
        ));
        assert!(start.elapsed() < std::time::Duration::from_millis(50));
        assert!(!running.load(Ordering::Relaxed));
    }

    #[test]
    fn second_hello_closes_the_registered_client() {
        let (mut server_stream, mut client_stream) = UnixStream::pair().expect("socket pair");
        let (event_tx, event_rx) = crossbeam_channel::bounded(2);
        let running = Arc::new(AtomicBool::new(true));
        let pump_running = Arc::clone(&running);
        let worker = std::thread::spawn(move || {
            pump_client_messages(17, &mut server_stream, &event_tx, &pump_running);
        });
        let hello = ClientMsg::Hello(HelloMsg {
            protocol: PROTOCOL_VERSION,
            term: "xterm".into(),
            colorterm: None,
            size: (80, 24),
            env: Vec::new(),
            cell_pixel: None,
        });
        client_stream
            .write_all(&encode(&hello).expect("encode hello"))
            .expect("send second hello");

        let event = event_rx.recv_timeout(std::time::Duration::from_millis(200));
        drop(client_stream);
        worker.join().expect("pump thread exits");
        assert!(matches!(
            event,
            Ok(CoreEvent::ClientDetach { client_id: 17 })
        ));
    }
}
