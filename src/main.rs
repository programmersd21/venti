#![forbid(unsafe_code)]
#[cfg(not(target_os = "linux"))]
compile_error!("venti is linux only (for now)");

use clap::Parser;
use std::fs;
use std::io;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use venti::cli::{Cli, CliCommand, ConfigAction};
use venti::cli_help::{self, BLUE, GREEN, OVERLAY, RED, SUBTEXT, TEAL, TEXT, YELLOW};
use venti::client::run_client;
use venti::config::{Theme, default_config_commented, load_from_file_or_default, load_from_str};
use venti::proto::{ClientMsg, ServerMsg, read_msg, write_msg};
use venti::server::run_server;
use venti::util::clipboard::paste_external;
use venti::util::paths::{config_path, socket_dir, socket_path};

fn main() -> anyhow::Result<()> {
    let _guard = init_tracing();

    // hand-rendered help/version pages, intercepted before clap sees argv.
    // every --help/-h/-V/--version and `venti help ...` gets the design system.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Some(page) = cli_help::intercept(&argv) {
        print!("{page}");
        return Ok(());
    }

    let cli = Cli::parse();

    match cli.command {
        Some(CliCommand::Config(config_args)) => match config_args.action {
            None => missing_args_page(&["config"]),
            Some(ConfigAction::Path) => {
                let p = config_path();
                println!("  {}", cli_help::paint(TEXT, &p.display().to_string()));
            }
            Some(ConfigAction::Init) => {
                let p = config_path();
                if p.exists() {
                    eprintln!(
                        "  {} {} {}",
                        cli_help::paint_bold(RED, "✗"),
                        cli_help::paint(TEXT, "config already exists"),
                        cli_help::paint(OVERLAY, &p.display().to_string())
                    );
                    std::process::exit(1);
                }
                if let Some(parent) = p.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&p, default_config_commented())?;
                println!(
                    "  {} {} {}",
                    cli_help::paint_bold(GREEN, "✓"),
                    cli_help::paint(TEXT, "created"),
                    cli_help::paint(SUBTEXT, &p.display().to_string())
                );
            }
            Some(ConfigAction::Check) => {
                let p = config_path();
                if !p.exists() {
                    println!("{}", cli_help::warn("no config found · defaults valid"));
                    return Ok(());
                }
                let content = fs::read_to_string(&p)?;
                match load_from_str(&content) {
                    Ok(_) => {
                        println!("{}", cli_help::success("config valid"));
                    }
                    Err(errors) => {
                        for err in &errors {
                            eprintln!("{}", cli_help::error(&err.to_string()));
                        }
                        std::process::exit(1);
                    }
                }
            }
        },
        Some(CliCommand::Completions(args)) => {
            let Some(shell) = args.shell else {
                missing_args_page(&["completions"]);
            };
            let mut cmd = <Cli as clap::CommandFactory>::command();
            let name = cmd.get_name().to_string();
            let shell = match shell {
                venti::cli::ShellKind::Bash => clap_complete::Shell::Bash,
                venti::cli::ShellKind::Elvish => clap_complete::Shell::Elvish,
                venti::cli::ShellKind::Fish => clap_complete::Shell::Fish,
                venti::cli::ShellKind::Zsh => clap_complete::Shell::Zsh,
            };
            clap_complete::generate(shell, &mut cmd, name, &mut io::stdout());
        }
        Some(CliCommand::Man) => {
            // the man subcommand prints the exact manual page shipped in the repo
            print!("{}", include_str!("../man/venti.1"));
        }
        Some(CliCommand::Doctor) => {
            let term = std::env::var("TERM").unwrap_or_default();
            let colorterm = std::env::var("COLORTERM").unwrap_or_default();
            let truecolor = matches!(colorterm.as_str(), "truecolor" | "24bit");
            let locale = std::env::var("LC_ALL")
                .or_else(|_| std::env::var("LC_CTYPE"))
                .or_else(|_| std::env::var("LANG"))
                .unwrap_or_default();
            let utf8 = locale.to_lowercase().contains("utf");

            let graphics = venti::util::outer_term::detect();
            let tmux_outer = venti::util::outer_term::tmux_outer_graphics();
            let graphics_label = match (graphics, tmux_outer) {
                (venti::util::outer_term::OuterGraphics::Kitty, _) => "kitty".to_string(),
                (venti::util::outer_term::OuterGraphics::Sixel, _) => "sixel".to_string(),
                (venti::util::outer_term::OuterGraphics::Iterm2, _) => "iterm2".to_string(),
                (venti::util::outer_term::OuterGraphics::Tmux, Some(g)) => {
                    format!("tmux ({g:?})").to_lowercase()
                }
                (venti::util::outer_term::OuterGraphics::Tmux, None) => "tmux".to_string(),
                (venti::util::outer_term::OuterGraphics::Unknown, _) => "none".to_string(),
            };
            let graphics_known =
                !matches!(graphics, venti::util::outer_term::OuterGraphics::Unknown);

            let ok = |s: &str| cli_help::paint(GREEN, s);
            let warn_c = |s: &str| cli_help::paint(YELLOW, s);
            let bad = |s: &str| cli_help::paint(RED, s);
            let faint = |s: &str| cli_help::paint(OVERLAY, s);

            let mut protos = Vec::new();
            if venti::util::outer_term::sixel_supported() {
                protos.push(ok("sixel"));
            }

            let sock = socket_path("default");
            let rows: Vec<(&str, String)> = vec![
                (
                    "term",
                    if term.is_empty() {
                        bad("missing")
                    } else {
                        ok(&term)
                    },
                ),
                (
                    "color",
                    if truecolor {
                        ok("24bit")
                    } else {
                        warn_c("8bit")
                    },
                ),
                ("unicode", if utf8 { ok("utf8") } else { warn_c("ascii") }),
                (
                    "graphics",
                    if graphics_known {
                        ok(&graphics_label)
                    } else {
                        faint(&graphics_label)
                    },
                ),
                (
                    "proto",
                    if protos.is_empty() {
                        faint("none")
                    } else {
                        protos.join(&format!(" {} ", cli_help::paint(OVERLAY, "+")))
                    },
                ),
                ("mouse", ok("sgr")),
                ("paste", ok("bracketed")),
                ("focus", ok("events")),
                (
                    "clip",
                    if std::env::var_os("SSH_TTY").is_some() {
                        cli_help::paint(BLUE, "ssh")
                    } else {
                        ok("osc52")
                    },
                ),
                ("socket", faint(&sock.display().to_string())),
            ];

            println!(
                "  {} graphics is experimental: it may glitch on some terminals.",
                cli_help::paint(YELLOW, "note")
            );
            println!("  set graphics = false in ~/.config/venti/config.toml to turn images off.");

            println!(
                "  {} {}",
                cli_help::paint_bold(TEXT, "venti"),
                cli_help::paint_bold(BLUE, "doctor")
            );
            println!();
            let col = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
            for (k, v) in rows {
                println!(
                    "  {}{}{}",
                    cli_help::paint(OVERLAY, k),
                    " ".repeat(col + 3 - k.len()),
                    v
                );
            }
        }

        Some(CliCommand::Server(args)) => {
            let sock = socket_path(&args.session);
            let cfg_p = config_path();
            let config = match load_from_file_or_default(&cfg_p) {
                Ok(config) => {
                    // if the config file doesn't exist yet, write the documented
                    // template (same file `venti config init` produces). serializing
                    // the default struct here would instead emit a comment-free dump
                    // with float round-tripping artifacts.
                    if !cfg_p.exists()
                        && let Some(parent) = cfg_p.parent()
                    {
                        fs::create_dir_all(parent)?;
                        match fs::write(&cfg_p, default_config_commented()) {
                            Ok(()) => {
                                eprintln!(
                                    "  {} {} {}",
                                    cli_help::paint_bold(YELLOW, "⚠"),
                                    cli_help::paint(TEXT, "created default config"),
                                    cli_help::paint(OVERLAY, &cfg_p.display().to_string())
                                );
                            }
                            Err(e) => {
                                eprintln!(
                                    "  {} {}",
                                    cli_help::paint_bold(YELLOW, "⚠"),
                                    cli_help::paint(
                                        TEXT,
                                        &format!("could not save default config: {e}")
                                    )
                                );
                            }
                        }
                    }
                    config
                }
                Err(errors) => {
                    for err in &errors {
                        eprintln!("{}", cli_help::error(&err.to_string()));
                    }
                    std::process::exit(1);
                }
            };
            let theme = Theme::resolve(&config.theme);
            run_server(&sock, &args.session, config, theme).map_err(|e| anyhow::anyhow!("{e}"))?;
            return Ok(());
        }
        Some(CliCommand::New(args)) => {
            if let Err(e) = attach_or_start(&args.session, false) {
                exit_err(e);
            }
        }
        Some(CliCommand::Attach(args)) => {
            if let Err(e) = attach_or_start(&args.target, args.detach_others) {
                exit_err(e);
            }
        }
        Some(CliCommand::Ls) => {
            list_sessions()?;
        }
        Some(CliCommand::Kill(args)) => {
            let Some(target) = &args.target else {
                missing_args_page(&["kill"]);
            };
            if let Err(e) = send_command(target, "kill_session") {
                eprintln!("{}", cli_help::error(&e));
                std::process::exit(1);
            }
            println!(
                "  {} {} {}",
                cli_help::paint_bold(GREEN, "✓"),
                cli_help::paint(TEXT, "killed"),
                cli_help::paint(SUBTEXT, target)
            );
        }
        Some(CliCommand::KillServer) => {
            let sessions = live_session_names();
            for session in &sessions {
                let _ = send_command(session, "kill_session");
            }
            if sessions.is_empty() {
                println!("{}", cli_help::dim_line("no sessions to kill"));
            } else {
                println!(
                    "  {} {} {}",
                    cli_help::paint_bold(GREEN, "✓"),
                    cli_help::paint(TEXT, "killed"),
                    cli_help::paint(SUBTEXT, &format!("{} session(s)", sessions.len()))
                );
            }

            // give servers a moment to exit after kill_session, then remove
            // this user's socket directory (locks included) so stale sockets
            // don't linger
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = std::fs::remove_dir_all(venti::util::paths::socket_dir());
        }
        Some(CliCommand::Send(args)) => {
            let Some(keys) = &args.keys else {
                missing_args_page(&["send"]);
            };
            if let Err(e) = send_keys(&args.target, keys) {
                eprintln!("{}", cli_help::error(&e));
                std::process::exit(1);
            }
        }
        Some(CliCommand::Run(args)) => {
            let cmd = args.command.join(" ");
            if cmd.is_empty() {
                missing_args_page(&["run"]);
            }
            if let Err(e) = send_command(&args.target, &cmd) {
                eprintln!("{}", cli_help::error(&e));
                std::process::exit(1);
            }
        }
        Some(CliCommand::Paste(args)) => {
            if let Err(e) = send_paste(&args.target, args.stdin) {
                eprintln!("{}", cli_help::error(&e));
                std::process::exit(1);
            }
        }
        None => {
            if let Err(e) = attach_or_start("default", false) {
                exit_err(e);
            }
        }
    }

    Ok(())
}

/// initializes tracing with RUST_LOG filtering (default: warn). one-shot cli
/// commands log to stderr; the daemon has no stderr, so it also appends to
/// $XDG_STATE_HOME/venti/venti.log.
fn exit_err(e: anyhow::Error) -> ! {
    eprintln!("{}", cli_help::error(&e.to_string()));
    std::process::exit(1);
}

/// missing required input: print the custom help page for the subcommand to
/// stderr (stdout stays clean for scripts) and exit with the clap usage-error
/// convention, 2.
fn missing_args_page(path: &[&str]) -> ! {
    if let Some(page) = cli_help::help_for(path) {
        eprint!("{page}");
    }
    std::process::exit(2);
}

fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let log_dir = venti::util::paths::state_dir();
    match fs::create_dir_all(&log_dir) {
        Ok(()) => {
            let (writer, guard) = tracing_appender::non_blocking(tracing_appender::rolling::never(
                log_dir,
                "venti.log",
            ));
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(writer)
                .with_ansi(false)
                .init();
            Some(guard)
        }
        Err(_) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stderr)
                .init();
            None
        }
    }
}

fn session_names() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(socket_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("sock")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    names
}

fn live_session_names() -> Vec<String> {
    let mut live = Vec::new();
    for name in session_names() {
        if UnixStream::connect(socket_path(&name)).is_ok() {
            live.push(name);
        } else {
            // clean up stale socket files left by an unclean server exit.
            let _ = fs::remove_file(socket_path(&name));
        }
    }
    live
}

fn list_sessions() -> anyhow::Result<()> {
    let names = live_session_names();
    if names.is_empty() {
        println!("  {}", cli_help::paint(OVERLAY, "no sessions"));
        println!(
            "  {} {}",
            cli_help::paint(SUBTEXT, "create one with"),
            cli_help::paint_bold(BLUE, "venti new -s work")
        );
        return Ok(());
    }
    for name in names {
        println!(
            "  {} {}",
            cli_help::paint_bold(TEAL, "●"),
            cli_help::paint(TEXT, &name)
        );
    }
    Ok(())
}

fn connect(session_name: &str) -> Result<(UnixStream, UnixStream), String> {
    let sock = socket_path(session_name);
    let stream = UnixStream::connect(&sock)
        .map_err(|e| format!("failed to connect to {}: {e}", sock.display()))?;
    let mut writer = stream
        .try_clone()
        .map_err(|e| format!("failed to clone socket: {e}"))?;
    let hello = ClientMsg::Hello(venti::client::hello_msg());
    write_msg(&mut writer, &hello).map_err(|e| format!("failed to send handshake: {e}"))?;
    Ok((stream, writer))
}

fn send_command(session_name: &str, command: &str) -> Result<(), String> {
    let (mut reader, mut writer) = connect(session_name)?;
    write_msg(&mut writer, &ClientMsg::Command(command.to_string()))
        .map_err(|e| format!("failed to send command: {e}"))?;
    let _ = reader.set_read_timeout(Some(Duration::from_millis(200)));

    // wait briefly for the server to acknowledge or exit so one-shot cli
    // commands observe completion deterministically.
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        match read_msg::<_, ServerMsg>(&mut reader) {
            Ok(ServerMsg::Exit(_)) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    Ok(())
}

/// reads the system clipboard (or stdin with --stdin) and pastes it into the
/// focused pane of `session_name`. the server wraps the bytes in bracketed
/// paste markers when the pane enabled the mode.
fn send_paste(session_name: &str, from_stdin: bool) -> Result<(), String> {
    let bytes = if from_stdin {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf)
            .map_err(|e| format!("failed to read stdin: {e}"))?;
        buf
    } else {
        let content = paste_external().ok_or_else(|| {
            "failed to read clipboard (no wl-paste, xclip or xsel found)".to_string()
        })?;
        if content.is_empty() {
            return Err("clipboard is empty".to_string());
        }
        content.into_bytes()
    };

    let (_reader, mut writer) = connect(session_name)?;
    write_msg(&mut writer, &ClientMsg::Paste(bytes))
        .map_err(|e| format!("failed to send paste: {e}"))?;
    write_msg(&mut writer, &ClientMsg::Detach).map_err(|e| format!("failed to detach: {e}"))?;
    Ok(())
}

fn send_keys(session_name: &str, keys: &str) -> Result<(), String> {
    let (_reader, mut writer) = connect(session_name)?;
    write_msg(&mut writer, &ClientMsg::Input(keys.as_bytes().to_vec()))
        .map_err(|e| format!("failed to send keys: {e}"))?;
    write_msg(&mut writer, &ClientMsg::Detach).map_err(|e| format!("failed to detach: {e}"))?;
    Ok(())
}

fn attach_or_start(session_name: &str, detach_others: bool) -> anyhow::Result<()> {
    // refuse to run interactively from inside a venti pane: panes set VENTI=1
    // and VENTI_PANE=<id>, so a plain `venti` inside venti would nest (and in
    // rc files, respawn) forever. control commands (ls, send, kill, paste,
    // doctor, ...) are still allowed from inside a pane; VENTI_NEST=1 opts out.
    if std::env::var_os("VENTI").is_some() && std::env::var_os("VENTI_NEST").is_none() {
        eprintln!(
            "  {} {}\n  {} {}",
            cli_help::paint_bold(YELLOW, "⚠"),
            cli_help::paint(TEXT, "venti is already running inside this pane"),
            cli_help::paint(SUBTEXT, "start a nested instance with"),
            cli_help::paint_bold(BLUE, "VENTI_NEST=1 venti ...")
        );
        std::process::exit(1);
    }
    let sock = socket_path(session_name);

    // a stale socket file can remain after an unclean server exit. probe with a
    // real connection and only reuse it when the server is alive.
    let alive = UnixStream::connect(&sock).is_ok();
    if !alive {
        // pre-flight the config so a bad config fails here, fast and visibly,
        // instead of the daemon dying silently on a null stderr.
        let cfg_p = config_path();
        if cfg_p.exists()
            && let Ok(content) = fs::read_to_string(&cfg_p)
            && let Err(errors) = load_from_str(&content)
        {
            for err in &errors {
                eprintln!("{}", cli_help::error(&err.to_string()));
            }
            eprintln!(
                "  {}",
                cli_help::dim_line(&format!(
                    "fix {} or run venti config check",
                    cfg_p.display()
                ))
            );
            std::process::exit(1);
        }

        let _ = fs::remove_file(&sock);
        spawn_daemon(session_name)?;

        let start = Instant::now();
        while !sock.exists() {
            if start.elapsed() > Duration::from_secs(3) {
                if let Some(tail) = daemon_log_tail(12) {
                    let log = venti::util::paths::state_dir().join("venti.log");
                    eprintln!(
                        "  {}",
                        cli_help::paint(OVERLAY, &format!("server log tail ({}):", log.display()))
                    );
                    for line in tail.lines() {
                        eprintln!("    {}", cli_help::paint(SUBTEXT, line));
                    }
                }
                anyhow::bail!("timed out waiting for server socket to initialize");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    if detach_others {
        // detach all clients already attached to the session before we take over.
        let _ = send_command(session_name, "detach_others");
    }

    if let Err(e) = run_client(&sock, session_name) {
        eprintln!("{}", cli_help::error(&e.to_string()));
        std::process::exit(1);
    }

    Ok(())
}

/// opens the daemon log for appending, creating the state dir if needed.
fn daemon_log() -> Option<fs::File> {
    let dir = venti::util::paths::state_dir();
    fs::create_dir_all(&dir).ok()?;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("venti.log"))
        .ok()
}

/// returns the last `n` lines of the daemon log, if it exists and is readable.
fn daemon_log_tail(n: usize) -> Option<String> {
    let log = venti::util::paths::state_dir().join("venti.log");
    let content = fs::read_to_string(log).ok()?;
    let tail: Vec<&str> = content.lines().rev().take(n).collect();
    Some(tail.into_iter().rev().collect::<Vec<_>>().join("\n"))
}

fn spawn_daemon(session_name: &str) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;

    // daemon stdout/stderr go to the daemon log (append) instead of null, so
    // startup failures (bad config, theme, socket bind) are diagnosable.
    let stdout = daemon_log().map(Stdio::from).unwrap_or_else(Stdio::null);
    let stderr = daemon_log().map(Stdio::from).unwrap_or_else(Stdio::null);

    // prefer setsid so the daemon survives the parent terminal/session going
    // away. fall back to a plain detached child when setsid is unavailable.
    let spawned = Command::new("setsid")
        .arg(&exe)
        .arg("server")
        .arg("--session")
        .arg(session_name)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn();

    if spawned.is_err() {
        let stdout = daemon_log().map(Stdio::from).unwrap_or_else(Stdio::null);
        let stderr = daemon_log().map(Stdio::from).unwrap_or_else(Stdio::null);
        Command::new(&exe)
            .arg("server")
            .arg("--session")
            .arg(session_name)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()?;
    }

    Ok(())
}
