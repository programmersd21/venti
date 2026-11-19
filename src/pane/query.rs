use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

/// bit flags for graphics capability, shared between the server and each pane's
/// vt100 query responder. panes keep the same probe the client Hello writes,
/// so a pane spawned before the client attaches upgrades from "unknown" to the
/// attached terminal's real capabilities instead of answering "unsupported"
/// forever.
const SIXEL_BIT: u8 = 1 << 0;
const KITTY_BIT: u8 = 1 << 1;

/// shared graphics-capability snapshot, read on every probe answer.
pub type GraphicsProbe = Arc<AtomicU8>;

/// encodes a capability bitset for a terminal graphics level.
pub fn graphics_code(graphics: crate::util::outer_term::OuterGraphics) -> u8 {
    match graphics {
        crate::util::outer_term::OuterGraphics::Kitty => KITTY_BIT,
        crate::util::outer_term::OuterGraphics::Sixel => SIXEL_BIT,
        crate::util::outer_term::OuterGraphics::Tmux
        | crate::util::outer_term::OuterGraphics::Unknown => 0,
    }
}

/// creates an independent probe initialized to the given capability.
pub fn new_graphics_probe(graphics: crate::util::outer_term::OuterGraphics) -> GraphicsProbe {
    Arc::new(AtomicU8::new(graphics_code(graphics)))
}

/// terminal state used to answer position/size queries truthfully.
/// all values are 0-based; cursor reports are emitted 1-based.
#[derive(Debug, Clone, Copy)]
pub struct ScreenState {
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub rows: u16,
    pub cols: u16,
}

/// marker for a pending OSC query; payload accumulates in `osc_buf` until a
/// terminator arrives (BEL or `ESC \`).
#[derive(Debug)]
struct PendingOsc;

/// detects terminal query escape sequences split across read chunks and responds via pty master.
#[derive(Debug)]
pub struct QueryResponder {
    buf: Vec<u8>,
    string_state: Option<u8>,
    string_esc: bool,
    string_is_apc: bool,
    apc_overflow: bool,
    /// set when an OSC introducer (`ESC ]`) is seen; holds the collected
    /// payload until BEL or ST arrives.
    osc_state: Option<PendingOsc>,
    osc_buf: Vec<u8>,
    osc_esc: bool,
    /// shared with the pane: updated when a client attaches, so a pane spawned
    /// before the first attach gets the attached terminal's real capabilities.
    probe: GraphicsProbe,
    /// trusted terminal name returned for XTVERSION queries.
    xtversion: &'static str,
    /// version string paired with `xtversion` for the XTVERSION response.
    xtversion_ver: &'static str,
}

const MAX_QUERY_BUF: usize = 4096;

impl QueryResponder {
    fn sixel(&self) -> bool {
        self.probe.load(Ordering::Relaxed) & SIXEL_BIT != 0
    }

    fn kitty(&self) -> bool {
        self.probe.load(Ordering::Relaxed) & KITTY_BIT != 0
    }

    fn build(
        xtversion_term: &str,
        graphics: crate::util::outer_term::OuterGraphics,
        probe: GraphicsProbe,
    ) -> Self {
        let (name, ver) = crate::util::outer_term::xtversion_identity_for(xtversion_term, graphics);
        Self {
            buf: Vec::with_capacity(256),
            string_state: None,
            string_esc: false,
            string_is_apc: false,
            apc_overflow: false,
            osc_state: None,
            osc_buf: Vec::with_capacity(32),
            osc_esc: false,
            probe,
            xtversion: name,
            xtversion_ver: ver,
        }
    }

    pub fn new() -> Self {
        let term = std::env::var("TERM").unwrap_or_default();
        let graphics = crate::util::outer_term::detect();
        Self::build(&term, graphics, new_graphics_probe(graphics))
    }

    /// constructs a responder with explicit graphics capabilities (tests).
    pub fn with_graphics_support(sixel: bool, kitty: bool) -> Self {
        let (name, ver) = if kitty {
            ("kitty", "0.36.0")
        } else if sixel {
            ("foot", "1.18.0")
        } else {
            ("venti", env!("CARGO_PKG_VERSION"))
        };
        let bits = (sixel as u8) | ((kitty as u8) << 1);
        Self::probe_build(name, ver, bits)
    }

    fn probe_build(name: &'static str, ver: &'static str, bits: u8) -> Self {
        Self {
            buf: Vec::with_capacity(256),
            string_state: None,
            string_esc: false,
            string_is_apc: false,
            apc_overflow: false,
            osc_state: None,
            osc_buf: Vec::with_capacity(32),
            osc_esc: false,
            probe: Arc::new(AtomicU8::new(bits)),
            xtversion: name,
            xtversion_ver: ver,
        }
    }

    /// constructs a responder sharing a probe the caller keeps and can update.
    pub fn with_probe(
        term: &str,
        graphics: crate::util::outer_term::OuterGraphics,
        probe: GraphicsProbe,
    ) -> Self {
        Self::build(term, graphics, probe)
    }

    /// constructs a responder using the attached client's terminal identity.
    pub fn for_outer_terminal(
        term: &str,
        graphics: crate::util::outer_term::OuterGraphics,
    ) -> Self {
        Self::with_probe(term, graphics, new_graphics_probe(graphics))
    }

    /// feeds bytes, checks for known terminal query sequences, and writes appropriate responses back.
    pub fn process_queries<W: Write + ?Sized>(
        &mut self,
        bytes: &[u8],
        pty_writer: &mut W,
        screen: Option<ScreenState>,
    ) -> std::io::Result<()> {
        for &b in bytes {
            if self.osc_state.is_some() {
                self.osc_write_byte(b, pty_writer)?;
                continue;
            }
            if let Some(terminator) = self.string_state {
                if b == terminator || (self.string_esc && b == b'\\') {
                    if self.string_is_apc && !self.apc_overflow {
                        if b == b'\\' && self.string_esc {
                            self.buf.pop();
                        }
                        self.respond_kitty_query(&self.buf, pty_writer)?;
                    }
                    self.string_state = None;
                    self.string_esc = false;
                    self.string_is_apc = false;
                    self.apc_overflow = false;
                    self.buf.clear();
                } else {
                    self.string_esc = b == 0x1b;
                    if self.string_is_apc && !self.apc_overflow {
                        self.buf.push(b);
                        if self.buf.len() > MAX_QUERY_BUF {
                            self.buf.clear();
                            self.apc_overflow = true;
                        }
                    }
                }
                continue;
            }
            self.buf.push(b);
            if self.buf.len() > MAX_QUERY_BUF {
                let overflow = self.buf.len() - MAX_QUERY_BUF;
                self.buf.drain(..overflow);
            }

            // OSC queries we can answer truthfully: default fg/bg color and kitty's
            // text probe. everything else is swallowed until BEL or ST so apps
            // that block on a reply still proceed instead of hanging forever.
            if self.buf.ends_with(b"\x1b]") {
                self.osc_state = Some(PendingOsc);
                self.string_esc = false;
                self.buf.clear();
                continue;
            }
            if self.buf.ends_with(b"\x1bP") {
                self.string_state = Some(0x07);
                self.string_esc = false;
                self.buf.clear();
                continue;
            }
            if self.buf.ends_with(b"\x1b_") {
                self.string_state = Some(0x07);
                self.string_esc = false;
                self.string_is_apc = true;
                self.apc_overflow = false;
                self.buf.clear();
                continue;
            }

            // csi c / csi 0 c: primary device attributes. Only claim sixel when
            // the outer terminal (or the one behind tmux) actually supports it:
            // programs otherwise emit sixel into terminals that render it as garbage.
            if self.buf.ends_with(b"\x1b[c") || self.buf.ends_with(b"\x1b[0c") {
                let mut da1: Vec<u8> = b"\x1b[?63;1;2".to_vec();
                if self.sixel() {
                    da1.extend_from_slice(b";4");
                }
                if self.kitty() {
                    da1.extend_from_slice(b";52");
                }
                da1.extend_from_slice(b";22c");
                pty_writer.write_all(&da1)?;
                pty_writer.flush()?;
            }
            // csi > q: XTVERSION query, answer with our real identity so
            // apps that probe terminal type can see it (they must be tolerant
            // of unknown names; entering CLOSED kitty-compatible mode would
            // lie to applications that gate graphics on this)
            else if self.buf.ends_with(b"\x1b[>q") || self.buf.ends_with(b"\x1b[>0q") {
                let resp = format!("\x1bP>|{}({})\x1b\\", self.xtversion, self.xtversion_ver);
                pty_writer.write_all(resp.as_bytes())?;
                pty_writer.flush()?;
            }
            // csi > c / csi > 0 c: secondary device attributes -> vt220, firmware version 1, rom 0
            else if self.buf.ends_with(b"\x1b[>c") || self.buf.ends_with(b"\x1b[>0c") {
                pty_writer.write_all(b"\x1b[>1;1;0c")?;
                pty_writer.flush()?;
            }
            // csi 5 n: device status report -> status ok (\x1b[0n)
            else if self.buf.ends_with(b"\x1b[5n") {
                pty_writer.write_all(b"\x1b[0n")?;
                pty_writer.flush()?;
            }
            // csi 6 n: cursor position report, 1-based row;col
            else if self.buf.ends_with(b"\x1b[6n") {
                let (row, col) = screen
                    .map(|s| {
                        (
                            s.cursor_row.saturating_add(1),
                            s.cursor_col.saturating_add(1),
                        )
                    })
                    .unwrap_or((1, 1));
                let resp = format!("\x1b[{row};{col}R");
                pty_writer.write_all(resp.as_bytes())?;
                pty_writer.flush()?;
            }
            // csi 18 t: report terminal size in characters -> csi 8 ; rows ; cols t
            else if self.buf.ends_with(b"\x1b[18t") {
                let (rows, cols) = screen.map(|s| (s.rows, s.cols)).unwrap_or((24, 80));
                let resp = format!("\x1b[8;{rows};{cols}t");
                pty_writer.write_all(resp.as_bytes())?;
                pty_writer.flush()?;
            }
            // csi 14 t: report terminal size in pixels
            else if self.buf.ends_with(b"\x1b[14t") {
                let (rows, cols) = screen.map(|s| (s.rows, s.cols)).unwrap_or((24, 80));
                let (cell_w, cell_h) = crate::util::outer_term::cell_pixel_size();
                let (h, w) = (u32::from(rows) * cell_h, u32::from(cols) * cell_w);
                let resp = format!("\x1b[4;{h};{w}t");
                pty_writer.write_all(resp.as_bytes())?;
                pty_writer.flush()?;
            }
            // csi 16 t: report cell size in pixels
            else if self.buf.ends_with(b"\x1b[16t") {
                let (cell_w, cell_h) = crate::util::outer_term::cell_pixel_size();
                let resp = format!("\x1b[6;{};{}t", cell_h, cell_w);
                pty_writer.write_all(resp.as_bytes())?;
                pty_writer.flush()?;
            }
            // dcs $ q q st: DECRQM request for cursor shape, answer "block" (2)
            // formatted as: dcs ? Ps ; Pm $ y st where Ps=0 (request OK) and Pm=2 (block)
            // must handle before string_state clears the buffer for DCS sequences
            else if (self.buf.ends_with(b"\x1b\\") || self.buf.ends_with(b"\x07"))
                && self.buf.contains(&b'P')
                && self.buf.contains(&b'$')
                && self.buf.contains(&b'q')
                && let Some(start) = self.buf.windows(2).rposition(|w| w == b"\x1bP")
            {
                let term_len = if self.buf.ends_with(b"\x07") { 1 } else { 2 };
                let payload = &self.buf[start + 2..self.buf.len() - term_len];
                if payload == b"$q q" {
                    pty_writer.write_all(b"\x1bP1$r2 q\x1b\\")?;
                    pty_writer.flush()?;
                }
            }
            // csi ? 12 $ p: DECRQM request for cursor blink mode (12), answer "off" (0)
            else if self.buf.ends_with(b"\x1b[?12$p") {
                pty_writer.write_all(b"\x1b[?12;0$y")?;
                pty_writer.flush()?;
            }
            // sixel geometry and attributes query (csi ? 4 S / csi ? 2 S)
            else if self.buf.ends_with(b"\x1b[?4S") || self.buf.ends_with(b"\x1b[?2S") {
                // report sixel graphics available: color registers (1024), max width/height (1920x1080)
                if self.sixel() {
                    pty_writer.write_all(b"\x1b[?1;0;1024S")?;
                    pty_writer.flush()?;
                }
            }
            // kitty graphics protocol query: apc _g ... a=q ... st
            // answer only explicit queries; anything else is passed through untouched.
            else if (self.buf.ends_with(b"\x1b\\") || self.buf.ends_with(b"\x07"))
                && let Some(start) = self.buf.windows(2).rposition(|w| w == b"\x1b_")
            {
                let term_len = if self.buf.ends_with(b"\x07") { 1 } else { 2 };
                let payload = &self.buf[start + 2..self.buf.len() - term_len];
                if payload.starts_with(b"G") {
                    self.respond_kitty_query(payload, pty_writer)?;
                }
            }
        }
        Ok(())
    }

    /// answers OSC 10 (default foreground) and OSC 11 (default background)
    /// with the current theme colors so apps don't wait the full probe
    /// timeout before drawing (chafa blocks on this answer).
    fn osc_write_byte<W: Write + ?Sized>(
        &mut self,
        b: u8,
        pty_writer: &mut W,
    ) -> std::io::Result<()> {
        if self.osc_esc && b == b'\\' {
            self.osc_handle(pty_writer)?;
            return Ok(());
        }
        if b == 0x07 {
            self.osc_handle(pty_writer)?;
            return Ok(());
        }
        self.osc_esc = b == 0x1b;
        self.osc_buf.push(b);
        if self.osc_buf.len() > 256 {
            // malformed or very long osc; give up on answering it
            self.osc_state = None;
            self.osc_esc = false;
            self.osc_buf.clear();
        }
        Ok(())
    }

    fn osc_handle<W: Write + ?Sized>(&mut self, pty_writer: &mut W) -> std::io::Result<()> {
        let buf = std::mem::take(&mut self.osc_buf);
        self.osc_state = None;
        self.osc_esc = false;
        let query = buf.as_slice();
        let resp = if query.starts_with(b"10;?") {
            Some(b"\x1b]10;rgb:bfbf/bfbf/bfbf\x1b\\".to_vec())
        } else if query.starts_with(b"11;?") {
            Some(b"\x1b]11;rgb:2828/2828/2828\x1b\\".to_vec())
        } else if query.starts_with(b"12;?") {
            Some(b"\x1b]12;rgb:bfbf/bfbf/bfbf\x1b\\".to_vec())
        } else {
            None
        };
        if let Some(resp) = resp {
            pty_writer.write_all(&resp)?;
            pty_writer.flush()?;
        }
        Ok(())
    }

    fn respond_kitty_query<W: Write + ?Sized>(
        &self,
        payload: &[u8],
        pty_writer: &mut W,
    ) -> std::io::Result<()> {
        let Some(payload) = payload.strip_prefix(b"G") else {
            return Ok(());
        };
        let Ok(s) = std::str::from_utf8(payload) else {
            return Ok(());
        };
        // only explicit capability queries (a=q) may be answered; never respond to
        // transmit/display commands, which carry no query intent.
        let mut img_id = None;
        let mut is_query = false;
        let mut feature_action = None;
        let mut queried_unicode_placeholder = false;
        let mut queried_cursor_policy = false;

        for part in s.split(';') {
            for kv in part.split(',') {
                let Some((k, v)) = kv.split_once('=') else {
                    continue;
                };
                match k {
                    "a" => is_query = v == "q",
                    "i" => {
                        let Ok(id) = v.parse::<u32>() else {
                            return Ok(());
                        };
                        if id == 0 {
                            return Ok(());
                        }
                        img_id = Some(id);
                    }
                    "e" => feature_action = Some(v),
                    "U" if v == "1" => queried_unicode_placeholder = true,
                    "C" if v == "1" => queried_cursor_policy = true,
                    _ => {}
                }
            }
        }
        if !is_query {
            return Ok(());
        }
        let Some(img_id) = img_id else {
            return Ok(());
        };

        // if the outer terminal can't do kitty graphics (or it can't be known
        // from inside tmux), answer with an error so applications fall back
        // instead of vomiting megabytes of image bytes as text.
        if !self.kitty() {
            let resp = format!("\x1b_Gi={img_id};ENOENT:kitty graphics not available\x1b\\");
            pty_writer.write_all(resp.as_bytes())?;
            pty_writer.flush()?;
            return Ok(());
        }

        // build response echoing back supported feature flags.
        // u=1: unicode placeholder support (required for yazi/multiplexer image preview)
        // C=1: cursor movement policy (images don't move cursor)
        // e=T: Unicode placeholder confirmed
        let mut resp = format!("\x1b_Gi={img_id}");
        if feature_action == Some("T") {
            resp.push_str(",e=T");
            if queried_unicode_placeholder {
                resp.push_str(",U=1");
            }
            if queried_cursor_policy {
                resp.push_str(",C=1");
            }
        } else if self.xtversion == "kitty"
            && let Some(action) = feature_action.filter(|action| matches!(*action, "f" | "a" | "c"))
        {
            resp.push_str(&format!(",e={action}"));
        }
        resp.push_str(";OK\x1b\\");

        pty_writer.write_all(resp.as_bytes())?;
        pty_writer.flush()?;
        Ok(())
    }
}

impl Default for QueryResponder {
    fn default() -> Self {
        Self::new()
    }
}
