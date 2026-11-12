use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// spawns a pty pair running a shell or child command.
pub struct PtyHandle {
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
}

fn select_working_directory(
    requested: Option<&Path>,
    home: Option<&Path>,
    current: Option<&Path>,
) -> Option<PathBuf> {
    [requested, home, current]
        .into_iter()
        .flatten()
        .find(|path| path.is_dir())
        .map(Path::to_path_buf)
        .or_else(|| Path::new("/").is_dir().then(|| PathBuf::from("/")))
}

/// spawns a child with graphics capabilities chosen by the attached client.
pub fn spawn_pty_with_graphics(
    cols: u16,
    rows: u16,
    shell: &str,
    cwd: Option<&PathBuf>,
    session_name: &str,
    pane_id: u32,
    graphics: crate::util::outer_term::GraphicsSpawn,
) -> Result<PtyHandle, String> {
    // cap dimensions to prevent pixel calculation overflow: 10*cols and 20*rows must fit in u16
    let cols = cols.clamp(1, 1000).min(6553);
    let rows = rows.clamp(1, 1000).min(3276);

    let pty_system = native_pty_system();

    // report non-zero pixel dimensions so icat/yazi/etc. proceed through their
    // winsize-pixel gate. prefer the attached terminal's real cell size when
    // the client reported one; otherwise fall back to the historic 10x20 assumption.
    let (cell_w, cell_h) = crate::util::outer_term::cell_pixel_size();
    let pixel_width = (u32::from(cols) * cell_w).min(u32::from(u16::MAX)) as u16;
    let pixel_height = (u32::from(rows) * cell_h).min(u32::from(u16::MAX)) as u16;

    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width,
            pixel_height,
        })
        .map_err(|e| format!("failed to open pty: {e}"))?;

    let shell_path = if !shell.is_empty() {
        shell.to_string()
    } else if let Ok(s) = std::env::var("SHELL") {
        if !s.is_empty() {
            s
        } else {
            "/bin/sh".to_string()
        }
    } else {
        "/bin/sh".to_string()
    };

    // support "shell -flag" style config values: split into program + args.
    let mut parts = shell_path.split_whitespace();
    let program = parts.next().unwrap_or("/bin/sh").to_string();
    let shell_args: Vec<String> = parts.map(str::to_string).collect();
    let mut cmd = CommandBuilder::new(&program);
    for arg in shell_args {
        cmd.arg(arg);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let current = std::env::current_dir().ok();
    if let Some(requested) = cwd
        && !requested.is_dir()
    {
        tracing::warn!(
            "pane {pane_id} working directory {} is unavailable; using a fallback",
            requested.display()
        );
    }
    if let Some(dir) = select_working_directory(
        cwd.map(PathBuf::as_path),
        home.as_deref(),
        current.as_deref(),
    ) {
        cmd.cwd(dir);
    }

    // advertise graphics capabilities truthfully: TERM=xterm-kitty only when the
    // outer (or the one behind tmux) speaks kitty graphics; TERM_PROGRAM=iTerm.app
    // when it speaks iterm2's inline image protocol; otherwise xterm-256color.
    // do NOT propagate KITTY_WINDOW_ID: that would make kitten icat pick
    // shared-memory transfer mode (can't cross multiplexer); without it kitten
    // falls back to stream mode which venti forwards.
    cmd.env("TERM", graphics.child_term());
    cmd.env("TERM_PROGRAM", graphics.child_term_program());
    cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    // vENTI_GRAPHICS tells apps (yazi, ranger plugins) that venti supports graphics passthrough
    cmd.env("VENTI_GRAPHICS", "1");
    cmd.env("VENTI", "1");
    cmd.env("VENTI_SESSION", session_name);
    cmd.env("VENTI_PANE", pane_id.to_string());

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("failed to spawn child in pty: {e}"))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("failed to clone pty reader: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("failed to take pty writer: {e}"))?;

    Ok(PtyHandle {
        master: pair.master,
        child,
        reader,
        writer,
    })
}

#[cfg(test)]
mod tests {
    use super::{select_working_directory, spawn_pty_with_graphics};
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;

    #[test]
    fn stale_requested_directory_falls_back_to_home_then_current_directory() {
        let root = std::env::temp_dir().join(format!(
            "venti-cwd-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let home = root.join("home");
        let current = root.join("current");
        std::fs::create_dir_all(&home).expect("create home directory");
        std::fs::create_dir_all(&current).expect("create current directory");
        let missing = root.join("removed-project");

        assert_eq!(
            select_working_directory(Some(&missing), Some(&home), Some(&current)),
            Some(home.clone())
        );
        assert_eq!(
            select_working_directory(Some(&missing), Some(&missing), Some(&current)),
            Some(current)
        );
        assert_eq!(
            select_working_directory(Some(&missing), Some(&missing), None),
            Some(PathBuf::from("/"))
        );

        std::fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn spawn_succeeds_when_saved_working_directory_was_removed() {
        let missing = std::env::temp_dir().join(format!(
            "venti-removed-cwd-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        assert!(!missing.exists());

        let mut pty = super::spawn_pty_with_graphics(
            80,
            24,
            "/bin/sh",
            Some(&missing),
            "cwd-test",
            1,
            crate::util::outer_term::GraphicsSpawn::from_env(),
        )
        .expect("spawn falls back from the removed working directory");
        pty.child.kill().expect("terminate test shell");
        pty.child.wait().expect("reap test shell");
    }

    #[test]
    fn spawned_child_receives_the_attached_foot_terminal_type() {
        let mut pty = spawn_pty_with_graphics(
            80,
            24,
            "/bin/sh -c env",
            None,
            "term-test",
            1,
            crate::util::outer_term::GraphicsSpawn::from_term("foot"),
        )
        .expect("spawn environment probe");
        let mut reader = BufReader::new(&mut pty.reader);
        let mut line = String::new();
        let mut term_seen = false;
        let mut program_seen = false;

        for _ in 0..256 {
            line.clear();
            if reader.read_line(&mut line).expect("read child environment") == 0 {
                break;
            }
            term_seen |= line.trim_end() == "TERM=foot";
            program_seen |= line.trim_end() == "TERM_PROGRAM=venti";
            if term_seen && program_seen {
                break;
            }
        }

        assert!(term_seen, "child did not receive TERM=foot");
        assert!(program_seen, "child did not receive TERM_PROGRAM=venti");
        pty.child.wait().expect("reap environment probe");
    }
}
