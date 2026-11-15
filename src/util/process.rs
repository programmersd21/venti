//! best-effort detection and termination of process trees via /proc (linux only).

/// returns true while the process has a foreground child in a different
/// process group than itself (i.e. an app is running in the foreground).
pub fn has_foreground_app(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let Some(end_comm) = stat.rfind(')') else {
        return false;
    };
    let fields: Vec<&str> = stat[end_comm + 1..].split_whitespace().collect();
    let pgrp: i32 = fields.get(2).and_then(|f| f.parse().ok()).unwrap_or(0);
    let tpgid: i32 = fields.get(5).and_then(|f| f.parse().ok()).unwrap_or(-1);
    tpgid != 0 && tpgid != pgrp
}

/// returns true if the given process has at least one non-zombie descendant.
///
/// this is a heuristic for "the user has a command running in this pane": the
/// shell is usually the direct child; anything below it that is not a zombie
/// counts as busy work (builds, editors, less, ...).
pub fn has_busy_children(pid: u32) -> bool {
    !descendants_of(pid).is_empty()
}

/// collects all descendant process ids (recursive, deduplicated).
pub fn descendants_of(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![pid];
    while let Some(current) = stack.pop() {
        for child in read_children(current) {
            if visited.insert(child) {
                if !is_zombie(child) {
                    out.push(child);
                }
                stack.push(child);
            }
        }
    }
    out
}

/// sends sighup to the process and its tree, then sigkill to survivors shortly after.
///
/// foreground processes die on the sighup; the delayed sigkill sweep cleans up
/// anything that chose to ignore it. non-blocking for the caller.
pub fn kill_process_tree(pid: u32) {
    let tree = descendants_of(pid);
    for &child in &tree {
        send_signal(child, rustix::process::Signal::HUP);
    }
    send_signal(pid, rustix::process::Signal::HUP);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        for &child in &tree {
            if process_exists(child) {
                send_signal(child, rustix::process::Signal::KILL);
            }
        }
        if process_exists(pid) {
            send_signal(pid, rustix::process::Signal::KILL);
        }
    });
}

fn send_signal(pid: u32, sig: rustix::process::Signal) {
    if let Ok(raw) = i32::try_from(pid)
        && let Some(p) = rustix::process::Pid::from_raw(raw)
    {
        let _ = rustix::process::kill_process(p, sig);
    }
}

fn process_exists(pid: u32) -> bool {
    if let Ok(raw) = i32::try_from(pid)
        && let Some(p) = rustix::process::Pid::from_raw(raw)
    {
        return rustix::process::test_kill_process(p).is_ok();
    }
    false
}

fn read_children(pid: u32) -> Vec<u32> {
    let path = format!("/proc/{pid}/task/{pid}/children");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    content
        .split_whitespace()
        .filter_map(|t| t.parse::<u32>().ok())
        .collect()
}

fn is_zombie(pid: u32) -> bool {
    let path = format!("/proc/{pid}/stat");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return false; // unreadable: assume alive
    };
    // state is the field right after the closing paren of the comm
    let Some(end_comm) = content.rfind(')') else {
        return false;
    };
    let rest = content[end_comm + 1..].trim_start();
    rest.starts_with('Z')
}

/// best-effort name of the program currently in the foreground of the given
/// shell's controlling terminal (linux only). falls back to the shell itself.
pub fn foreground_process_name(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    let fields: Vec<&str> = stat[end + 1..].split_whitespace().collect();
    // fields: state ppid pgrp session tty_nr tpgid ...
    let tpgid: i32 = fields.get(5)?.parse().ok()?;
    let target = if tpgid > 0 { tpgid as u32 } else { pid };
    process_name(target).or_else(|| process_name(pid))
}

/// get the working directory of a process (linux only). returns the symlink
/// target of /proc/{pid}/cwd, or None if unavailable.
pub fn process_cwd(pid: u32) -> Option<String> {
    let cwd_path = format!("/proc/{pid}/cwd");
    std::fs::read_link(&cwd_path)
        .ok()?
        .into_os_string()
        .into_string()
        .ok()
}

fn process_name(pid: u32) -> Option<String> {
    if let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) {
        let first = cmdline.split(|&b| b == 0).next().unwrap_or(&[]);
        if !first.is_empty()
            && let Ok(text) = std::str::from_utf8(first)
        {
            let base = std::path::Path::new(text)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(text);
            if !base.is_empty() {
                return Some(base.to_string());
            }
        }
    }
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = comm.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}
