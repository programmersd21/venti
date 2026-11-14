use serde::{Deserialize, Serialize};
use std::fmt;

/// modifier flags for key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        super_key: false,
    };
    pub const ALT: Self = Self {
        ctrl: false,
        alt: true,
        shift: false,
        super_key: false,
    };
}

/// abstract key representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    F(u8),
    Space,
}

/// fully qualified key press with modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

impl Key {
    pub const fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        Self { code, modifiers }
    }

    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::NONE,
        }
    }

    pub const fn alt(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::ALT,
        }
    }

    pub const fn ctrl(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::CTRL,
        }
    }

    /// encodes this key press into the byte sequence a terminal application expects.
    pub fn to_bytes(&self) -> Vec<u8> {
        let m = self.modifiers;
        let xmod = xterm_modifier(m);
        let mut out = Vec::with_capacity(8);

        match &self.code {
            KeyCode::Char(c) => {
                if m.ctrl
                    && let Some(b) = control_byte(*c)
                {
                    if m.alt {
                        out.push(0x1b);
                    }
                    out.push(b);
                    return out;
                }
                if m.alt {
                    out.push(0x1b);
                }
                let ch = if m.shift && c.is_ascii_lowercase() {
                    c.to_ascii_uppercase()
                } else {
                    *c
                };
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            KeyCode::Space => {
                if m.alt {
                    out.push(0x1b);
                }
                out.push(if m.ctrl { 0x00 } else { b' ' });
            }
            KeyCode::Enter => {
                if m.alt {
                    out.push(0x1b);
                }
                out.push(b'\r');
            }
            KeyCode::Backspace => {
                if m.alt {
                    out.push(0x1b);
                }
                out.push(if m.ctrl { 0x08 } else { 0x7f });
            }
            KeyCode::Tab => {
                if m.ctrl || m.super_key {
                    // modifyotherkeys encoding: csi 27;<mod>;9~
                    let v = xmod.expect("ctrl or super set means modifier bits are set");
                    push_csi(&mut out, Some(&format!("27;{v};9")), b'~');
                } else if m.shift {
                    let v = xmod.expect("shift set means modifier bits are set");
                    push_csi(&mut out, Some(&format!("1;{v}")), b'Z');
                } else {
                    if m.alt {
                        out.push(0x1b);
                    }
                    out.push(b'\t');
                }
            }
            KeyCode::Esc => {
                out.push(0x1b);
                if m.alt {
                    out.push(0x1b);
                }
            }
            KeyCode::Up => push_cursor_key(&mut out, xmod, b'A'),
            KeyCode::Down => push_cursor_key(&mut out, xmod, b'B'),
            KeyCode::Right => push_cursor_key(&mut out, xmod, b'C'),
            KeyCode::Left => push_cursor_key(&mut out, xmod, b'D'),
            KeyCode::Home => push_cursor_key(&mut out, xmod, b'H'),
            KeyCode::End => push_cursor_key(&mut out, xmod, b'F'),
            KeyCode::Insert => push_tilde_key(&mut out, xmod, 2),
            KeyCode::Delete => push_tilde_key(&mut out, xmod, 3),
            KeyCode::PageUp => push_tilde_key(&mut out, xmod, 5),
            KeyCode::PageDown => push_tilde_key(&mut out, xmod, 6),
            KeyCode::F(n) => push_function_key(&mut out, xmod, *n),
        }

        out
    }
}

/// computes the xterm modifier parameter (1-based) for a modifier set.
fn xterm_modifier(m: Modifiers) -> Option<u8> {
    let bits = u8::from(m.shift)
        | (u8::from(m.alt) << 1)
        | (u8::from(m.ctrl) << 2)
        | (u8::from(m.super_key) << 3);
    if bits == 0 { None } else { Some(bits + 1) }
}

/// maps a character to its ascii control code, if it has one.
fn control_byte(c: char) -> Option<u8> {
    let lower = c.to_ascii_lowercase();
    let byte = match lower {
        'a'..='z' => (lower as u8) - b'a' + 1,
        '@' => 0,
        '[' => 27,
        '\\' => 28,
        ']' => 29,
        '^' => 30,
        '_' => 31,
        ' ' => 0,
        '?' => 0x7f,
        _ => return None,
    };
    Some(byte)
}

fn push_csi(out: &mut Vec<u8>, params: Option<&str>, final_byte: u8) {
    out.extend_from_slice(b"\x1b[");
    if let Some(params) = params {
        out.extend_from_slice(params.as_bytes());
    }
    out.push(final_byte);
}

fn push_cursor_key(out: &mut Vec<u8>, xmod: Option<u8>, final_byte: u8) {
    let params = xmod.map(|v| format!("1;{v}"));
    push_csi(out, params.as_deref(), final_byte);
}

fn push_tilde_key(out: &mut Vec<u8>, xmod: Option<u8>, code: u8) {
    let params = xmod.map_or_else(|| code.to_string(), |v| format!("{code};{v}"));
    push_csi(out, Some(&params), b'~');
}

fn push_function_key(out: &mut Vec<u8>, xmod: Option<u8>, n: u8) {
    match n {
        1..=4 => {
            let final_byte = b'P' + (n - 1);
            if let Some(v) = xmod {
                push_csi(out, Some(&format!("1;{v}")), final_byte);
            } else {
                out.extend_from_slice(b"\x1bO");
                out.push(final_byte);
            }
        }
        _ => {
            let code = match n {
                5 => 15,
                6 => 17,
                7 => 18,
                8 => 19,
                9 => 20,
                10 => 21,
                11 => 23,
                12 => 24,
                _ => return,
            };
            push_tilde_key(out, xmod, code);
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push("ctrl");
        }
        if self.modifiers.alt {
            parts.push("alt");
        }
        if self.modifiers.shift {
            parts.push("shift");
        }
        if self.modifiers.super_key {
            parts.push("super");
        }

        let code_str = match &self.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "enter".to_string(),
            KeyCode::Backspace => "backspace".to_string(),
            KeyCode::Tab => "tab".to_string(),
            KeyCode::Esc => "esc".to_string(),
            KeyCode::Up => "up".to_string(),
            KeyCode::Down => "down".to_string(),
            KeyCode::Left => "left".to_string(),
            KeyCode::Right => "right".to_string(),
            KeyCode::Home => "home".to_string(),
            KeyCode::End => "end".to_string(),
            KeyCode::PageUp => "pageup".to_string(),
            KeyCode::PageDown => "pagedown".to_string(),
            KeyCode::Delete => "delete".to_string(),
            KeyCode::Insert => "insert".to_string(),
            KeyCode::F(n) => format!("f{n}"),
            KeyCode::Space => "space".to_string(),
        };
        parts.push(&code_str);

        write!(f, "{}", parts.join("+"))
    }
}

/// parses a string representation of a key (e.g., "alt+enter", "ctrl+shift+h", "f1").
pub fn parse_key(s: &str) -> Result<Key, String> {
    let lower = s.trim().to_lowercase();
    let mut tokens: Vec<&str> = lower.split('+').map(|t| t.trim()).collect();
    // a trailing '+' means the key itself is '+', e.g. "ctrl++".
    if lower.ends_with('+') && tokens.last() == Some(&"") {
        tokens.pop();
        tokens.push("+");
    }
    if tokens.is_empty() || (tokens.len() == 1 && tokens[0].is_empty()) {
        return Err("empty key specification".to_string());
    }

    let mut modifiers = Modifiers::NONE;
    let mut code: Option<KeyCode> = None;

    for (i, token) in tokens.iter().enumerate() {
        if i < tokens.len() - 1 {
            let slot = match *token {
                "ctrl" => &mut modifiers.ctrl,
                "alt" => &mut modifiers.alt,
                "shift" => &mut modifiers.shift,
                "super" => &mut modifiers.super_key,
                other => return Err(format!("unknown modifier \"{other}\" in \"{s}\"")),
            };
            if *slot {
                return Err(format!("duplicate modifier \"{token}\" in \"{s}\""));
            }
            *slot = true;
        } else {
            // last token is key code, unless string is something like "ctrl++"
            code = Some(match *token {
                "enter" | "return" => KeyCode::Enter,
                "backspace" | "bs" => KeyCode::Backspace,
                "tab" => KeyCode::Tab,
                "esc" | "escape" => KeyCode::Esc,
                "up" | "arrow_up" => KeyCode::Up,
                "down" | "arrow_down" => KeyCode::Down,
                "left" | "arrow_left" => KeyCode::Left,
                "right" | "arrow_right" => KeyCode::Right,
                "home" => KeyCode::Home,
                "end" => KeyCode::End,
                "pageup" | "pgup" => KeyCode::PageUp,
                "pagedown" | "pgdn" => KeyCode::PageDown,
                "delete" | "del" => KeyCode::Delete,
                "insert" | "ins" => KeyCode::Insert,
                "space" => KeyCode::Space,
                f_str if f_str.starts_with('f') && f_str.len() > 1 => {
                    let num = f_str[1..]
                        .parse::<u8>()
                        .map_err(|_| format!("invalid function key \"{f_str}\" in \"{s}\""))?;
                    if !(1..=12).contains(&num) {
                        return Err(format!(
                            "function keys f1-f12 are supported, got \"{f_str}\" in \"{s}\""
                        ));
                    }
                    KeyCode::F(num)
                }
                c_str if c_str.chars().count() == 1 => {
                    KeyCode::Char(c_str.chars().next().expect("valid character"))
                }
                other => return Err(format!("unknown key code \"{other}\" in \"{s}\"")),
            });
        }
    }

    let code = code.ok_or_else(|| format!("missing key code in \"{s}\""))?;
    Ok(Key { code, modifiers })
}
