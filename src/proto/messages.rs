use serde::{Deserialize, Serialize};

/// cursor display shape style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorShape {
    Default,
    Block,
    Underline,
    Bar,
}

/// kind of graphics sequence carried by a graphics command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GfxFormat {
    Sixel,
    Kitty,
    Iterm2,
}

/// a graphics protocol escape sequence (sixel, kitty, or iterm2) anchored to a
/// cell position, forwarded verbatim to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GfxCmd {
    pub row: u16,
    pub col: u16,
    pub format: GfxFormat,
    pub seq: u64,
    pub data: Vec<u8>,
}

/// cell color as transmitted to the client.
///
/// `indexed` carries an xterm 256-color palette index; indices 0-15 are the
/// terminal's own ansi colors. `rgb` carries 24-bit true color. `default`
/// resets to the terminal's default cell color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtoColor {
    Default,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

/// compact representation of a styled terminal cell for screen diff updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellDiff {
    pub x: u16,
    pub y: u16,
    pub symbol: String,
    pub fg: ProtoColor,
    pub bg: ProtoColor,
    pub underline_color: ProtoColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub strike: bool,
}

/// incremental or complete screen buffer update sent from server to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenUpdate {
    pub cols: u16,
    pub rows: u16,
    pub cursor_pos: Option<(u16, u16)>,
    pub cursor_visible: bool,
    pub cursor_shape: CursorShape,
    pub cells: Vec<CellDiff>,
    pub full_redraw: bool,
    /// graphics escape sequences to draw after the cell diff (sixel, kitty,
    /// iterm2). emitted after cells so images draw on top.
    pub gfx: Vec<GfxCmd>,
    /// when set, the client emits a DECSTR soft reset before painting, so
    /// stale modes/attributes from an exited application are dropped. must NOT
    /// be RIS: RIS switches the host terminal back to the main screen and wipes
    /// scrollback, leaving venti's frame painted on top of the user's shell
    /// after exit. the server sets this whenever a pane's alt screen exits.
    #[serde(default)]
    pub reset: bool,
}

/// mouse event button kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButtonKind {
    Left,
    Right,
    Middle,
    ScrollUp,
    ScrollDown,
}

/// mouse event action kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseActionKind {
    Down,
    Up,
    Drag,
    Moved,
}

/// mouse event data sent from client to server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseEventMsg {
    pub col: u16,
    pub row: u16,
    pub button: MouseButtonKind,
    pub action: MouseActionKind,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// client disconnect reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitReason {
    Detached,
    Killed,
    Error(String),
}

/// handshake message sent by client upon socket connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloMsg {
    pub protocol: u16,
    pub term: String,
    pub colorterm: Option<String>,
    pub size: (u16, u16),
    pub env: Vec<(String, String)>,
    /// cell pixel size reported by the outer terminal (CSI 16t), if it replied.
    /// kitty, foot and xterm all report this; apps rely on it to size images.
    #[serde(default)]
    pub cell_pixel: Option<(u16, u16)>,
}

/// message stream sent from client to server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello(HelloMsg),
    Input(Vec<u8>),
    Mouse(MouseEventMsg),
    Resize(u16, u16),
    Paste(Vec<u8>),
    Command(String),
    Detach,
}

/// message stream sent from server to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMsg {
    HelloAck {
        session: String,
        /// graphics capability of the outer terminal, so the client does not
        /// have to guess from its own environment (which may be wrong when
        /// venti is started by a different terminal than the client).
        graphics: u8,
    },
    Update(ScreenUpdate),
    SetTitle(String),
    Clipboard(Vec<u8>),
    Exit(ExitReason),
    /// server asks each client to write these raw bytes to its own stdout.
    /// used for terminal-specific escape sequences (osc 4/10/11) that the
    /// pane inside venti would otherwise swallow via its vt100 grid.
    RawOutput(Vec<u8>),
}
