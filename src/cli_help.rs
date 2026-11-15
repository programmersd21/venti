//! design system for venti's cli output.
//!
//! minimalist, lowercase, one accent family (catppuccin mocha truecolor),
//! semantic roles, computed alignment, generous whitespace. no banners,
//! no rainbow, no emojis. plain text when piped or NO_COLOR is set.

use std::io::IsTerminal;
use unicode_width::UnicodeWidthStr;

pub const TEXT: &str = "\x1b[38;2;205;214;244m";
pub const SUBTEXT: &str = "\x1b[38;2;166;173;200m";
pub const OVERLAY: &str = "\x1b[38;2;108;112;134m";
pub const RED: &str = "\x1b[38;2;243;139;168m";
pub const PEACH: &str = "\x1b[38;2;250;179;135m";
pub const YELLOW: &str = "\x1b[38;2;249;226;175m";
pub const GREEN: &str = "\x1b[38;2;166;227;161m";
pub const TEAL: &str = "\x1b[38;2;148;226;213m";
pub const BLUE: &str = "\x1b[38;2;137;180;250m";
pub const MAUVE: &str = "\x1b[38;2;203;166;247m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const RESET: &str = "\x1b[0m";

/// color is on for a tty unless no_color/clicolor=0; clicolor_force wins.
pub fn color_enabled() -> bool {
    if let Ok(v) = std::env::var("CLICOLOR_FORCE") {
        return v != "0";
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Ok(v) = std::env::var("CLICOLOR")
        && v == "0"
    {
        return false;
    }
    std::io::stdout().is_terminal()
}

pub fn paint(color: &str, s: &str) -> String {
    if color_enabled() {
        format!("{color}{s}{RESET}")
    } else {
        s.to_string()
    }
}

pub fn paint_bold(color: &str, s: &str) -> String {
    if color_enabled() {
        format!("{BOLD}{color}{s}{RESET}")
    } else {
        s.to_string()
    }
}

fn w(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// aligned rows: each row is (painted cell, visible width of cell, painted rest).
/// pads every cell to the widest cell + `gap` so nothing ever overlaps.
pub fn aligned_rows(indent: usize, gap: usize, rows: &[(String, usize, String)]) -> Vec<String> {
    let col = rows.iter().map(|r| r.1).max().unwrap_or(0);
    rows.iter()
        .map(|(cell, vis, rest)| {
            let pad = col.saturating_sub(*vis) + gap;
            format!("{}{}{}{}", " ".repeat(indent), cell, " ".repeat(pad), rest)
        })
        .collect()
}

fn heading(out: &mut Vec<String>, name: &str) {
    out.push(format!("  {}", paint_bold(MAUVE, name)));
}

fn wordmark(path: &[&str]) -> String {
    let mut s = paint_bold(TEXT, "venti");
    for seg in path {
        s.push(' ');
        s.push_str(&paint_bold(BLUE, seg));
    }
    s
}

fn tagline(out: &mut Vec<String>, path: &[&str], text: &str) {
    out.push(format!("  {}", wordmark(path)));
    out.push(format!("  {}", paint(SUBTEXT, text)));
    out.push(String::new());
}

fn footer(out: &mut Vec<String>, text: &str) {
    out.push(String::new());
    out.push(format!("  {}", paint(OVERLAY, text)));
}

struct Seg {
    painted: String,
    vis: usize,
}

impl Seg {
    fn blue(s: &str) -> Self {
        Seg {
            painted: paint_bold(BLUE, s),
            vis: w(s),
        }
    }
    fn teal(s: &str) -> Self {
        Seg {
            painted: paint_bold(TEAL, s),
            vis: w(s),
        }
    }
    fn peach(s: &str) -> Self {
        Seg {
            painted: paint(PEACH, s),
            vis: w(s),
        }
    }
    fn plain(s: &str) -> Self {
        Seg {
            painted: s.to_string(),
            vis: w(s),
        }
    }
}

fn seg_line(segs: &[Seg]) -> (String, usize) {
    let mut painted = String::new();
    let mut vis = 0;
    for seg in segs {
        painted.push_str(&seg.painted);
        vis += seg.vis;
    }
    (painted, vis)
}

/// a flag cell like `-s, --session <name>` painted teal/peach
fn opt_cell(flag: &str, long: Option<&str>, meta: Option<&str>) -> (String, usize) {
    let mut segs = vec![Seg::teal(flag), Seg::plain(",")];
    if let Some(long) = long {
        segs.push(Seg::plain(" "));
        segs.push(Seg::teal(long));
    }
    if let Some(meta) = meta {
        segs.push(Seg::plain(" "));
        segs.push(Seg::peach(&format!("<{meta}>")));
    }
    seg_line(&segs)
}

fn opt_rows(out: &mut Vec<String>, items: &[(String, usize, String, Option<String>)]) {
    let col = items.iter().map(|i| i.1).max().unwrap_or(0);
    for (cell, vis, desc, ctx) in items {
        let pad = col.saturating_sub(*vis) + 3;
        let mut rest = paint(SUBTEXT, desc);
        if let Some(ctx) = ctx {
            rest.push_str(&format!("  {}", paint(OVERLAY, &format!("[{ctx}]"))));
        }
        out.push(format!("    {}{}{}", cell, " ".repeat(pad), rest));
    }
}

fn help_flag_row() -> (String, usize, String, Option<String>) {
    let (cell, vis) = opt_cell("-h", Some("--help"), None);
    (cell, vis, "print help".into(), None)
}

fn example_rows(out: &mut Vec<String>, items: &[(&str, &str)]) {
    let col = items.iter().map(|(cmd, _)| w(cmd)).max().unwrap_or(0);
    for (cmd, desc) in items {
        let pad = col.saturating_sub(w(cmd)) + 4;
        out.push(format!(
            "    {}{}{}",
            paint_bold(BLUE, cmd),
            " ".repeat(pad),
            paint(SUBTEXT, desc)
        ));
    }
}

fn kv_rows(out: &mut Vec<String>, items: &[(&str, String)]) {
    let col = items.iter().map(|(k, _)| w(k)).max().unwrap_or(0);
    for (k, v) in items {
        let pad = col.saturating_sub(w(k)) + 3;
        out.push(format!("    {}{}{}", paint(OVERLAY, k), " ".repeat(pad), v));
    }
}

fn render(lines: Vec<String>) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

fn top_page() -> String {
    let mut o = Vec::new();
    tagline(&mut o, &[], "fast quiet terminal multiplexer");

    heading(&mut o, "usage");
    let usage: Vec<(String, usize, String)> = [
        (vec![Seg::blue("venti")], "attach default session"),
        (
            vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("new"),
                Seg::plain(" "),
                Seg::teal("-s"),
                Seg::plain(" "),
                Seg::peach("<name>"),
            ],
            "create and attach named session",
        ),
        (
            vec![Seg::blue("venti"), Seg::plain(" "), Seg::blue("ls")],
            "list sessions",
        ),
        (
            vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("attach"),
                Seg::plain(" "),
                Seg::teal("-t"),
                Seg::plain(" "),
                Seg::peach("<name>"),
            ],
            "attach to session",
        ),
    ]
    .into_iter()
    .map(|(segs, desc)| {
        let (painted, vis) = seg_line(&segs);
        (painted, vis, paint(SUBTEXT, desc))
    })
    .collect();
    o.extend(aligned_rows(4, 3, &usage));
    o.push(String::new());

    heading(&mut o, "commands");
    let cmds: Vec<(String, usize, String)> = [
        ("new", "create session"),
        ("attach", "attach to session"),
        ("ls", "list sessions"),
        ("kill", "kill session"),
        ("kill-server", "stop background daemon"),
        ("send", "send keys to pane"),
        ("run", "run command"),
        ("paste", "paste to pane"),
        ("config", "manage configuration"),
        ("doctor", "check terminal"),
        ("completions", "shell completions"),
        ("man", "manual page"),
    ]
    .into_iter()
    .map(|(name, desc)| (paint_bold(BLUE, name), w(name), paint(SUBTEXT, desc)))
    .collect();
    o.extend(aligned_rows(4, 3, &cmds));
    o.push(String::new());

    heading(&mut o, "inside venti");
    let dot = paint(OVERLAY, " · ");
    let key = |k: &str| paint_bold(TEAL, k);
    let desc = |d: &str| paint(SUBTEXT, d);
    let pair = |k: &str, d: &str| format!("{} {}", key(k), desc(d));
    o.push(format!(
        "    {}{}{}{}{}{}{}{}{}{}{}",
        pair("f1", "help"),
        dot,
        pair("f2", "settings"),
        dot,
        pair("f3", "split"),
        dot,
        pair("f4", "close"),
        dot,
        pair("f5", "zoom"),
        dot,
        pair("f6", "float"),
    ));
    o.push(format!(
        "    {}{}{}{}{}{}{}{}{}{}{}",
        pair("f7", "scratch"),
        dot,
        pair("f8", "search"),
        dot,
        pair("f9", "copy"),
        dot,
        pair("f10", "resize"),
        dot,
        pair("f11", "palette"),
        dot,
        pair("f12", "theme"),
    ));
    o.push(format!(
        "    {}{}{}",
        pair("alt+h/j/k/l", "focus"),
        dot,
        pair("alt+shift+hjkl", "swap"),
    ));
    o.push(format!(
        "    {}{}{}{}{}",
        pair("alt+n", "new workspace"),
        dot,
        pair("alt+1..9", "goto ws"),
        dot,
        pair("alt+←→", "prev/next ws"),
    ));
    o.push(String::new());

    heading(&mut o, "config");
    kv_rows(
        &mut o,
        &[
            ("config", paint(TEXT, "~/.config/venti/config.toml")),
            ("log", paint(TEXT, "~/.local/state/venti/venti.log")),
        ],
    );

    footer(&mut o, "venti man · mit");
    render(o)
}

struct SubPage {
    tagline: &'static str,
    usage: Vec<Seg>,
    arguments: &'static [(&'static str, &'static str)],
    options: Vec<(String, usize, String, Option<String>)>,
    note: Option<&'static str>,
    examples: &'static [(&'static str, &'static str)],
    footer: &'static str,
}

fn sub_page(path: &[&str], page: SubPage) -> String {
    let mut o = Vec::new();
    tagline(&mut o, path, page.tagline);

    heading(&mut o, "usage");
    let (painted, _) = seg_line(&page.usage);
    o.push(format!("    {painted}"));
    o.push(String::new());

    if !page.arguments.is_empty() {
        heading(&mut o, "arguments");
        let col = page.arguments.iter().map(|a| w(a.0)).max().unwrap_or(0);
        for (name, desc) in page.arguments {
            let pad = col.saturating_sub(w(name)) + 3;
            o.push(format!(
                "    {}{}{}",
                paint(PEACH, &format!("<{name}>")),
                " ".repeat(pad),
                paint(SUBTEXT, desc)
            ));
        }
        o.push(String::new());
    }

    heading(&mut o, "options");
    opt_rows(&mut o, &page.options);

    if let Some(note) = page.note {
        o.push(String::new());
        o.push(format!("    {}", paint(OVERLAY, note)));
    }

    if !page.examples.is_empty() {
        o.push(String::new());
        heading(&mut o, "examples");
        example_rows(&mut o, page.examples);
    }

    footer(&mut o, page.footer);
    render(o)
}

fn page_new() -> String {
    let (s, sv) = opt_cell("-s", Some("--session"), Some("name"));
    sub_page(
        &["new"],
        SubPage {
            tagline: "create and attach to new session",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("new"),
                Seg::plain(" "),
                Seg::plain("["),
                Seg::teal("-s"),
                Seg::plain(" "),
                Seg::peach("<name>"),
                Seg::plain("]"),
            ],
            arguments: &[],
            options: vec![
                (
                    s,
                    sv,
                    "session name to create".into(),
                    Some("default: default".into()),
                ),
                help_flag_row(),
            ],
            note: None,
            examples: &[
                ("venti new", "start default session"),
                ("venti new -s work", "named session"),
            ],
            footer: "venti --help · mit",
        },
    )
}

fn page_attach() -> String {
    let (t, tv) = opt_cell("-t", Some("--target"), Some("name"));
    let (d, dv) = opt_cell("-d", Some("--detach-others"), None);
    sub_page(
        &["attach"],
        SubPage {
            tagline: "attach to existing session",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("attach"),
                Seg::plain(" "),
                Seg::plain("["),
                Seg::teal("-t"),
                Seg::plain(" "),
                Seg::peach("<name>"),
                Seg::plain("] ["),
                Seg::teal("-d"),
                Seg::plain("]"),
            ],
            arguments: &[],
            options: vec![
                (
                    t,
                    tv,
                    "session name to attach to".into(),
                    Some("default: default".into()),
                ),
                (d, dv, "disconnect other clients".into(), None),
                help_flag_row(),
            ],
            note: None,
            examples: &[
                ("venti attach -t work", "attach to work"),
                ("venti attach -d -t work", "kick off other clients"),
            ],
            footer: "venti --help · mit",
        },
    )
}

fn page_ls() -> String {
    sub_page(
        &["ls"],
        SubPage {
            tagline: "list all running sessions",
            usage: vec![Seg::blue("venti"), Seg::plain(" "), Seg::blue("ls")],
            arguments: &[],
            options: vec![help_flag_row()],
            note: None,
            examples: &[("venti ls", "list sessions")],
            footer: "venti --help · mit",
        },
    )
}

fn page_kill() -> String {
    let (t, tv) = opt_cell("-t", Some("--target"), Some("name"));
    sub_page(
        &["kill"],
        SubPage {
            tagline: "kill a session",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("kill"),
                Seg::plain(" "),
                Seg::teal("-t"),
                Seg::plain(" "),
                Seg::peach("<name>"),
            ],
            arguments: &[],
            options: vec![
                (
                    t,
                    tv,
                    "session name to terminate".into(),
                    Some("required".into()),
                ),
                help_flag_row(),
            ],
            note: None,
            examples: &[("venti kill -t work", "terminate work")],
            footer: "venti --help · mit",
        },
    )
}

fn page_kill_server() -> String {
    sub_page(
        &["kill-server"],
        SubPage {
            tagline: "stop the background daemon",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("kill-server"),
            ],
            arguments: &[],
            options: vec![help_flag_row()],
            note: Some("kills every session and removes stale sockets"),
            examples: &[("venti kill-server", "stop the daemon")],
            footer: "venti --help · mit",
        },
    )
}

fn page_send() -> String {
    let (t, tv) = opt_cell("-t", Some("--target"), Some("name"));
    sub_page(
        &["send"],
        SubPage {
            tagline: "send keys or commands to pane",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("send"),
                Seg::plain(" "),
                Seg::plain("["),
                Seg::teal("-t"),
                Seg::plain(" "),
                Seg::peach("<name>"),
                Seg::plain("] "),
                Seg::peach("<keys>"),
            ],
            arguments: &[("keys", "keys or shell command to send to focused pane")],
            options: vec![
                (
                    t,
                    tv,
                    "session name to send to".into(),
                    Some("default: default".into()),
                ),
                help_flag_row(),
            ],
            note: None,
            examples: &[
                ("venti send -t work \"ls -la\"", "run a command"),
                ("venti send -t work \":q\"", "send vim quit"),
            ],
            footer: "venti --help · mit",
        },
    )
}

fn page_run() -> String {
    let (t, tv) = opt_cell("-t", Some("--target"), Some("name"));
    sub_page(
        &["run"],
        SubPage {
            tagline: "run a command against the session (same as typing it in the palette)",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("run"),
                Seg::plain(" "),
                Seg::plain("["),
                Seg::teal("-t"),
                Seg::plain(" "),
                Seg::peach("<name>"),
                Seg::plain("] "),
                Seg::peach("<command>"),
                Seg::plain(" [args…]"),
            ],
            arguments: &[(
                "command",
                "command to execute, e.g. 'workspace_goto 2' or 'close_workspace 1 2 3'",
            )],
            options: vec![
                (
                    t,
                    tv,
                    "session name to target".into(),
                    Some("default: default".into()),
                ),
                help_flag_row(),
            ],
            note: None,
            examples: &[
                ("venti run workspace_goto 2", "switch to workspace 2"),
                ("venti run close_workspace 1 2 3", "close three workspaces"),
                ("venti run move_to_workspace 4", "move pane to workspace 4"),
                ("venti run help", "open the help screen"),
            ],
            footer: "venti --help · mit",
        },
    )
}

fn page_paste() -> String {
    let (t, tv) = opt_cell("-t", Some("--target"), Some("name"));
    let (s, sv) = seg_line(&[Seg::teal("--stdin")]);
    sub_page(
        &["paste"],
        SubPage {
            tagline: "paste to focused pane",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("paste"),
                Seg::plain(" "),
                Seg::plain("["),
                Seg::teal("-t"),
                Seg::plain(" "),
                Seg::peach("<name>"),
                Seg::plain("] ["),
                Seg::teal("--stdin"),
                Seg::plain("]"),
            ],
            arguments: &[],
            options: vec![
                (
                    t,
                    tv,
                    "session name to paste to".into(),
                    Some("default: default".into()),
                ),
                (
                    s,
                    sv,
                    "read paste content from stdin instead of clipboard".into(),
                    None,
                ),
                help_flag_row(),
            ],
            note: None,
            examples: &[
                ("venti paste", "paste clipboard"),
                ("echo hi | venti paste --stdin", "paste from stdin"),
            ],
            footer: "venti --help · mit",
        },
    )
}

fn page_config() -> String {
    let mut o = Vec::new();
    tagline(&mut o, &["config"], "manage venti configuration");

    heading(&mut o, "usage");
    let (usage, _) = seg_line(&[
        Seg::blue("venti"),
        Seg::plain(" "),
        Seg::blue("config"),
        Seg::plain(" "),
        Seg::peach("<command>"),
    ]);
    o.push(format!("    {usage}"));
    o.push(String::new());

    heading(&mut o, "commands");
    let cmds: Vec<(String, usize, String)> = [
        ("init", "create default config"),
        ("check", "check config file for errors"),
        ("path", "show config file path"),
    ]
    .into_iter()
    .map(|(name, desc)| (paint_bold(BLUE, name), w(name), paint(SUBTEXT, desc)))
    .collect();
    o.extend(aligned_rows(4, 3, &cmds));
    o.push(String::new());

    heading(&mut o, "options");
    opt_rows(&mut o, &[help_flag_row()]);

    o.push(String::new());
    heading(&mut o, "examples");
    example_rows(
        &mut o,
        &[
            ("venti config init", "create ~/.config/venti/config.toml"),
            ("venti config check", "validate config"),
        ],
    );

    footer(&mut o, "venti --help · mit");
    render(o)
}

fn page_config_init() -> String {
    sub_page(
        &["config", "init"],
        SubPage {
            tagline: "create default config",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("config"),
                Seg::plain(" "),
                Seg::blue("init"),
            ],
            arguments: &[],
            options: vec![help_flag_row()],
            note: Some("writes ~/.config/venti/config.toml with commented defaults"),
            examples: &[("venti config init", "create config")],
            footer: "venti config --help · mit",
        },
    )
}

fn page_config_check() -> String {
    sub_page(
        &["config", "check"],
        SubPage {
            tagline: "check config file for errors",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("config"),
                Seg::plain(" "),
                Seg::blue("check"),
            ],
            arguments: &[],
            options: vec![help_flag_row()],
            note: None,
            examples: &[("venti config check", "validate config")],
            footer: "venti config --help · mit",
        },
    )
}

fn page_config_path() -> String {
    sub_page(
        &["config", "path"],
        SubPage {
            tagline: "show config file path",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("config"),
                Seg::plain(" "),
                Seg::blue("path"),
            ],
            arguments: &[],
            options: vec![help_flag_row()],
            note: None,
            examples: &[("venti config path", "print the path")],
            footer: "venti config --help · mit",
        },
    )
}

fn page_doctor() -> String {
    sub_page(
        &["doctor"],
        SubPage {
            tagline: "check terminal: term, colors, graphics",
            usage: vec![Seg::blue("venti"), Seg::plain(" "), Seg::blue("doctor")],
            arguments: &[],
            options: vec![help_flag_row()],
            note: None,
            examples: &[("venti doctor", "check terminal capabilities")],
            footer: "venti --help · mit",
        },
    )
}

fn page_completions() -> String {
    sub_page(
        &["completions"],
        SubPage {
            tagline: "output shell completions",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("completions"),
                Seg::plain(" "),
                Seg::peach("<shell>"),
            ],
            arguments: &[("shell", "bash · fish · zsh · elvish")],
            options: vec![help_flag_row()],
            note: None,
            examples: &[
                ("venti completions zsh > _venti", "zsh completion"),
                ("venti completions fish > venti.fish", "fish completion"),
            ],
            footer: "venti --help · mit",
        },
    )
}

fn page_man() -> String {
    sub_page(
        &["man"],
        SubPage {
            tagline: "display man page",
            usage: vec![Seg::blue("venti"), Seg::plain(" "), Seg::blue("man")],
            arguments: &[],
            options: vec![help_flag_row()],
            note: None,
            examples: &[("venti man", "print the manual")],
            footer: "venti --help · mit",
        },
    )
}

fn page_server() -> String {
    let (s, sv) = opt_cell("-s", Some("--session"), Some("name"));
    sub_page(
        &["server"],
        SubPage {
            tagline: "internal daemon server mode",
            usage: vec![
                Seg::blue("venti"),
                Seg::plain(" "),
                Seg::blue("server"),
                Seg::plain(" "),
                Seg::plain("["),
                Seg::teal("-s"),
                Seg::plain(" "),
                Seg::peach("<name>"),
                Seg::plain("]"),
            ],
            arguments: &[],
            options: vec![
                (
                    s,
                    sv,
                    "session name to serve".into(),
                    Some("default: default".into()),
                ),
                help_flag_row(),
            ],
            note: Some("internal · spawned by venti · not for interactive use"),
            examples: &[],
            footer: "venti --help · mit",
        },
    )
}

const KNOWN: &[&str] = &[
    "new",
    "attach",
    "ls",
    "kill",
    "kill-server",
    "send",
    "run",
    "paste",
    "config",
    "doctor",
    "completions",
    "man",
    "server",
];

pub fn help_for(path: &[&str]) -> Option<String> {
    Some(match path {
        [] => top_page(),
        ["new"] => page_new(),
        ["attach"] => page_attach(),
        ["ls"] => page_ls(),
        ["kill"] => page_kill(),
        ["kill-server"] => page_kill_server(),
        ["send"] => page_send(),
        ["run"] => page_run(),
        ["paste"] => page_paste(),
        ["config"] => page_config(),
        ["config", "init"] => page_config_init(),
        ["config", "check"] => page_config_check(),
        ["config", "path"] => page_config_path(),
        ["doctor"] => page_doctor(),
        ["completions"] => page_completions(),
        ["man"] => page_man(),
        ["server"] => page_server(),
        _ => return None,
    })
}

pub fn version_line() -> String {
    format!(
        "  {} {}\n",
        paint_bold(TEXT, "venti"),
        paint(OVERLAY, env!("CARGO_PKG_VERSION"))
    )
}

/// routes `venti ... --help` / `venti help ...` / `-V` before clap sees argv.
/// returns the page to print, or none to let clap parse normally.
pub fn intercept(args: &[String]) -> Option<String> {
    let mut help = false;
    let mut version = false;
    let mut tokens: Vec<&str> = Vec::new();
    for arg in args {
        if arg == "--" {
            break;
        }
        match arg.as_str() {
            "-h" | "--help" => help = true,
            "-V" | "--version" => version = true,
            _ if !arg.starts_with('-') => tokens.push(arg.as_str()),
            _ => {}
        }
    }

    let requested = help || version || tokens.first() == Some(&"help");
    if !requested {
        return None;
    }

    // positional route: skip a leading `help`, then known command chain
    let i = usize::from(tokens.first() == Some(&"help"));
    let mut path: Vec<&str> = Vec::new();
    if let Some(&cmd) = tokens.get(i)
        && KNOWN.contains(&cmd)
    {
        path.push(cmd);
        if cmd == "config"
            && let Some(&sub) = tokens.get(i + 1)
            && matches!(sub, "init" | "check" | "path")
        {
            path.push(sub);
        }
    }

    if version && !help {
        return Some(version_line());
    }
    help_for(&path)
}

pub fn success(msg: &str) -> String {
    format!("  {} {}", paint_bold(GREEN, "✓"), paint(TEXT, msg))
}

pub fn error(msg: &str) -> String {
    format!("  {} {}", paint_bold(RED, "✗"), paint(TEXT, msg))
}

pub fn warn(msg: &str) -> String {
    format!("  {} {}", paint_bold(YELLOW, "⚠"), paint(TEXT, msg))
}

pub fn info(msg: &str) -> String {
    format!("  {} {}", paint_bold(BLUE, "→"), paint(TEXT, msg))
}

pub fn dim_line(msg: &str) -> String {
    format!("  {}", paint(OVERLAY, msg))
}
