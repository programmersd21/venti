use super::keys::{Key, KeyCode, Modifiers};
use crate::proto::{MouseActionKind, MouseButtonKind, MouseEventMsg};

/// parsed event from terminal input stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedInput {
    Key(Key),
    Mouse(MouseEventMsg),
    Paste(Vec<u8>),
    FocusGained,
    FocusLost,
    Raw(Vec<u8>),
}

/// state machine state for parsing terminal escape sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserState {
    Ground,
    Esc,
    Csi,
    Ss3,
    Osc,
    Apc,
    Dcs,
    Paste,
    DiscardPaste,
}

/// pure stateful terminal input byte parser.
pub struct KeyParser {
    state: ParserState,
    buf: Vec<u8>,
    paste_buf: Vec<u8>,
}

impl Default for KeyParser {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyParser {
    /// creates a new key parser.
    pub fn new() -> Self {
        Self {
            state: ParserState::Ground,
            buf: Vec::with_capacity(32),
            paste_buf: Vec::with_capacity(1024),
        }
    }

    /// feeds a chunk of bytes, returning all parsed input events in sequence.
    pub fn parse_bytes(&mut self, bytes: &[u8]) -> Vec<ParsedInput> {
        let mut results = Vec::new();
        for &b in bytes {
            self.feed_byte(b, &mut results);
        }
        results
    }

    /// reports whether a trailing escape byte is waiting for a continuation.
    pub fn has_pending_escape(&self) -> bool {
        self.state == ParserState::Esc && self.buf == [0x1b]
    }

    /// emits a lone escape after the caller's ambiguity timeout has elapsed.
    pub fn flush_pending_escape(&mut self) -> Option<ParsedInput> {
        if !self.has_pending_escape() {
            return None;
        }
        self.reset();
        Some(ParsedInput::Key(Key::plain(KeyCode::Esc)))
    }

    fn feed_byte(&mut self, b: u8, out: &mut Vec<ParsedInput>) {
        match self.state {
            ParserState::Ground => {
                // a non-ascii byte after a partial utf-8 sequence means the sequence is
                // invalid; drop the stale bytes before applying ascii rules.
                if b < 0x80 && !self.buf.is_empty() {
                    self.buf.clear();
                }
                if b == 0x1b {
                    self.state = ParserState::Esc;
                    self.buf.clear();
                    self.buf.push(b);
                } else if b == 0x7f {
                    out.push(ParsedInput::Key(Key::plain(KeyCode::Backspace)));
                } else if b == b'\r' || b == b'\n' {
                    out.push(ParsedInput::Key(Key::plain(KeyCode::Enter)));
                } else if b == b'\t' {
                    out.push(ParsedInput::Key(Key::plain(KeyCode::Tab)));
                } else if b == b' ' {
                    out.push(ParsedInput::Key(Key::plain(KeyCode::Space)));
                } else if b == 0 {
                    out.push(ParsedInput::Key(Key::ctrl(KeyCode::Space)));
                } else if (1..32).contains(&b) {
                    // control characters (ctrl+a through ctrl+z) plus the punctuation class
                    let ch = ctrl_punct_char(b).unwrap_or((b + b'a' - 1) as char);
                    out.push(ParsedInput::Key(Key::ctrl(KeyCode::Char(ch))));
                } else if (b as char).is_ascii() {
                    out.push(ParsedInput::Key(Key::plain(KeyCode::Char(b as char))));
                } else {
                    // multi-byte utf-8 in ground state
                    self.buf.push(b);
                    if let Ok(s) = std::str::from_utf8(&self.buf) {
                        for c in s.chars() {
                            out.push(ParsedInput::Key(Key::plain(KeyCode::Char(c))));
                        }
                        self.buf.clear();
                    } else if self.buf.len() >= 4 {
                        out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                    }
                }
            }
            ParserState::Esc => {
                self.buf.push(b);
                match b {
                    b'[' => {
                        self.state = ParserState::Csi;
                    }
                    b'O' => {
                        self.state = ParserState::Ss3;
                    }
                    b']' => {
                        self.state = ParserState::Osc;
                    }
                    b'_' => {
                        // APC: kitty's response to graphics queries (a=T, etc.)
                        self.state = ParserState::Apc;
                    }
                    b'P' => {
                        // DCS: terminal answers to DECRQM / XTGETTCAP / XTVERSION
                        self.state = ParserState::Dcs;
                    }
                    0x1b => {
                        out.push(ParsedInput::Key(Key::plain(KeyCode::Esc)));
                        self.buf.clear();
                        self.buf.push(b);
                    }
                    b'\r' | b'\n' => {
                        out.push(ParsedInput::Key(Key::alt(KeyCode::Enter)));
                        self.reset();
                    }
                    b'\t' => {
                        out.push(ParsedInput::Key(Key::alt(KeyCode::Tab)));
                        self.reset();
                    }
                    b' ' => {
                        out.push(ParsedInput::Key(Key::alt(KeyCode::Space)));
                        self.reset();
                    }
                    c if c.is_ascii_uppercase() => {
                        let modifiers = Modifiers {
                            ctrl: false,
                            alt: true,
                            shift: true,
                            super_key: false,
                        };
                        out.push(ParsedInput::Key(Key::new(
                            KeyCode::Char((c as char).to_ascii_lowercase()),
                            modifiers,
                        )));
                        self.reset();
                    }
                    c if shift_digit_char(c).is_some() => {
                        // shifted glyphs arrive as the glyph itself with alt+shift
                        // unmodified alt+glyph is handled by the graphic branch below
                        let base = shift_digit_char(c).expect("guarded by the match arm");
                        let modifiers = Modifiers {
                            ctrl: false,
                            alt: true,
                            shift: true,
                            super_key: false,
                        };
                        out.push(ParsedInput::Key(Key::new(KeyCode::Char(base), modifiers)));
                        self.reset();
                    }
                    c if (1..32).contains(&c) => {
                        // esc followed by a control byte is alt+ctrl+letter.
                        let modifiers = Modifiers {
                            ctrl: true,
                            alt: true,
                            shift: false,
                            super_key: false,
                        };
                        let ch = ctrl_punct_char(c).unwrap_or((b'a' + c - 1) as char);
                        out.push(ParsedInput::Key(Key::new(KeyCode::Char(ch), modifiers)));
                        self.reset();
                    }
                    c if (c as char).is_ascii_graphic() => {
                        out.push(ParsedInput::Key(Key::alt(KeyCode::Char(c as char))));
                        self.reset();
                    }
                    _ => {
                        out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                        self.reset();
                    }
                }
            }
            // APC and DCS responses from the outer terminal are bound for the
            // pane's graphics pipeline, not for the pane's stdin; the pane
            // already gets its own answers from the local query responder.
            // swallow them whole instead of leaking to apps.
            ParserState::Apc => {
                self.buf.push(b);
                // APC terminates with ST (\\) or BEL; swallow everything until then.
                if b == 0x07 || self.buf.ends_with(b"\x1b\\") {
                    self.reset();
                } else if self.buf.len() > 4096 {
                    self.buf.clear();
                    self.reset();
                }
            }
            ParserState::Dcs => {
                self.buf.push(b);
                // DCS terminates with ST (\\) or BEL.
                if b == 0x07 || self.buf.ends_with(b"\x1b\\") {
                    self.reset();
                } else if self.buf.len() > 8192 {
                    self.buf.clear();
                    self.reset();
                }
            }
            ParserState::Csi => {
                self.buf.push(b);
                if (0x40..=0x7e).contains(&b) {
                    if !self.handle_csi_final(b, out) {
                        self.reset();
                    }
                } else if self.buf.len() > 64 {
                    out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                    self.reset();
                }
            }
            ParserState::Ss3 => {
                self.buf.push(b);
                self.handle_ss3_final(b, out);
                self.reset();
            }
            ParserState::Osc => {
                // OSC responses from the outer terminal (palette replies, title
                // replies, sync-bracketed-paste probes etc.) are owned by the
                // pane's vt100, not the pane's stdin; swallow them.
                self.buf.push(b);
                if b == 0x07 || self.buf.ends_with(b"\x1b\\") {
                    self.reset();
                } else if self.buf.len() > 4096 {
                    self.buf.clear();
                    self.reset();
                }
            }
            ParserState::Paste => {
                self.paste_buf.push(b);
                // bracketed paste must complete within 256kb; exceeding this indicates
                // malformed input or dos attempt. flush partial data and reset parser.
                if self.paste_buf.ends_with(b"\x1b[201~") {
                    let len = self.paste_buf.len() - 6;
                    let content = self.paste_buf[..len].to_vec();
                    out.push(ParsedInput::Paste(content));
                    self.paste_buf.clear();
                    self.reset();
                } else if self.paste_buf.len() > 256 * 1024 {
                    const PASTE_END: &[u8] = b"\x1b[201~";
                    let suffix_len = (1..=PASTE_END.len().min(self.paste_buf.len()))
                        .rev()
                        .find(|&len| self.paste_buf.ends_with(&PASTE_END[..len]))
                        .unwrap_or(0);
                    self.buf.clear();
                    self.buf.extend_from_slice(
                        &self.paste_buf[self.paste_buf.len().saturating_sub(suffix_len)..],
                    );
                    self.paste_buf.clear();
                    self.state = ParserState::DiscardPaste;
                }
            }
            ParserState::DiscardPaste => {
                self.buf.push(b);
                if self.buf.ends_with(b"\x1b[201~") {
                    self.reset();
                } else if self.buf.len() > 6 {
                    self.buf.drain(..self.buf.len() - 6);
                }
            }
        }
    }

    fn reset(&mut self) {
        self.state = ParserState::Ground;
        self.buf.clear();
    }

    fn handle_ss3_final(&mut self, final_byte: u8, out: &mut Vec<ParsedInput>) {
        let key = match final_byte {
            b'P' => Key::plain(KeyCode::F(1)),
            b'Q' => Key::plain(KeyCode::F(2)),
            b'R' => Key::plain(KeyCode::F(3)),
            b'S' => Key::plain(KeyCode::F(4)),
            b'A' => Key::plain(KeyCode::Up),
            b'B' => Key::plain(KeyCode::Down),
            b'C' => Key::plain(KeyCode::Right),
            b'D' => Key::plain(KeyCode::Left),
            b'H' => Key::plain(KeyCode::Home),
            b'F' => Key::plain(KeyCode::End),
            _ => {
                out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                return;
            }
        };
        out.push(ParsedInput::Key(key));
    }

    fn handle_csi_final(&mut self, final_byte: u8, out: &mut Vec<ParsedInput>) -> bool {
        let s = match std::str::from_utf8(&self.buf[2..self.buf.len() - 1]) {
            Ok(s) => s,
            Err(_) => {
                out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                return false;
            }
        };

        // bracketed paste start: stay in paste state
        if final_byte == b'~' && s == "200" {
            self.state = ParserState::Paste;
            self.buf.clear();
            return true;
        }

        // focus events
        if final_byte == b'I' && s.is_empty() {
            out.push(ParsedInput::FocusGained);
            return false;
        }
        if final_byte == b'O' && s.is_empty() {
            out.push(ParsedInput::FocusLost);
            return false;
        }

        // modified f1-f4 arrive as csi 1;<mod>p..s (unmodified uses ss3 p..s)
        if matches!(final_byte, b'P' | b'Q' | b'R' | b'S')
            && let Some((params, mod_part)) = s.split_once(';')
            && params == "1"
        {
            let f = match final_byte {
                b'P' => KeyCode::F(1),
                b'Q' => KeyCode::F(2),
                b'R' => KeyCode::F(3),
                b'S' => KeyCode::F(4),
                _ => unreachable!(),
            };
            out.push(ParsedInput::Key(Key::new(
                f,
                parse_xterm_modifier(mod_part),
            )));
            return false;
        }

        // decrqm-style responses (e.g. kitty keyboard flags report "csi ? 1 u")
        // must be swallowed, not written to the pane.
        if s.starts_with('?') {
            return false;
        }

        // sgr mouse sequence: \x1b[<b;x;ym or m
        if s.starts_with('<')
            && (final_byte == b'M' || final_byte == b'm')
            && let Some(mouse) = self.parse_sgr_mouse(&s[1..], final_byte == b'M')
        {
            out.push(ParsedInput::Mouse(mouse));
            return false;
        }

        // kitty keyboard protocol: \x1b[<keycode>;<modifiers>u
        if final_byte == b'u'
            && let Some(key) = self.parse_kitty_keyboard(s)
        {
            out.push(ParsedInput::Key(key));
            return false;
        }

        // standard arrows and edit keys
        let (code, default_mod) = match final_byte {
            b'A' => (KeyCode::Up, Modifiers::NONE),
            b'B' => (KeyCode::Down, Modifiers::NONE),
            b'C' => (KeyCode::Right, Modifiers::NONE),
            b'D' => (KeyCode::Left, Modifiers::NONE),
            b'H' => (KeyCode::Home, Modifiers::NONE),
            b'F' => (KeyCode::End, Modifiers::NONE),
            // plain f1-f4 keep legacy encoding even under kitty flag 1
            b'P' => (KeyCode::F(1), Modifiers::NONE),
            b'Q' => (KeyCode::F(2), Modifiers::NONE),
            b'R' => (KeyCode::F(3), Modifiers::NONE),
            b'S' => (KeyCode::F(4), Modifiers::NONE),
            b'Z' => (
                KeyCode::Tab,
                Modifiers {
                    ctrl: false,
                    alt: false,
                    shift: true,
                    super_key: false,
                },
            ),
            b'~' => {
                // modifyotherkeys: csi 27 ; <mods> ; <code> ~
                if let Some(rest) = s.strip_prefix("27;") {
                    if let Some((mod_part, code_part)) = rest.split_once(';') {
                        let mods =
                            parse_xterm_modifier(mod_part.split(':').next().unwrap_or(mod_part));
                        let codepoint = code_part
                            .split(':')
                            .next()
                            .and_then(|p| p.parse::<u32>().ok());
                        if let Some(cp) = codepoint {
                            let key = legacy_fcode(cp)
                                .map(|f| Key::new(f, mods))
                                .or_else(|| key_from_codepoint(cp, mods));
                            if let Some(key) = key {
                                out.push(ParsedInput::Key(key));
                                return false;
                            }
                        }
                    }
                    out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                    return false;
                }
                match s {
                    "1" | "7" => (KeyCode::Home, Modifiers::NONE),
                    "2" => (KeyCode::Insert, Modifiers::NONE),
                    "3" => (KeyCode::Delete, Modifiers::NONE),
                    "4" | "8" => (KeyCode::End, Modifiers::NONE),
                    "5" => (KeyCode::PageUp, Modifiers::NONE),
                    "6" => (KeyCode::PageDown, Modifiers::NONE),
                    "11" => (KeyCode::F(1), Modifiers::NONE),
                    "12" => (KeyCode::F(2), Modifiers::NONE),
                    "13" => (KeyCode::F(3), Modifiers::NONE),
                    "14" => (KeyCode::F(4), Modifiers::NONE),
                    "15" => (KeyCode::F(5), Modifiers::NONE),
                    "17" => (KeyCode::F(6), Modifiers::NONE),
                    "18" => (KeyCode::F(7), Modifiers::NONE),
                    "19" => (KeyCode::F(8), Modifiers::NONE),
                    "20" => (KeyCode::F(9), Modifiers::NONE),
                    "21" => (KeyCode::F(10), Modifiers::NONE),
                    "23" => (KeyCode::F(11), Modifiers::NONE),
                    "24" => (KeyCode::F(12), Modifiers::NONE),
                    _ => {
                        if let Some((k_part, mod_part)) = s.split_once(';') {
                            let k = match k_part {
                                "1" => KeyCode::Home,
                                "2" => KeyCode::Insert,
                                "3" => KeyCode::Delete,
                                "4" => KeyCode::End,
                                "5" => KeyCode::PageUp,
                                "6" => KeyCode::PageDown,
                                "11" => KeyCode::F(1),
                                "12" => KeyCode::F(2),
                                "13" => KeyCode::F(3),
                                "14" => KeyCode::F(4),
                                "15" => KeyCode::F(5),
                                "17" => KeyCode::F(6),
                                "18" => KeyCode::F(7),
                                "19" => KeyCode::F(8),
                                "20" => KeyCode::F(9),
                                "21" => KeyCode::F(10),
                                "23" => KeyCode::F(11),
                                "24" => KeyCode::F(12),
                                _ => {
                                    out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                                    return false;
                                }
                            };
                            let m = parse_xterm_modifier(mod_part);
                            out.push(ParsedInput::Key(Key::new(k, m)));
                            return false;
                        }
                        out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                        return false;
                    }
                }
            }
            _ => {
                out.push(ParsedInput::Raw(std::mem::take(&mut self.buf)));
                return false;
            }
        };

        // check if there was an xterm modifier parameter (e.g. 1;2a -> shift+up)
        if let Some((_, mod_part)) = s.split_once(';') {
            let m = parse_xterm_modifier(mod_part);
            out.push(ParsedInput::Key(Key::new(code, m)));
        } else {
            out.push(ParsedInput::Key(Key::new(code, default_mod)));
        }
        false
    }

    fn parse_sgr_mouse(&self, params: &str, is_press: bool) -> Option<MouseEventMsg> {
        let parts: Vec<&str> = params.split(';').collect();
        if parts.len() != 3 {
            return None;
        }
        let b = parts[0].parse::<u16>().ok()?;
        let col = parts[1].parse::<u16>().ok()?.saturating_sub(1);
        let row = parts[2].parse::<u16>().ok()?.saturating_sub(1);

        let shift = (b & 4) != 0;
        let alt = (b & 8) != 0;
        let ctrl = (b & 16) != 0;
        let is_motion = (b & 32) != 0;

        let (button, action) = if (b & 64) != 0 {
            // scroll wheel
            let btn = if (b & 1) != 0 {
                MouseButtonKind::ScrollDown
            } else {
                MouseButtonKind::ScrollUp
            };
            (btn, MouseActionKind::Down)
        } else {
            let btn_num = b & 3;
            let btn = match btn_num {
                0 => MouseButtonKind::Left,
                1 => MouseButtonKind::Middle,
                2 => MouseButtonKind::Right,
                _ => MouseButtonKind::Left,
            };

            let act = if !is_press {
                MouseActionKind::Up
            } else if is_motion {
                if btn_num == 3 {
                    MouseActionKind::Moved
                } else {
                    MouseActionKind::Drag
                }
            } else {
                MouseActionKind::Down
            };
            (btn, act)
        };

        Some(MouseEventMsg {
            col,
            row,
            button,
            action,
            shift,
            alt,
            ctrl,
        })
    }

    fn parse_kitty_keyboard(&self, params: &str) -> Option<Key> {
        let parts: Vec<&str> = params.split(';').collect();
        let codepoint = parts[0].parse::<u32>().ok()?;

        let modifiers = if parts.len() > 1 {
            parse_xterm_modifier(parts[1].split(':').next()?)
        } else {
            Modifiers::NONE
        };

        key_from_codepoint(codepoint, modifiers)
    }
}

/// legacy `~`-final function key codes (vt220 family).
fn legacy_fcode(cp: u32) -> Option<KeyCode> {
    Some(match cp {
        11 => KeyCode::F(1),
        12 => KeyCode::F(2),
        13 => KeyCode::F(3),
        14 => KeyCode::F(4),
        15 => KeyCode::F(5),
        17 => KeyCode::F(6),
        18 => KeyCode::F(7),
        19 => KeyCode::F(8),
        20 => KeyCode::F(9),
        21 => KeyCode::F(10),
        23 => KeyCode::F(11),
        24 => KeyCode::F(12),
        _ => return None,
    })
}

/// builds a key from a codepoint and modifiers, applying the keyboard layout
/// identity rules: c0 codes map to special keys, kitty pua codepoints f13-f35
/// map to f(13)..f(35), and shift+digit-symbols normalize to the digit key.
fn key_from_codepoint(cp: u32, modifiers: Modifiers) -> Option<Key> {
    let mut code = match cp {
        9 => KeyCode::Tab,
        13 => KeyCode::Enter,
        27 => KeyCode::Esc,
        8 | 127 => KeyCode::Backspace,
        32 => KeyCode::Space,
        // kitty functional keys: f13-f35
        57376..=57398 => KeyCode::F((cp - 57376 + 13) as u8),
        _ => KeyCode::Char(char::from_u32(cp)?),
    };

    // if shift is pressed with an uppercase letter, normalize to the
    // lowercase key plus the shift modifier so bindings and forwarding agree.
    if modifiers.shift
        && let KeyCode::Char(c) = &mut code
        && c.is_ascii_uppercase()
    {
        *c = c.to_ascii_lowercase();
    }

    // if shift is pressed with numbers, normalize to digit key
    if modifiers.shift
        && let KeyCode::Char(c) = code
        && c.is_ascii()
        && let Some(digit) = shift_digit_char(c as u8)
    {
        code = KeyCode::Char(digit);
    }

    Some(Key::new(code, modifiers))
}

/// maps 0x1c..0x1f to the ctrl+punct characters terminals deliver.
fn ctrl_punct_char(b: u8) -> Option<char> {
    match b {
        0x1c => Some('\\'),
        0x1d => Some(']'),
        0x1e => Some('^'),
        0x1f => Some('_'),
        _ => None,
    }
}

/// maps the shifted number-row glyphs to their base characters.
/// the one unambiguous shift-case for almost all glyphs us-layout emits after esc
/// is "base+shift": there is no separate modifier bit, only the glyph.
fn shift_digit_char(b: u8) -> Option<char> {
    Some(match b {
        b'!' => '1',
        b'@' => '2',
        b'#' => '3',
        b'$' => '4',
        b'%' => '5',
        b'^' => '6',
        b'&' => '7',
        b'*' => '8',
        b'(' => '9',
        b')' => '0',
        b'<' => ',',
        b'>' => '.',
        b'{' => '[',
        b'}' => ']',
        b'_' => '-',
        b'+' => '=',
        b'|' => '\\',
        b':' => ';',
        b'"' => '\'',
        b'?' => '/',
        b'~' => '`',
        _ => return None,
    })
}

/// parses an xterm-style modifier parameter string (e.g., "2" for shift, "5" for ctrl).
/// kitty event-type sub-parameters after ':' (e.g. "2:3") are ignored.
fn parse_xterm_modifier(s: &str) -> Modifiers {
    let num = s
        .split(':')
        .next()
        .and_then(|p| p.parse::<u8>().ok())
        .unwrap_or(0);

    // xterm modifier codes: bit 0=shift, bit 1=alt, bit 2=ctrl, bit 3=super
    // note: terminal sends (code+1), so we need to subtract 1 first
    let val = num.saturating_sub(1);
    Modifiers {
        shift: (val & 1) != 0,
        alt: (val & 2) != 0,
        ctrl: (val & 4) != 0,
        super_key: (val & 8) != 0,
    }
}
