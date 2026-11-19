use crossterm::QueueableCommand;
use crossterm::cursor::{Hide, MoveTo, SetCursorStyle, Show};
use crossterm::style::{Attribute, SetAttribute};
use crossterm::terminal::{BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate};
use std::borrow::Cow;
use std::io::{Stdout, Write};

use unicode_width::UnicodeWidthStr;

use crate::proto::{CursorShape, ProtoColor, ScreenUpdate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ColorState {
    #[default]
    Unknown,
    Default,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

impl ColorState {
    fn from_proto(c: ProtoColor) -> Self {
        match c {
            ProtoColor::Default => Self::Default,
            ProtoColor::Rgb(r, g, b) => Self::Rgb(r, g, b),
            ProtoColor::Indexed(i) => Self::Indexed(i),
        }
    }
}

fn write_color<W: Write>(out: &mut W, selector: u8, color: ColorState) -> std::io::Result<()> {
    match color {
        ColorState::Default => {
            let reset = match selector {
                38 => 39,
                48 => 49,
                58 => 59,
                _ => return Ok(()),
            };
            write!(out, "\x1b[{reset}m")
        }
        ColorState::Rgb(r, g, b) => write!(out, "\x1b[{selector};2;{r};{g};{b}m"),
        ColorState::Indexed(index) => write!(out, "\x1b[{selector};5;{index}m"),
        ColorState::Unknown => Ok(()),
    }
}

/// persistent terminal state used to avoid redundant escape sequences between frames.
#[derive(Debug, Default)]
pub struct ApplyState {
    fg: ColorState,
    bg: ColorState,
    underline_color: ColorState,
    modifiers: u8,
    cursor_visible: Option<bool>,
    cursor_shape: Option<CursorShape>,
    cursor_pos: Option<(u16, u16)>,
    /// outer terminal graphics forwarding mode (unknown = drop sequences).
    pub gfx_passthrough: crate::util::outer_term::OuterGraphics,
    /// whether the local terminal has mouse reporting grabbed (mirrors
    /// `input.mouse` in the config). used to re-assert host-side mouse
    /// capture after a soft reset wipes it.
    pub mouse_enabled: bool,
    /// kitty graphics image ids forwarded to the host this session. deleted
    /// on client teardown (before leaving the alternate screen), because kitty
    /// keeps placements and image data across `?1049l` and they would
    /// otherwise linger on the user's terminal.
    pub kitty_images: std::sync::Arc<std::sync::Mutex<crate::client::terminal::KittyImageIds>>,
}

const BOLD: u8 = 1 << 0;
const DIM: u8 = 1 << 1;
const ITALIC: u8 = 1 << 2;
const UNDERLINE: u8 = 1 << 3;
const REVERSE: u8 = 1 << 4;
const STRIKE: u8 = 1 << 5;

fn safe_cell_symbol(symbol: &str) -> Cow<'_, str> {
    safe_terminal_text(symbol, 64)
}

pub(super) fn safe_terminal_title(title: &str) -> Cow<'_, str> {
    safe_terminal_text(title, 256)
}

fn safe_terminal_text(text: &str, max_chars: usize) -> Cow<'_, str> {
    let mut safe = String::new();
    let mut count = 0;
    for ch in text.chars() {
        if ch.is_control()
            || matches!(
                ch,
                '\u{061c}'
                    | '\u{200e}'..='\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{206f}'
            )
        {
            continue;
        }
        safe.push(ch);
        count += 1;
        if count == max_chars {
            break;
        }
    }
    if safe.is_empty() {
        Cow::Borrowed(" ")
    } else if safe == text {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(safe)
    }
}

/// applies server screen updates to the client terminal using crossterm.
pub fn apply_update(
    out: &mut Stdout,
    update: &ScreenUpdate,
    state: &mut ApplyState,
) -> std::io::Result<()> {
    out.queue(BeginSynchronizedUpdate)?;
    let result = apply_update_inner(out, update, state);

    // always close the frame, even if an update fails partway through, so the
    // terminal is not left holding an open synchronized-update.
    let end_result = match out.queue(EndSynchronizedUpdate) {
        Ok(_) => out.flush(),
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => end_result,
        Err(error) => {
            let _ = end_result;
            Err(error)
        }
    }
}

fn apply_update_inner<W: Write>(
    out: &mut W,
    update: &ScreenUpdate,
    state: &mut ApplyState,
) -> std::io::Result<()> {
    if update.reset {
        // DECSTR soft-resets SGR, charset, margins and most DEC private modes
        // without leaving the alternate screen or wiping scrollback. RIS would
        // drop the host back to the main screen and the TUI would keep painting
        // there. sixel pixels are wiped by the ED2 of the full redraw that
        // follows, kitty images are deleted with kitty delete commands.
        out.write_all(b"\x1b[!p")?;
        state.fg = ColorState::Unknown;
        state.bg = ColorState::Unknown;
        state.underline_color = ColorState::Unknown;
        state.modifiers = 0;
        state.cursor_visible = None;
        state.cursor_shape = None;
        if state.mouse_enabled {
            // DECSTR also resets the modes that capture the mouse (1000/1002/
            // 1003/1006), focus events (1004) and bracketed paste (2004) on
            // terminals that treat them as resettable. re-assert them, or the
            // host stops reporting mouse input to us.
            out.write_all(
                b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h\x1b[?2004h\x1b[?1004h\x1b[?8452h",
            )?;
        }
    }
    if update.full_redraw {
        // ensure background & foreground colors and attributes are reset before clearing,
        // preventing BCE (Background Color Erase) from painting the screen with a stale/gray background.
        out.queue(SetAttribute(Attribute::Reset))?;
        out.write_all(b"\x1b[39m\x1b[49m\x1b[59m")?;
        out.queue(Clear(ClearType::All))?;
        // ED2 clears Kitty images too. the server re-emits retained placements
        // after this clear; other text erases leave Kitty images untouched.
        state.fg = ColorState::Default;
        state.bg = ColorState::Default;
        state.underline_color = ColorState::Default;
        state.modifiers = 0;
        state.cursor_visible = None;
        state.cursor_shape = None;
        state.cursor_pos = None;
    }

    let mut pen: Option<(u16, u16)> = None;
    for cell in &update.cells {
        if pen != Some((cell.x, cell.y)) {
            out.queue(MoveTo(cell.x, cell.y))?;
        }

        let modifiers = (u8::from(cell.bold) * BOLD)
            | (u8::from(cell.dim) * DIM)
            | (u8::from(cell.italic) * ITALIC)
            | (u8::from(cell.underline) * UNDERLINE)
            | (u8::from(cell.reverse) * REVERSE)
            | (u8::from(cell.strike) * STRIKE);

        if modifiers != state.modifiers {
            out.queue(SetAttribute(Attribute::Reset))?;
            state.modifiers = 0;
            // after Attribute::Reset the terminal is at default fg/bg, record
            // that so we don't immediately re-emit \x1b[39m / \x1b[49m for
            // the very next cell that also wants default colors.
            state.fg = ColorState::Default;
            state.bg = ColorState::Default;
            state.underline_color = ColorState::Default;

            if modifiers & BOLD != 0 {
                out.queue(SetAttribute(Attribute::Bold))?;
            }
            if modifiers & DIM != 0 {
                out.queue(SetAttribute(Attribute::Dim))?;
            }
            if modifiers & ITALIC != 0 {
                out.queue(SetAttribute(Attribute::Italic))?;
            }
            if modifiers & UNDERLINE != 0 {
                out.queue(SetAttribute(Attribute::Underlined))?;
            }
            if modifiers & REVERSE != 0 {
                out.queue(SetAttribute(Attribute::Reverse))?;
            }
            if modifiers & STRIKE != 0 {
                out.queue(SetAttribute(Attribute::CrossedOut))?;
            }
            state.modifiers = modifiers;
        }

        let fg = ColorState::from_proto(cell.fg);
        if fg != state.fg {
            write_color(out, 38, fg)?;
            state.fg = fg;
        }

        let bg = ColorState::from_proto(cell.bg);
        if bg != state.bg {
            write_color(out, 48, bg)?;
            state.bg = bg;
        }

        let underline_color = ColorState::from_proto(cell.underline_color);
        if underline_color != state.underline_color {
            write_color(out, 58, underline_color)?;
            state.underline_color = underline_color;
        }

        let symbol = safe_cell_symbol(&cell.symbol);
        write!(out, "{symbol}")?;
        let width = UnicodeWidthStr::width(symbol.as_ref()).max(1) as u16;
        pen = Some((cell.x.saturating_add(width), cell.y));
    }

    // graphics escape sequences are written verbatim at their anchored cell.
    // how they are forwarded depends on the outer terminal:
    // - tmux: wrapped in tmux's DCS passthrough envelope or the bytes are
    //   filtered out entirely
    // - kitty/ghostty/wezterm: raw kitty APC sequences go straight through
    // - anything else: dropped, because writing them as-is prints garbage text
    for g in &update.gfx {
        if let Some(bytes) = forward_gfx(state.gfx_passthrough, g) {
            if g.format == crate::proto::GfxFormat::Kitty
                && let Ok(mut ids) = state.kitty_images.lock()
            {
                ids.extend(kitty_image_ids(&g.data));
            }
            out.queue(MoveTo(g.col, g.row))?;
            out.write_all(&bytes)?;
            // the host terminal's cursor moved with the image; track it so
            // the next cell write repositions correctly instead of leaving
            // text wherever the image left the cursor.
            state.cursor_pos = Some((g.col, g.row));
        }
    }

    // Child applications and shell integrations can change the host cursor
    // without changing this client's cache. Reassert it on every update,
    // including modal frames that hide the cursor.
    let style = match update.cursor_shape {
        CursorShape::Default | CursorShape::Block => SetCursorStyle::SteadyBlock,
        CursorShape::Underline => SetCursorStyle::SteadyUnderScore,
        CursorShape::Bar => SetCursorStyle::SteadyBar,
    };
    out.queue(style)?;
    state.cursor_shape = Some(update.cursor_shape);

    if update.cursor_visible
        && let Some((x, y)) = update.cursor_pos
    {
        if state.cursor_visible != Some(true) {
            out.queue(Show)?;
            state.cursor_visible = Some(true);
        }
        // cell diffs and graphics moves leave the host terminal cursor at the
        // last drawn cell, so always restore the logical (cached) position at
        // the end of the frame even if it did not change.
        if !update.cells.is_empty() || !update.gfx.is_empty() || state.cursor_pos != Some((x, y)) {
            out.queue(MoveTo(x, y))?;
            state.cursor_pos = Some((x, y));
        }
    } else if state.cursor_visible != Some(false) {
        // visible flag without a position: hide rather than leave a stale cursor.
        out.queue(Hide)?;
        state.cursor_visible = Some(false);
        state.cursor_pos = None;
    }

    out.flush()
}

/// extracts kitty image ids/numbers from forwarded APC sequences so the
/// client can delete exactly the images it placed on the host terminal when it
/// exits. scans every `ESC _ G … ESC \` block in the payload; the param
/// section (before the first `;`) is comma-separated `key=value` pairs.
/// `q=2` on the delete commands we later emit keeps the host silent, so
/// deleting ids the application already removed is harmless.
fn kitty_image_ids(data: &[u8]) -> Vec<crate::client::terminal::KittyImageId> {
    let mut out = Vec::new();
    let mut rest = data;
    while let Some(start) = find_subslice(rest, b"\x1b_G") {
        let body_start = start + 3;
        let body = &rest[body_start..];
        let body_end = find_subslice(body, b"\x1b\\").unwrap_or(body.len());
        let params = &body[..body.iter().position(|&b| b == b';').unwrap_or(body_end)];
        for pair in params.split(|&b| b == b',') {
            let Some(eq) = pair.iter().position(|&b| b == b'=') else {
                continue;
            };
            let key = &pair[..eq];
            let value = &pair[eq + 1..];
            let parsed = std::str::from_utf8(value)
                .ok()
                .and_then(|v| v.trim().parse::<u32>().ok());
            let Some(id) = parsed else { continue };
            match key {
                b"i" => out.push(crate::client::terminal::KittyImageId::Image(id)),
                b"I" => out.push(crate::client::terminal::KittyImageId::Number(id)),
                _ => {}
            }
        }
        rest = &body[body_end.min(body.len())..];
    }
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// decides how one queued graphics command reaches the outer terminal, if at
/// all. borrowed payloads avoid copying multi-megabyte images per frame.
fn forward_gfx(
    passthrough: crate::util::outer_term::OuterGraphics,
    g: &crate::proto::GfxCmd,
) -> Option<Cow<'_, [u8]>> {
    use crate::proto::GfxFormat;
    use crate::util::outer_term::OuterGraphics;
    match passthrough {
        OuterGraphics::Tmux => Some(Cow::Owned(wrap_tmux_passthrough(&g.data))),
        OuterGraphics::Kitty if g.format == GfxFormat::Kitty => {
            Some(Cow::Borrowed(g.data.as_slice()))
        }
        OuterGraphics::Sixel if g.format == GfxFormat::Sixel => {
            Some(Cow::Borrowed(g.data.as_slice()))
        }
        _ => None,
    }
}

/// wraps a graphics sequence in tmux's DCS passthrough envelope.
///
/// tmux filters out escape sequences it does not own, so applications inside
/// tmux must tunnel terminal-specific sequences through this envelope:
///   `ESC P t m u x ; <inner sequence with every ESC doubled> ESC \`
/// tmux strips the envelope and forwards the payload to the outer terminal.
/// requires `set -g allow-passthrough on` in ~/.tmux.conf (tmux 3.3+); when it
/// is off the whole envelope is silently dropped, which is still better than
/// printing the payload as text.
pub fn wrap_tmux_passthrough(seq: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(seq.len() + 10);
    // open: ESC P t m u x ;
    out.extend_from_slice(b"\x1bPtmux;");
    // inner payload with ESC bytes doubled so tmux's DCS parser doesn't see
    // a premature ST
    for &b in seq {
        if b == 0x1b {
            out.push(0x1b);
            out.push(0x1b);
        } else {
            out.push(b);
        }
    }
    // close: ESC \
    out.extend_from_slice(b"\x1b\\");
    out
}

#[cfg(test)]
mod tests {
    use super::{ApplyState, kitty_image_ids, safe_cell_symbol, safe_terminal_title};
    use crate::client::terminal::KittyImageId;
    use crate::proto::{CursorShape, ProtoColor, ScreenUpdate};

    #[test]
    fn cursor_style_is_reasserted_even_when_cached_value_matches() {
        let mut state = ApplyState {
            cursor_shape: Some(CursorShape::Block),
            ..ApplyState::default()
        };
        let update = ScreenUpdate {
            cols: 80,
            rows: 24,
            cursor_pos: Some((3, 4)),
            cursor_visible: true,
            cursor_shape: CursorShape::Block,
            cells: Vec::new(),
            full_redraw: false,
            gfx: Vec::new(),
            reset: false,
        };
        let mut output = Vec::new();
        super::apply_update_inner(&mut output, &update, &mut state).unwrap();
        assert!(output.windows(5).any(|window| window == b"\x1b[2 q"));
    }

    #[test]
    fn cursor_style_is_reasserted_when_cursor_is_hidden() {
        let update = ScreenUpdate {
            cols: 80,
            rows: 24,
            cursor_pos: None,
            cursor_visible: false,
            cursor_shape: CursorShape::Block,
            cells: Vec::new(),
            full_redraw: false,
            gfx: Vec::new(),
            reset: false,
        };
        let mut output = Vec::new();
        super::apply_update_inner(&mut output, &update, &mut ApplyState::default()).unwrap();
        assert!(output.windows(5).any(|window| window == b"\x1b[2 q"));
    }

    #[test]
    fn underline_color_is_sent_to_the_host_terminal() {
        let update = ScreenUpdate {
            cols: 80,
            rows: 24,
            cursor_pos: None,
            cursor_visible: false,
            cursor_shape: CursorShape::Block,
            cells: vec![crate::proto::CellDiff {
                x: 0,
                y: 0,
                symbol: "x".to_string(),
                fg: ProtoColor::Rgb(0, 0, 42),
                bg: ProtoColor::Default,
                underline_color: ProtoColor::Rgb(17, 34, 51),
                bold: false,
                dim: false,
                italic: false,
                underline: false,
                reverse: false,
                strike: false,
            }],
            full_redraw: false,
            gfx: Vec::new(),
            reset: false,
        };
        let mut output = Vec::new();
        super::apply_update_inner(&mut output, &update, &mut ApplyState::default()).unwrap();
        assert!(
            output
                .windows(b"\x1b[58;2;17;34;51m".len())
                .any(|window| window == b"\x1b[58;2;17;34;51m"),
            "{output:?}"
        );
        assert!(
            output
                .windows(b"\x1b[38;2;0;0;42m".len())
                .any(|window| window == b"\x1b[38;2;0;0;42m")
        );
    }

    #[test]
    fn kitty_image_ids_are_extracted_from_placement_and_transmit_commands() {
        let transmit = b"\x1b_Ga=T,f=100,i=42,q=2;aGk=\x1b\\";
        let place = b"\x1b_Ga=p,i=42,p=7,q=2\x1b\\";
        let mut bytes = transmit.to_vec();
        bytes.extend_from_slice(place);
        let ids = kitty_image_ids(&bytes);
        assert_eq!(ids, vec![KittyImageId::Image(42), KittyImageId::Image(42)]);
    }

    #[test]
    fn kitty_image_ids_support_numbers_and_delete_commands() {
        let named = b"\x1b_Ga=T,I=9,f=24,s=1,v=1;AAA=\x1b\\";
        let delete = b"\x1b_Ga=d,d=I,i=5,q=2\x1b\\";
        let named_ids = kitty_image_ids(named);
        assert_eq!(named_ids, vec![KittyImageId::Number(9)]);
        let delete_ids = kitty_image_ids(delete);
        assert_eq!(delete_ids, vec![KittyImageId::Image(5)]);
        // commands without an id contribute nothing.
        assert!(kitty_image_ids(b"\x1b_Ga=d,d=A,q=2\x1b\\").is_empty());
    }

    #[test]
    fn cell_symbols_cannot_emit_terminal_or_bidi_controls() {
        assert_eq!(safe_cell_symbol("a\x1b[2Jb\u{009b}c"), "a[2Jbc");
        assert_eq!(safe_cell_symbol("\u{202e}spoof\u{2069}"), "spoof");
        assert_eq!(safe_cell_symbol("\x1b\x07"), " ");
    }

    #[test]
    fn cell_symbols_are_bounded() {
        let symbol = "x".repeat(1000);
        assert_eq!(safe_cell_symbol(&symbol).chars().count(), 64);
    }

    #[test]
    fn terminal_titles_cannot_terminate_the_osc_sequence() {
        assert_eq!(safe_terminal_title("name\x1b\\\x07\u{202e}x"), "name\\x");
        assert_eq!(safe_terminal_title(&"t".repeat(1000)).chars().count(), 256);
    }
}
