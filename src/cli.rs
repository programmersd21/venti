use clap::builder::styling::{Color, RgbColor, Style, Styles};
use clap::{Args, Parser, Subcommand, ValueEnum};

const MAUVE: RgbColor = RgbColor(203, 166, 247);
const TEAL: RgbColor = RgbColor(148, 226, 213);
const PEACH: RgbColor = RgbColor(250, 179, 135);
const GREEN: RgbColor = RgbColor(166, 227, 161);
const RED: RgbColor = RgbColor(243, 139, 168);
const YELLOW: RgbColor = RgbColor(249, 226, 175);
const OVERLAY: RgbColor = RgbColor(108, 112, 134);

/// clap fallback styling for errors and any clap-rendered output.
/// the full help pages are hand-rendered in cli_help; this keeps clap
/// internals on the same palette.
pub const VENTI_STYLES: Styles = Styles::styled()
    .header(Style::new().fg_color(Some(Color::Rgb(MAUVE))).bold())
    .usage(Style::new().fg_color(Some(Color::Rgb(MAUVE))).bold())
    .literal(Style::new().fg_color(Some(Color::Rgb(TEAL))).bold())
    .placeholder(Style::new().fg_color(Some(Color::Rgb(PEACH))))
    .error(Style::new().fg_color(Some(Color::Rgb(RED))).bold())
    .valid(Style::new().fg_color(Some(Color::Rgb(GREEN))))
    .invalid(Style::new().fg_color(Some(Color::Rgb(YELLOW))))
    .context(Style::new().fg_color(Some(Color::Rgb(OVERLAY))))
    .context_value(Style::new().fg_color(Some(Color::Rgb(OVERLAY))));

/// fast quiet terminal multiplexer
///
/// press f1 inside venti for help
#[derive(Debug, Parser)]
#[command(
    name = "venti",
    version,
    about = "fast quiet terminal multiplexer",
    disable_help_subcommand = true,
    styles = VENTI_STYLES,
    arg_required_else_help = false
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

/// available cli subcommands
#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// internal daemon server mode (hidden)
    #[command(hide = true)]
    Server(ServerArgs),
    /// create and attach to new session
    New(NewArgs),
    /// attach to existing session
    Attach(AttachArgs),
    /// list all running sessions
    Ls,
    /// kill a session
    Kill(KillArgs),
    /// stop the background daemon
    KillServer,
    /// send keys or commands to pane
    Send(SendArgs),
    /// run a command against the session (same as typing it in the palette)
    Run(RunArgs),
    /// paste to focused pane
    Paste(PasteArgs),
    /// manage venti configuration
    Config(ConfigArgs),
    /// check terminal: TERM, colors, graphics
    Doctor,
    /// output shell completions (bash|fish|zsh|elvish)
    Completions(CompletionsArgs),
    /// display man page
    Man,
}

/// server mode args
#[derive(Debug, Args)]
pub struct ServerArgs {
    /// session name to start server for
    #[arg(short = 's', long = "session", default_value = "default")]
    pub session: String,
}

/// new session args
#[derive(Debug, Args)]
pub struct NewArgs {
    /// session name to create (default: default)
    #[arg(short = 's', long = "session", default_value = "default")]
    pub session: String,
}

/// attach args
#[derive(Debug, Args)]
pub struct AttachArgs {
    /// session name to attach to (default: default)
    #[arg(short = 't', long = "target", default_value = "default")]
    pub target: String,
    /// detach all other clients from this session
    #[arg(short = 'd', long = "detach-others", help = "disconnect other clients")]
    pub detach_others: bool,
}

/// kill args
#[derive(Debug, Args)]
pub struct KillArgs {
    /// session name to terminate
    #[arg(short = 't', long = "target")]
    pub target: Option<String>,
}

/// send args
#[derive(Debug, Args)]
pub struct SendArgs {
    /// session name to send to (default: default)
    #[arg(short = 't', long = "target", default_value = "default")]
    pub target: String,
    /// keys or shell command to send to focused pane
    pub keys: Option<String>,
}

/// run command args: everything after `run` is the command line
#[derive(Debug, Args)]
pub struct RunArgs {
    /// session name to target (default: default)
    #[arg(short = 't', long = "target", default_value = "default")]
    pub target: String,
    /// the command to run, e.g. `venti run "workspace_goto 3"` or `venti run move_to_workspace 2`
    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

/// paste args
#[derive(Debug, Args)]
pub struct PasteArgs {
    /// session name to paste to (default: default)
    #[arg(short = 't', long = "target", default_value = "default")]
    pub target: String,
    /// read paste content from stdin instead of clipboard
    #[arg(long, help = "read from stdin instead of clipboard")]
    pub stdin: bool,
}

/// config subcommands
#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: Option<ConfigAction>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// create default ~/.config/venti/config.toml
    Init,
    /// check config file for errors
    Check,
    /// show config file path
    Path,
}

/// shell types for completion generation
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ShellKind {
    /// bash shell
    Bash,
    /// elvish shell
    Elvish,
    /// fish shell
    Fish,
    /// zsh shell
    Zsh,
}

/// shell completions generation args
#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// shell to generate completions for
    pub shell: Option<ShellKind>,
}
