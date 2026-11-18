//! Pure-Rust production-grade Sixel graphics pipeline for Venti.
//!
//! Provides:
//! - PTY stream interception of Sixel DCS sequences (`ESC P ... q ... ESC \`)
//! - Pure-Rust decoding via `icy_sixel` into `image::DynamicImage`
//! - Anchoring to logical scrollback lines and cursor tracking
//! - Viewport culling and layout containment for Ratatui pane rendering

use icy_sixel::SixelImage;
use image::{DynamicImage, RgbaImage};
use std::collections::VecDeque;

/// Maximum allowed bytes in a single Sixel DCS sequence to prevent DoS (32MB).
pub const MAX_SIXEL_BYTES: usize = 32 * 1024 * 1024;
/// Maximum number of active Sixel images stored per pane.
pub const MAX_STORED_SIXELS: usize = 64;

/// Parser state machine for intercepting raw Sixel sequences from the PTY stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SixelParserState {
    /// Normal text or ANSI sequences, forwarding to vt100 emulator.
    Ground,
    /// Encountered ESC (0x1B).
    Escape,
    /// Encountered DCS introducer ('P' after ESC, or 0x90). Accumulating DCS parameters.
    DcsParam,
    /// Inside Sixel body after 'q', accumulating until ST (ESC \ or 0x9C).
    SixelBody,
    /// Encountered ESC inside Sixel body, checking for '\'.
    SixelEscape,
}

/// A parsed segment from the PTY reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SixelSegment {
    /// Clean terminal text/ANSI bytes to feed into `vt100::Parser`.
    Text(Vec<u8>),
    /// A complete raw Sixel escape sequence starting with `ESC P ... q` and ending with `ESC \`.
    Sixel(Vec<u8>),
}

/// Stream interceptor for Sixel sequences.
#[derive(Debug)]
pub struct SixelStreamParser {
    state: SixelParserState,
    text_buf: Vec<u8>,
    sixel_buf: Vec<u8>,
}

impl Default for SixelStreamParser {
    fn default() -> Self {
        Self {
            state: SixelParserState::Ground,
            text_buf: Vec::with_capacity(8192),
            sixel_buf: Vec::with_capacity(4096),
        }
    }
}

impl SixelStreamParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds incoming PTY bytes, returning parsed text and Sixel segments in order.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SixelSegment> {
        let mut segments = Vec::new();

        for &b in bytes {
            match self.state {
                SixelParserState::Ground => {
                    if b == 0x1b {
                        self.state = SixelParserState::Escape;
                    } else if b == 0x90 {
                        // 8-bit DCS
                        self.flush_text(&mut segments);
                        self.sixel_buf.clear();
                        self.sixel_buf.push(b);
                        self.state = SixelParserState::DcsParam;
                    } else {
                        self.text_buf.push(b);
                    }
                }
                SixelParserState::Escape => {
                    if b == b'P' {
                        self.flush_text(&mut segments);
                        self.sixel_buf.clear();
                        self.sixel_buf.push(0x1b);
                        self.sixel_buf.push(b'P');
                        self.state = SixelParserState::DcsParam;
                    } else {
                        self.text_buf.push(0x1b);
                        self.text_buf.push(b);
                        self.state = SixelParserState::Ground;
                    }
                }
                SixelParserState::DcsParam => {
                    self.sixel_buf.push(b);
                    if b == b'q' {
                        // Enter Sixel image data body
                        self.state = SixelParserState::SixelBody;
                    } else if (0x20..=0x3f).contains(&b) {
                        // Parameter characters (digits, semicolons)
                        if self.sixel_buf.len() > 64 {
                            // Abandon oversized DCS introducer
                            self.text_buf.append(&mut self.sixel_buf);
                            self.state = SixelParserState::Ground;
                        }
                    } else {
                        // Non-sixel DCS sequence (e.g. DECRQSS, etc.) -> return to text
                        self.text_buf.append(&mut self.sixel_buf);
                        self.state = SixelParserState::Ground;
                    }
                }
                SixelParserState::SixelBody => {
                    self.sixel_buf.push(b);
                    if b == 0x1b {
                        self.state = SixelParserState::SixelEscape;
                    } else if b == 0x9c {
                        // 8-bit ST
                        segments.push(SixelSegment::Sixel(std::mem::take(&mut self.sixel_buf)));
                        self.state = SixelParserState::Ground;
                    } else if self.sixel_buf.len() > MAX_SIXEL_BYTES {
                        // Sequence too large, drop to prevent OOM
                        self.sixel_buf.clear();
                        self.state = SixelParserState::Ground;
                    }
                }
                SixelParserState::SixelEscape => {
                    self.sixel_buf.push(b);
                    if b == b'\\' {
                        // Completed 7-bit ST (ESC \)
                        segments.push(SixelSegment::Sixel(std::mem::take(&mut self.sixel_buf)));
                        self.state = SixelParserState::Ground;
                    } else if b == 0x1b {
                        // Consecutive escape
                        self.state = SixelParserState::SixelEscape;
                    } else {
                        // Not ST, continue inside Sixel body
                        self.state = SixelParserState::SixelBody;
                    }
                }
            }
        }

        self.flush_text(&mut segments);
        segments
    }

    fn flush_text(&mut self, segments: &mut Vec<SixelSegment>) {
        if !self.text_buf.is_empty() {
            segments.push(SixelSegment::Text(std::mem::take(&mut self.text_buf)));
        }
    }
}

/// Decodes raw Sixel bytes into a standard `image::DynamicImage` using `icy_sixel`.
pub fn decode_sixel(raw_bytes: &[u8]) -> Result<DynamicImage, String> {
    let sixel_img =
        SixelImage::decode(raw_bytes).map_err(|e| format!("sixel decode error: {e}"))?;
    let width = sixel_img.width as u32;
    let height = sixel_img.height as u32;

    if width == 0 || height == 0 {
        return Err("sixel image has zero dimensions".to_string());
    }

    let rgba = RgbaImage::from_raw(width, height, sixel_img.pixels)
        .ok_or_else(|| "failed to create RgbaImage buffer from decoded sixel".to_string())?;

    Ok(DynamicImage::ImageRgba8(rgba))
}

/// Calculates the number of terminal cell rows occupied by an image given cell pixel height.
pub fn calculate_cell_rows(pixel_height: u32, cell_pixel_height: u32) -> u16 {
    let cell_h = cell_pixel_height.max(1);
    pixel_height.div_ceil(cell_h).clamp(1, u16::MAX as u32) as u16
}

/// Calculates the number of terminal cell columns occupied by an image given cell pixel width.
pub fn calculate_cell_cols(pixel_width: u32, cell_pixel_width: u32) -> u16 {
    let cell_w = cell_pixel_width.max(1);
    pixel_width.div_ceil(cell_w).clamp(1, u16::MAX as u32) as u16
}

/// Represents an active Sixel graphic anchored to a pane's buffer.
#[derive(Debug, Clone)]
pub struct SixelImageEntry {
    /// Monotonic ID assigned to this image entry.
    pub id: u64,
    /// Cursor column at emission.
    pub col: u16,
    /// Logical line index in scrollback history: `scrollback_lines + cursor_row`.
    pub logical_line: usize,
    /// Cell dimensions `(cols, rows)`.
    pub cell_cols: u16,
    pub cell_rows: u16,
    /// Pixel dimensions `(width, height)`.
    pub pixel_size: (u32, u32),
    /// Decoded dynamic RGBA image.
    pub image: DynamicImage,
    /// Whether image was emitted on the alternate screen buffer.
    pub alternate_screen: bool,
    /// Raw sixel DCS escape payload for direct pass-through/fallback.
    pub raw_data: Vec<u8>,
}

/// Container for managing anchored Sixel images in a pane.
#[derive(Debug, Default)]
pub struct SixelStore {
    entries: VecDeque<SixelImageEntry>,
    next_id: u64,
}

impl SixelStore {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            next_id: 1,
        }
    }

    pub fn push(&mut self, mut entry: SixelImageEntry) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        entry.id = id;

        if self.entries.len() >= MAX_STORED_SIXELS {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        id
    }

    pub fn entries(&self) -> &VecDeque<SixelImageEntry> {
        &self.entries
    }

    pub fn clear_alternate_screen(&mut self) {
        self.entries.retain(|e| !e.alternate_screen);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_sixel_stream() {
        let mut parser = SixelStreamParser::new();
        let stream = b"hello\x1bPq#0;2;100;0;0#0~-\x1b\\world";
        let segments = parser.feed(stream);

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], SixelSegment::Text(b"hello".to_vec()));
        match &segments[1] {
            SixelSegment::Sixel(data) => {
                assert!(data.starts_with(b"\x1bPq"));
                assert!(data.ends_with(b"\x1b\\"));
            }
            _ => panic!("expected sixel segment"),
        }
        assert_eq!(segments[2], SixelSegment::Text(b"world".to_vec()));
    }

    #[test]
    fn decodes_simple_sixel() {
        // Minimal valid sixel sequence
        let sixel_bytes = b"\x1bPq#0;2;100;0;0#0~-\x1b\\";
        let decoded = decode_sixel(sixel_bytes);
        assert!(
            decoded.is_ok(),
            "failed to decode sixel: {:?}",
            decoded.err()
        );
        let img = decoded.unwrap();
        assert!(img.width() > 0);
        assert!(img.height() > 0);
    }
}
