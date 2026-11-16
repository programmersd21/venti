use std::io::Write;
use std::process::{Command as StdCommand, Stdio};

/// copies text to the system clipboard using external cli tools.
pub fn copy_external(text: &str) -> bool {
    let candidates = [
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
        ("xsel", &["-ib"][..]),
    ];

    for (cmd, args) in candidates {
        let spawned = StdCommand::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else {
            continue;
        };
        let ok = match child.stdin.take() {
            Some(mut stdin) => {
                let written = stdin.write_all(text.as_bytes()).is_ok();
                // close stdin so the child sees eof and can exit.
                drop(stdin);
                written && child.wait().is_ok()
            }
            None => false,
        };
        if ok {
            return true;
        }
    }

    false
}

/// pastes text from the system clipboard using external cli tools.
pub fn paste_external() -> Option<String> {
    let candidates = [
        ("wl-paste", &[][..]),
        ("xclip", &["-selection", "clipboard", "-o"][..]),
        ("xsel", &["-ob"][..]),
    ];

    for (cmd, args) in candidates {
        match StdCommand::new(cmd).args(args).output() {
            Ok(output) if output.status.success() => {
                return String::from_utf8(output.stdout).ok();
            }
            _ => continue,
        }
    }

    None
}
