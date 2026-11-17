//! terminal graphics protocol detection and management.
//!
//! supports:
//! - sixel graphics: `DCS q ... ST` / `DCS <params> q ... ST`
//! - kitty graphics protocol: `APC _G <control-data> ; <payload> ST` (or `ESC _ G ... ESC \` / BEL)
//!   implements full protocol including: chunked transmission (m key), image caching (i key),
//!   placement commands (a=p/T/d/q), metadata parsing, and feature queries (e key)
//! - iterm2 inline images: `OSC 1337 ; File = ... ST` / BEL
//!
//! plain (non-graphics) byte spans are surfaced separately so callers can feed
//! them to the vt100 parser in order, keeping the emulator and the graphics
//! tracker consistent within a chunk.

use base64::Engine as _;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// process-wide kitty protocol id generator; ids assigned by the host are
/// unknowable to the multiplexer (quiet mode suppresses the responses), so
/// transmits carry explicit image/placement ids.
static IMAGE_ID: AtomicU32 = AtomicU32::new(1_000_000);
static PLACEMENT_ID: AtomicU32 = AtomicU32::new(2_000_000);

pub fn next_image_id() -> u32 {
    IMAGE_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn next_placement_id() -> u32 {
    PLACEMENT_ID.fetch_add(1, Ordering::Relaxed)
}

/// complete Kitty graphics control metadata.
#[derive(Debug, Clone, Default)]
#[allow(non_snake_case)]
pub struct KittyControlData {
    /// action: T (transmit+display), t (transmit only), p (place), d (delete), q (query)
    pub a: Option<String>,
    /// format: 24 (RGB), 32 (RGBA), 100 (PNG), 8 (indexed 8bit), 16 (indexed 16bit)
    pub f: Option<u32>,
    /// image width in pixels (for RGB/RGBA)
    pub s: Option<u32>,
    /// image height in pixels (for RGB/RGBA)
    pub v: Option<u32>,
    /// image id (for caching and reuse)
    pub i: Option<u32>,
    /// image id to delete (for a=d)
    pub d: Option<String>,
    /// more chunks: 0=final, 1=more coming
    pub m: Option<u8>,
    /// quiet mode: 0=report status, 1=reserved, 2=no response
    pub q: Option<u8>,
    /// transmission medium: d (direct), f (file), t (temp), s (shared memory)
    pub t: Option<String>,
    /// compression: z (zlib deflate)
    pub o: Option<String>,
    /// size of pixel data (for PNG with compression)
    pub S: Option<u64>,
    /// offset into file/shm
    pub O: Option<u64>,
    /// display width in cells
    pub c: Option<u32>,
    /// display height in cells
    pub r: Option<u32>,
    /// source rectangle x
    pub x: Option<u32>,
    /// source rectangle y
    pub y: Option<u32>,
    /// source rectangle width
    pub w: Option<u32>,
    /// source rectangle height
    pub h: Option<u32>,
    /// z-index (stacking order)
    pub z: Option<i32>,
    /// pixel offset x within cell
    pub X: Option<u32>,
    /// pixel offset y within cell
    pub Y: Option<u32>,
    /// cursor movement: 0=move (default), 1=no movement
    pub C: Option<u8>,
    /// placement id (paired with image id)
    pub p: Option<u32>,
    /// unicode placeholder: 1=use U+10EEEE as placeholder
    pub U: Option<u8>,
    /// feature query: T=transmit, f=animation, etc.
    pub e: Option<String>,
    /// image id to replace (alternate to i)
    pub I: Option<u32>,
}

impl KittyControlData {
    /// parse control data string (comma-separated key=value pairs)
    pub fn parse(s: &str) -> Self {
        let mut data = KittyControlData::default();
        for kv in s.split(',') {
            if let Some((k, v)) = kv.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                match k {
                    "a" => data.a = Some(v.to_string()),
                    "f" => data.f = v.parse().ok(),
                    "s" => data.s = v.parse().ok(),
                    "v" => data.v = v.parse().ok(),
                    "i" => data.i = v.parse().ok(),
                    "d" => data.d = Some(v.to_string()),
                    "m" => data.m = v.parse().ok(),
                    "q" => data.q = v.parse().ok(),
                    "t" => data.t = Some(v.to_string()),
                    "o" => data.o = Some(v.to_string()),
                    "S" => data.S = v.parse().ok(),
                    "O" => data.O = v.parse().ok(),
                    "c" => data.c = v.parse().ok(),
                    "r" => data.r = v.parse().ok(),
                    "x" => data.x = v.parse().ok(),
                    "y" => data.y = v.parse().ok(),
                    "w" => data.w = v.parse().ok(),
                    "h" => data.h = v.parse().ok(),
                    "z" => data.z = v.parse().ok(),
                    "X" => data.X = v.parse().ok(),
                    "Y" => data.Y = v.parse().ok(),
                    "C" => data.C = v.parse().ok(),
                    "p" => data.p = v.parse().ok(),
                    "U" => data.U = v.parse().ok(),
                    "e" => data.e = Some(v.to_string()),
                    "I" => data.I = v.parse().ok(),
                    _ => {}
                }
            }
        }
        data
    }
}

impl std::fmt::Display for KittyControlData {
    /// formats back to control string (for transmission).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if let Some(ref v) = self.a {
            parts.push(format!("a={}", v));
        }
        if let Some(v) = self.f {
            parts.push(format!("f={}", v));
        }
        if let Some(v) = self.s {
            parts.push(format!("s={}", v));
        }
        if let Some(v) = self.v {
            parts.push(format!("v={}", v));
        }
        if let Some(v) = self.i {
            parts.push(format!("i={}", v));
        }
        if let Some(ref v) = self.d {
            parts.push(format!("d={}", v));
        }
        if let Some(v) = self.m {
            parts.push(format!("m={}", v));
        }
        if let Some(v) = self.q {
            parts.push(format!("q={}", v));
        }
        if let Some(ref v) = self.t {
            parts.push(format!("t={}", v));
        }
        if let Some(ref v) = self.o {
            parts.push(format!("o={}", v));
        }
        if let Some(v) = self.S {
            parts.push(format!("S={}", v));
        }
        if let Some(v) = self.O {
            parts.push(format!("O={}", v));
        }
        if let Some(v) = self.c {
            parts.push(format!("c={}", v));
        }
        if let Some(v) = self.r {
            parts.push(format!("r={}", v));
        }
        if let Some(v) = self.x {
            parts.push(format!("x={}", v));
        }
        if let Some(v) = self.y {
            parts.push(format!("y={}", v));
        }
        if let Some(v) = self.w {
            parts.push(format!("w={}", v));
        }
        if let Some(v) = self.h {
            parts.push(format!("h={}", v));
        }
        if let Some(v) = self.z {
            parts.push(format!("z={}", v));
        }
        if let Some(v) = self.X {
            parts.push(format!("X={}", v));
        }
        if let Some(v) = self.Y {
            parts.push(format!("Y={}", v));
        }
        if let Some(v) = self.C {
            parts.push(format!("C={}", v));
        }
        if let Some(v) = self.p {
            parts.push(format!("p={}", v));
        }
        if let Some(v) = self.U {
            parts.push(format!("U={}", v));
        }
        if let Some(ref v) = self.e {
            parts.push(format!("e={}", v));
        }
        if let Some(v) = self.I {
            parts.push(format!("I={}", v));
        }
        f.write_str(&parts.join(","))
    }
}

/// kind of graphic payload recognized by terminal protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicFormat {
    Sixel,
    Kitty,
    Iterm2,
}

/// a segment of the pane byte stream: either bytes for the vt100 parser, or a
/// captured graphics escape sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Plain(Vec<u8>),
    Graphic(Box<GraphicCommand>),
}

/// represents an in-flight chunked Kitty graphics transmission.
/// the protocol allows splitting large payloads into multiple escape codes
/// using the `m` key (0=final, 1=more chunks).
#[derive(Debug, Clone)]
pub struct KittyChunkBuffer {
    /// control metadata from first chunk (contains format, dimensions, etc.)
    pub control: KittyControlData,
    /// accumulated base64-encoded payload across all chunks
    pub payload: Vec<u8>,
}

/// placement and raw byte command of a graphic sequence.
#[derive(Debug, Clone)]
pub struct GraphicCommand {
    pub format: GraphicFormat,
    /// monotonic id of the image in the owning pane, assigned by the store.
    pub seq: u64,
    /// pane cell row when emitted; negative when scrolled above the viewport
    pub row: i32,
    /// pane cell col when emitted
    pub col: u16,
    /// whether the child emitted this command while using its alternate screen.
    pub alternate_screen: bool,
    /// raw protocol escape sequence bytes (complete, not chunked)
    pub data: Vec<u8>,
    /// pixel data of a file-backed transmission (kitty `t=f` / `t=s`).
    /// the app deletes the file right after the transmission, so the bytes are
    /// read once and kept: an ED2 repaint can then re-transmit the image
    /// instead of losing it (zellij reads the file the same way).
    pub file_bytes: Option<Vec<u8>>,
    /// parsed Kitty metadata (if Kitty format)
    pub kitty_metadata: Option<KittyControlData>,
    /// bytes to re-place the image after its pixels were transmitted once.
    /// for kitty images with a known `i=` id this is an a=p placement command
    /// referencing the stored pixels, so the terminal re-draws them without
    /// the app re-sending its payload. none for sixel/iterm2 and anonymous
    /// kitty images, whose stores keep no image id to reference.
    pub reemit: Option<Vec<u8>>,
    /// pixel size for sixel images, parsed from raster attributes (`"Pan;Pad;Ph;Pv`)
    /// or counted from the data (one pixel per column, six pixels per band).
    /// none for kitty/iterm2 and for sixels whose size cannot be determined.
    pub pixel_size: Option<(u32, u32)>,
}

/// placement rectangle in cells for a kitty command, if enough information exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GraphicPlacementBounds {
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

impl GraphicCommand {
    fn kitty_control_and_payload(&self) -> Option<(Vec<&str>, Option<&[u8]>)> {
        let (start, marker_len) = find_kitty_apc(&self.data, 0)?;
        let control_start = start + marker_len;
        let (terminator_start, _terminator_len) = find_kitty_terminator(&self.data, control_start)?;
        let control_end = self.data[control_start..terminator_start]
            .iter()
            .position(|byte| *byte == b';')
            .map(|offset| control_start + offset)
            .unwrap_or(terminator_start);
        let control = std::str::from_utf8(&self.data[control_start..control_end]).ok()?;
        let payload =
            (control_end < terminator_start).then(|| &self.data[control_end + 1..terminator_start]);
        Some((control.split(',').collect(), payload))
    }

    /// pixel size of a kitty transmission: the declared size, or the size
    /// decoded from the png payload when the transmission carries one.
    pub fn kitty_pixel_size(&self) -> Option<(u32, u32)> {
        let metadata = self.kitty_metadata.as_ref()?;
        if let (Some(width), Some(height)) = (metadata.s, metadata.v) {
            return Some((width, height));
        }
        if metadata.f != Some(100) {
            return None;
        }
        let (_, payload) = self.kitty_control_and_payload()?;
        png_dimensions_from_base64(payload?)
    }

    pub(crate) fn placement_bounds(&self) -> Option<GraphicPlacementBounds> {
        let metadata = self.kitty_metadata.as_ref()?;
        if !matches!(metadata.a.as_deref(), Some("T" | "p")) || metadata.U == Some(1) {
            return None;
        }
        if metadata.c.is_some() && metadata.r.is_some() {
            return Some(GraphicPlacementBounds {
                cols: Some(metadata.c?.min(u16::MAX as u32) as u16),
                rows: Some(metadata.r?.min(u16::MAX as u32) as u16),
            });
        }

        let pixels = self
            .kitty_control_and_payload()
            .and_then(|(fields, payload)| kitty_pixel_dimensions(&fields, payload));

        let source_width = metadata.w.or(pixels.map(|(width, _)| width));
        let source_height = metadata.h.or(pixels.map(|(_, height)| height));
        let (cell_width, cell_height) = crate::util::outer_term::cell_pixel_size();
        let cell_width = cell_width.max(1);
        let cell_height = cell_height.max(1);
        let mut cols = metadata.c.map(|c| c.clamp(1, u16::MAX as u32) as u16);
        let mut rows = metadata.r.map(|r| r.clamp(1, u16::MAX as u32) as u16);

        if cols.is_none()
            && let Some(width) = source_width
            && width > 0
        {
            cols = Some(div_ceil_u32(width, cell_width).min(u16::MAX as u32) as u16);
        }
        if rows.is_none()
            && let Some(height) = source_height
            && height > 0
        {
            rows = Some(div_ceil_u32(height, cell_height).min(u16::MAX as u32) as u16);
        }

        if let (Some(width), Some(height)) = (source_width, source_height)
            && width > 0
            && height > 0
        {
            if cols.is_some()
                && rows.is_none()
                && let Some(c) = cols
            {
                let scale = (f64::from(c) * f64::from(cell_width)) / f64::from(width);
                rows = Some(
                    (f64::from(height) * scale / f64::from(cell_height))
                        .ceil()
                        .max(1.0)
                        .min(f64::from(u16::MAX)) as u16,
                );
            } else if rows.is_some()
                && cols.is_none()
                && let Some(r) = rows
            {
                let scale = (f64::from(r) * f64::from(cell_height)) / f64::from(height);
                cols = Some(
                    (f64::from(width) * scale / f64::from(cell_width))
                        .ceil()
                        .max(1.0)
                        .min(f64::from(u16::MAX)) as u16,
                );
            }
        }

        (cols.is_some() || rows.is_some()).then_some(GraphicPlacementBounds { cols, rows })
    }

    /// returns true when the placement is fully contained in a pane. unknown size is considered fit.
    #[cfg(test)]
    pub(crate) fn placement_contained(
        &self,
        pane_cols: u16,
        pane_rows: u16,
        scroll_offset: usize,
    ) -> bool {
        if self.format != GraphicFormat::Kitty {
            return true;
        }
        let Some(metadata) = self.kitty_metadata.as_ref() else {
            return true;
        };
        let row = self.row + scroll_offset as i32;
        if row < 0 {
            return false;
        }
        let row = row as u16;
        // virtual placeholders are clipped by their anchoring placeholder cells.
        if metadata.U == Some(1) {
            return row < pane_rows && self.col < pane_cols;
        }
        if !matches!(metadata.a.as_deref(), Some("T" | "p")) {
            return true;
        }
        let Some(bounds) = self.placement_bounds() else {
            return true;
        };
        let fits_columns = bounds
            .cols
            .is_none_or(|cols| self.col.saturating_add(cols) <= pane_cols);
        let fits_rows = bounds
            .rows
            .is_none_or(|rows| row.saturating_add(rows) <= pane_rows);
        fits_columns && fits_rows
    }

    /// the vertical fragment of a direct-kitty placement visible in the
    /// viewport. returns (viewport_top, viewport_rows); None when the image
    /// is entirely above or below the viewport.
    pub fn visible_vertical_fragment(
        &self,
        pane_rows: u16,
        scroll_offset: usize,
    ) -> Option<(i32, i32)> {
        if self.format != GraphicFormat::Kitty {
            return None;
        }
        let metadata = self.kitty_metadata.as_ref()?;
        if !matches!(metadata.a.as_deref(), Some("T" | "p")) || metadata.U == Some(1) {
            return None;
        }
        let top = self.row + scroll_offset as i32;
        let full_rows = self
            .placement_bounds()
            .and_then(|bounds| bounds.rows)
            .map_or(i32::from(pane_rows.max(1)), |rows| i32::from(rows.max(1)));
        let vis_top = top.max(0);
        let vis_bottom = (top + full_rows).min(i32::from(pane_rows));
        if vis_bottom <= vis_top {
            return None;
        }
        Some((vis_top, vis_bottom - vis_top))
    }

    /// true when the placement still touches the viewport: at least one
    /// visible v fragment and (when known) the column extent fits the pane.
    pub fn visible_in_pane(&self, pane_cols: u16, pane_rows: u16, scroll_offset: usize) -> bool {
        if self
            .kitty_metadata
            .as_ref()
            .is_some_and(|metadata| metadata.U == Some(1))
        {
            // virtual placements are anchored to their placeholder cells
            let row = self.row + scroll_offset as i32;
            return row >= 0 && row < i32::from(pane_rows) && self.col < pane_cols;
        }
        // a placement wider or taller than the pane is still visible: the
        // image is cropped to the pane (like zellij clips an image quad to
        // its owning pane), so it must not be hidden.
        self.visible_vertical_fragment(pane_rows, scroll_offset)
            .is_some()
    }

    /// crops a kitty placement to the part of it that is visible inside the
    /// pane, the way zellij clips an image quad to its owning pane.
    ///
    /// a placement that does not fit the pane is not hidden: it is re-placed
    /// with the source rectangle cut down to the visible cells, so a big
    /// screenshot drawn with `kitten icat` shows the part that fits instead
    /// of disappearing. returns the delete+place sequence and the viewport
    /// cell the cropped fragment is anchored at. `None` when the placement
    /// fits completely (nothing to crop).
    /// bytes to hand to the host for a full re-transmission of this command.
    ///
    /// a kitty command whose payload exceeds the protocol's 4096-byte base64
    /// chunk limit is re-emitted as a properly chunked `m=1`/`m=0` sequence;
    /// the host silently drops oversized single apcs. everything else is
    /// forwarded verbatim.
    pub fn emission_payload(&self) -> Vec<u8> {
        if self.format == GraphicFormat::Kitty
            && let Some(metadata) = self.kitty_metadata.as_ref()
            && let Some((_, payload)) = self.kitty_control_and_payload()
            && kitty_needs_chunking(payload.map_or(0, <[u8]>::len))
            && let Some(payload) = payload
        {
            let mut control = metadata.clone();
            control.m = None;
            return kitty_chunked_apc(&control, payload);
        }
        self.data.clone()
    }

    /// true when the command transmits image data through a file (t=f) or
    /// shared memory (t=s). the host reads the file when the transmission
    /// arrives; the file is gone a moment later (icat deletes it), so such a
    /// transmission must never be re-sent - only the placement can be
    /// re-created.
    pub fn reads_pixels_from_file(&self) -> bool {
        self.format == GraphicFormat::Kitty
            && self
                .kitty_metadata
                .as_ref()
                .and_then(|metadata| metadata.t.as_deref())
                .is_some_and(|medium| matches!(medium, "f" | "s"))
    }

    /// reads the pixel data of a file-backed kitty transmission (`t=f` for a
    /// temp file, `t=s` for shared memory).
    ///
    /// the app deletes the file right after the transmission, so the bytes
    /// have to be read while the file still exists - otherwise an ED2 repaint
    /// wipes the image on the host and there is nothing left to re-transmit.
    /// zellij reads the file the same way.
    pub fn read_transmitted_file(&mut self) -> Option<Vec<u8>> {
        let metadata = self.kitty_metadata.as_ref()?;
        let medium = metadata.t.as_deref()?;
        if !matches!(medium, "f" | "s") {
            return None;
        }
        if !matches!(metadata.a.as_deref(), Some("t") | Some("T")) {
            return None;
        }
        // the payload of a file transmission is the base64 encoded path
        let (_, payload) = self.kitty_control_and_payload()?;
        let encoded = payload?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let path = String::from_utf8(decoded).ok()?;
        let path = path.trim_end_matches('\0').to_string();
        let bytes = if medium == "s" {
            // shared memory: the payload is the file name inside /dev/shm
            std::fs::read(format!("/dev/shm/{path}")).ok()?
        } else {
            std::fs::read(&path).ok()?
        };
        if bytes.len() > MAX_TRANSMITTED_FILE_BYTES {
            return None;
        }
        // rewrite the command to a direct (`t=d`) chunked transmission right
        // away: the app deletes the file the moment it is sent, so the host
        // would otherwise try to read a path that no longer exists and the
        // first frame would be blank. with the pixels inline the first frame
        // already carries the image.
        let mut control = KittyControlData {
            a: Some("T".to_string()),
            f: metadata.f,
            i: metadata.i,
            p: metadata.p,
            q: Some(2),
            t: Some("d".to_string()),
            s: metadata.s,
            v: metadata.v,
            c: metadata.c,
            r: metadata.r,
            ..Default::default()
        };
        if control.i.is_none() {
            control.i = Some(next_image_id());
        }
        self.data = kitty_chunked_apc(&control, &bytes);
        self.file_bytes = Some(bytes);
        None
    }

    /// re-transmission sequence for a file-backed image: the pixels are sent
    /// inline (`t=d`) so the host does not need the (already deleted) file.
    /// the payload is chunked at the protocol's 4096-byte limit, exactly like
    /// the app's own transmission was.
    pub fn retransmit_with_cached_bytes(&self) -> Option<Vec<u8>> {
        let bytes = self.file_bytes.as_ref()?;
        let metadata = self.kitty_metadata.as_ref()?;
        if !matches!(metadata.a.as_deref(), Some("t") | Some("T")) {
            return None;
        }
        let mut control = KittyControlData {
            a: Some("T".to_string()),
            f: metadata.f,
            i: metadata.i,
            p: metadata.p,
            q: Some(2),
            t: Some("d".to_string()),
            c: metadata.c,
            r: metadata.r,
            ..Default::default()
        };
        if control.i.is_none() {
            control.i = Some(next_image_id());
        }
        Some(kitty_chunked_apc(&control, bytes))
    }

    pub fn crop_placement(
        &self,
        pane_cols: u16,
        pane_rows: u16,
        scroll_offset: usize,
    ) -> Option<(Vec<u8>, u16, u16)> {
        let metadata = self.kitty_metadata.as_ref()?;
        let id = metadata.i?;
        let (pixels, payload) = self.kitty_control_and_payload()?;
        let (full_w, full_h) = kitty_pixel_dimensions(&pixels, payload)?;
        let (cell_w, cell_h) = crate::util::outer_term::cell_pixel_size();
        let cell_w = i64::from(cell_w.max(1));
        let cell_h = i64::from(cell_h.max(1));

        let src_x0 = i64::from(metadata.x.unwrap_or(0));
        let src_y0 = i64::from(metadata.y.unwrap_or(0));
        let src_w = i64::from(metadata.w.unwrap_or(full_w));
        let src_h = i64::from(metadata.h.unwrap_or(full_h));

        // the placement occupies this many cells
        let place_cols = metadata
            .c
            .map(i64::from)
            .or_else(|| self.placement_bounds().and_then(|b| b.cols).map(i64::from))
            .unwrap_or_else(|| i64::from(div_ceil_u32(src_w as u32, cell_w as u32)));
        let place_rows = metadata
            .r
            .map(i64::from)
            .or_else(|| self.placement_bounds().and_then(|b| b.rows).map(i64::from))
            .unwrap_or_else(|| i64::from(div_ceil_u32(src_h as u32, cell_h as u32)));
        let place_cols = place_cols.max(1);
        let place_rows = place_rows.max(1);

        // the placement's origin in pane cells (the row is relative to the
        // live grid, the scroll offset moves the viewport up into history)
        let origin_col = i64::from(self.col);
        let origin_row =
            i64::from(self.row).saturating_add(i64::try_from(scroll_offset).unwrap_or(i64::MAX));

        // the visible part of the placement inside the pane
        let vis_x0 = origin_col.max(0);
        let vis_y0 = origin_row.max(0);
        let vis_x1 = origin_col
            .saturating_add(place_cols)
            .min(i64::from(pane_cols));
        let vis_y1 = origin_row
            .saturating_add(place_rows)
            .min(i64::from(pane_rows));
        if vis_x1 <= vis_x0 || vis_y1 <= vis_y0 {
            // nothing of the placement is inside the pane
            return None;
        }
        let fully_visible = vis_x0 == origin_col
            && vis_y0 == origin_row
            && vis_x1 == origin_col + place_cols
            && vis_y1 == origin_row + place_rows;
        if fully_visible {
            return None;
        }

        // cut the source rectangle down to the visible cells. The source
        // pixels per cell are the placement's own scale (the app may have
        // scaled the image into fewer cells than its pixel size suggests),
        // not the terminal's cell size: using the cell size pointed the
        // crop at the wrong pixels and the host drew a black rectangle.
        let px_per_col = src_w as f64 / place_cols as f64;
        let px_per_row = src_h as f64 / place_rows as f64;
        let cut_x = ((vis_x0 - origin_col) as f64 * px_per_col).round() as i64;
        let cut_y = ((vis_y0 - origin_row) as f64 * px_per_row).round() as i64;
        let crop_w = (((vis_x1 - vis_x0) as f64) * px_per_col)
            .round()
            .min((src_w - cut_x).max(1) as f64) as i64;
        let crop_h = (((vis_y1 - vis_y0) as f64) * px_per_row)
            .round()
            .min((src_h - cut_y).max(1) as f64) as i64;

        let del_ctl = KittyControlData {
            a: Some("d".to_string()),
            d: Some("i".to_string()),
            i: Some(id),
            p: metadata.p,
            q: Some(2),
            ..Default::default()
        };
        let mut place_ctl = KittyControlData {
            a: Some("p".to_string()),
            i: Some(id),
            p: metadata.p,
            q: Some(2),
            ..Default::default()
        };
        place_ctl.c = Some(u32::try_from(vis_x1 - vis_x0).ok()?);
        place_ctl.r = Some(u32::try_from(vis_y1 - vis_y0).ok()?);
        place_ctl.x = Some(src_x0.saturating_add(cut_x).clamp(0, i64::from(u32::MAX)) as u32);
        place_ctl.y = Some(src_y0.saturating_add(cut_y).clamp(0, i64::from(u32::MAX)) as u32);
        place_ctl.w = Some(u32::try_from(crop_w.max(1)).ok()?);
        place_ctl.h = Some(u32::try_from(crop_h.max(1)).ok()?);
        place_ctl.z = metadata.z;
        place_ctl.X = metadata.X;
        place_ctl.Y = metadata.Y;
        place_ctl.C = metadata.C;

        let mut out = kitty_apc(&del_ctl);
        out.extend_from_slice(&kitty_apc(&place_ctl));
        Some((
            out,
            u16::try_from(vis_x0.clamp(0, i64::from(pane_cols.saturating_sub(1)))).ok()?,
            u16::try_from(vis_y0.clamp(0, i64::from(pane_rows.saturating_sub(1)))).ok()?,
        ))
    }

    /// constrains a kitty placement to the cells remaining in its pane.
    /// kitty treats `c` and `r` as a bounding box and preserves image aspect ratio.
    pub fn fit_kitty_to_bounds(&mut self, max_cols: u16, max_rows: u16) {
        if self.format != GraphicFormat::Kitty {
            return;
        }
        let Some(metadata) = self.kitty_metadata.as_mut() else {
            return;
        };
        if matches!(metadata.a.as_deref(), Some("d" | "q" | "t")) {
            return;
        }
        // preserve native-size placements. Adding a c/r box here can upscale a
        // small logo to fill the pane because Kitty treats both values as a
        // scaling target, not merely a clipping boundary.
        if metadata.c.is_none() && metadata.r.is_none() {
            return;
        }

        let max_cols = u32::from(max_cols.max(1));
        let max_rows = u32::from(max_rows.max(1));
        match (metadata.c, metadata.r) {
            (Some(cols), Some(rows)) => {
                let requested_cols = cols.max(1);
                let requested_rows = rows.max(1);
                let scale = (max_cols as f64 / requested_cols as f64)
                    .min(max_rows as f64 / requested_rows as f64)
                    .min(1.0);
                metadata.c = Some(((requested_cols as f64 * scale).floor() as u32).max(1));
                metadata.r = Some(((requested_rows as f64 * scale).floor() as u32).max(1));
            }
            (Some(cols), None) => metadata.c = Some(cols.max(1).min(max_cols)),
            (None, Some(rows)) => metadata.r = Some(rows.max(1).min(max_rows)),
            (None, None) => return,
        }
        if let Some(data) = rewrite_kitty_control(&self.data, metadata) {
            self.data = data;
        }
        self.reemit = metadata
            .a
            .as_deref()
            .filter(|action| matches!(*action, "T" | "p"))
            .and_then(|_| reemit_placement(metadata));
    }

    /// clamps a graphic command's positioning to respect pane boundaries.
    /// for virtual placeholders (U=1), ensures the anchor row/col stay within bounds.
    /// for real placements, constrains to prevent visible overflow.
    pub fn clamp_to_pane_bounds(&mut self, pane_cols: u16, pane_rows: u16) {
        // virtual placeholders are text-anchored; just ensure the anchor is in bounds
        if let Some(metadata) = self.kitty_metadata.as_ref()
            && metadata.U == Some(1)
        {
            if self.row >= i32::from(pane_rows) {
                self.row = i32::from(pane_rows) - 1;
            }
            if self.col >= pane_cols {
                self.col = pane_cols.saturating_sub(1);
            }
            return;
        }

        // for non-virtual placements, fit to bounds
        self.fit_kitty_to_bounds(pane_cols, pane_rows);
    }

    /// returns the protocol sequence needed after a terminal clear erased its
    /// graphics. unlike `reemit`, this restores image data before placements.
    pub fn restore_after_clear(&self) -> Option<Vec<u8>> {
        if self.format != GraphicFormat::Kitty {
            return Some(self.data.clone());
        }
        let metadata = self.kitty_metadata.as_ref()?;
        match metadata.a.as_deref() {
            Some("d") | Some("q") => None,
            Some("t") | Some("T") => {
                let mut data = Vec::new();
                if let Some(id) = metadata.i {
                    let delete = KittyControlData {
                        a: Some("d".to_string()),
                        d: Some("I".to_string()),
                        i: Some(id),
                        q: Some(2),
                        ..Default::default()
                    };
                    data.extend_from_slice(&kitty_apc(&delete));
                }
                data.extend_from_slice(&self.data);
                Some(data)
            }
            _ => Some(self.data.clone()),
        }
    }
}

/// renders kitty control data as a complete payload-free escape sequence
/// (`\x1b_G...\x1b\\`).
pub fn kitty_apc(control: &KittyControlData) -> Vec<u8> {
    let control_string = control.to_string();
    let mut data = Vec::with_capacity(control_string.len() + 6);
    data.extend_from_slice(b"\x1b_G");
    data.extend_from_slice(control_string.as_bytes());
    data.extend_from_slice(b"\x1b\\");
    data
}

/// the kitty protocol caps a single apc payload at 4096 base64 bytes and
/// requires chunked transfers: every chunk but the last carries `m=1`, the
/// last carries `m=0`, and continuation chunks carry only `m` (and `q`).
/// a host that receives one oversized apc rejects it or drops the image
/// silently, so anything we re-transmit inline is chunked here.
pub fn kitty_chunked_apc(control: &KittyControlData, payload: &[u8]) -> Vec<u8> {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
    let encoded = encoded.as_bytes();
    let mut out = Vec::with_capacity(encoded.len() + encoded.len() / 4096 * 24 + 32);
    let mut first = true;
    let mut offset = 0usize;
    while offset < encoded.len() {
        let end = (offset + 4096).min(encoded.len());
        let last = end == encoded.len();
        let mut chunk_ctl = if first {
            control.clone()
        } else {
            KittyControlData::default()
        };
        chunk_ctl.q = Some(2);
        chunk_ctl.m = Some(if last { 0 } else { 1 });
        out.extend_from_slice(b"\x1b_G");
        out.extend_from_slice(chunk_ctl.to_string().as_bytes());
        out.push(b';');
        out.extend_from_slice(&encoded[offset..end]);
        out.extend_from_slice(b"\x1b\\");
        offset = end;
        first = false;
    }
    out
}

/// true when a kitty payload is large enough that it must be chunked.
pub fn kitty_needs_chunking(payload_len: usize) -> bool {
    payload_len > 4096
}

impl PartialEq for GraphicCommand {
    fn eq(&self, other: &Self) -> bool {
        self.seq == other.seq && self.data == other.data
    }
}

impl Eq for GraphicCommand {}

/// abort collection of a graphic sequence beyond this many bytes; the
/// sequence is dropped and the parser resynchronizes.
const MAX_GRAPHIC_SEQ: usize = 32 * 1024 * 1024;
const MAX_KITTY_CHUNKED_PAYLOAD: usize = 8 * 1024 * 1024;
/// upper bound for a file-backed transmission we keep around for re-sends
const MAX_TRANSMITTED_FILE_BYTES: usize = 32 * 1024 * 1024;
const MAX_STORED_GRAPHIC_COMMANDS: usize = 64;
const MAX_STORED_GRAPHIC_BYTES: usize = 8 * 1024 * 1024;
const MAX_STORED_SIXEL_BYTES: usize = 32 * 1024 * 1024;

/// upper bound for a DCS introducer (ESC P + params + final byte).
const MAX_DCS_INTRODUCER: usize = 34;

/// internal lexer state for the byte splitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// ordinary output; bytes accumulate into `plain`.
    Ground,
    /// saw ESC P; collecting dcs params and final byte into `seq`.
    DcsParams,
    /// probing literal "1337;File" after ESC ] to recognize iterm2 images.
    Osc1337,
    /// collecting a complete graphic sequence into `seq`.
    Graphics(GraphicFormat),
    /// dropping the remainder of an oversized sequence through its terminator.
    DiscardingGraphic(GraphicFormat, bool),
}

/// stateful parser that extracts sixel, kitty, and iterm2 graphic sequences from raw pty byte streams.
#[derive(Debug)]
pub struct GraphicsParser {
    plain: Vec<u8>,
    seq: Vec<u8>,
    state: State,
    /// utf-8 continuation bytes cannot be interpreted as 8-bit C1 controls.
    utf8_remaining: u8,
    /// active Kitty chunked transmission (None = waiting for new sequence, Some = in progress)
    kitty_chunk: Option<KittyChunkBuffer>,
    kitty_chunk_dropping: bool,
}

impl Default for GraphicsParser {
    fn default() -> Self {
        Self {
            plain: Vec::with_capacity(8192),
            seq: Vec::with_capacity(1024),
            state: State::Ground,
            utf8_remaining: 0,
            kitty_chunk: None,
            kitty_chunk_dropping: false,
        }
    }
}

impl GraphicsParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// splits incoming pty bytes into plain and graphics segments, in order.
    pub fn split(&mut self, bytes: &[u8]) -> Vec<Segment> {
        let mut segments = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            match self.state {
                State::Ground => self.ground(b, &mut segments),
                State::DcsParams => self.dcs_params(b),
                State::Osc1337 => self.osc1337(b, &mut segments),
                State::Graphics(kind) => self.collect(b, kind, &mut segments),
                State::DiscardingGraphic(kind, previous_escape) => {
                    self.discard_graphic_byte(b, kind, previous_escape)
                }
            }
            i += 1;
        }

        if !self.plain.is_empty() {
            segments.push(Segment::Plain(std::mem::take(&mut self.plain)));
        }
        segments
    }

    fn flush_plain(&mut self, segments: &mut Vec<Segment>) {
        if !self.plain.is_empty() {
            segments.push(Segment::Plain(std::mem::take(&mut self.plain)));
        }
    }

    /// ground state: sniff for the start of a graphic sequence.
    fn ground(&mut self, b: u8, segments: &mut Vec<Segment>) {
        if self.utf8_remaining > 0 {
            // an ESC always starts a sequence, even in the middle of a
            // multi-byte character: swallowing it made the whole kitty
            // transmission fall through to the text stream and the image
            // payload was printed as garbage ("AAAA..." everywhere). zellij's
            // APC filter has the same rule - a sequence introducer wins over
            // an incomplete utf-8 sequence.
            if b != 0x1b {
                self.plain.push(b);
                self.utf8_remaining -= 1;
                return;
            }
            self.utf8_remaining = 0;
        }
        match b {
            0x1b => {
                // possible sequence introducer: hold it in seq until we know.
                self.flush_plain(segments);
                self.seq.clear();
                self.seq.push(0x1b);
                self.state = State::DcsParams;
            }
            0x90 => {
                self.flush_plain(segments);
                self.seq.clear();
                self.seq.push(b);
                self.state = State::DcsParams;
            }
            0x9d => {
                self.flush_plain(segments);
                self.seq.clear();
                self.seq.push(b);
                self.state = State::Osc1337;
            }
            0x9f => {
                self.flush_plain(segments);
                self.seq.clear();
                self.seq.push(b);
                self.state = State::Graphics(GraphicFormat::Kitty);
            }
            0xc2..=0xdf => {
                self.plain.push(b);
                self.utf8_remaining = 1;
            }
            0xe0..=0xef => {
                self.plain.push(b);
                self.utf8_remaining = 2;
            }
            0xf0..=0xf4 => {
                self.plain.push(b);
                self.utf8_remaining = 3;
            }
            _ => self.plain.push(b),
        }
    }

    /// after ESC we must distinguish DCS (params .. final), APC _G (kitty),
    /// and OSC 1337;File (iterm2). seq[0] == 0x1b, and seq holds what we
    /// have seen so far.
    fn dcs_params(&mut self, b: u8) {
        // recall: after ESC, the first meaningful byte decides the kind.
        if self.seq.first() == Some(&0x90) {
            match b {
                0x20..=0x3f => {
                    self.seq.push(b);
                    if self.seq.len() > MAX_DCS_INTRODUCER {
                        self.flush_seq_to_plain();
                        self.state = State::Ground;
                    }
                }
                0x40..=0x7e => {
                    self.seq.push(b);
                    self.dcs_final();
                }
                _ => {
                    self.flush_seq_to_plain();
                    self.plain.push(b);
                    self.state = State::Ground;
                }
            }
            return;
        }
        match self.seq.len() {
            1 => match b {
                b'P' => {
                    self.seq.push(b);
                }
                b'_' => {
                    self.seq.push(b);
                }
                b']' => {
                    self.seq.push(b);
                }
                _ => {
                    // not a graphics introducer: hand ESC + byte back to plain.
                    let mut esc = std::mem::take(&mut self.seq);
                    self.plain.append(&mut esc);
                    self.plain.push(b);
                    self.state = State::Ground;
                }
            },
            2 => match (self.seq.get(1), b) {
                (Some(&b'P'), 0x20..=0x3f) => {
                    self.seq.push(b);
                }
                (Some(&b'P'), 0x40..=0x7e) => {
                    // esc p with no params: sixel only if final byte is q.
                    self.seq.push(b);
                    self.dcs_final();
                }
                (Some(&b'_'), b'G') => {
                    self.seq.push(b);
                    self.state = State::Graphics(GraphicFormat::Kitty);
                }
                (Some(&b']'), b'1') => {
                    // possible iterm2: probe "1337;File" starting at '1'
                    self.seq.push(b);
                    self.state = State::Osc1337;
                }
                _ => {
                    // not a graphics introducer: hand bytes back to plain.
                    let mut esc = std::mem::take(&mut self.seq);
                    self.plain.append(&mut esc);
                    self.plain.push(b);
                    self.state = State::Ground;
                }
            },
            _ => self.dcs_param_byte(b),
        }
    }

    /// collecting DCS parameter/intermediate bytes and waiting for the final byte.
    fn dcs_param_byte(&mut self, b: u8) {
        const DCS_PARAM: std::ops::RangeInclusive<u8> = 0x20..=0x3f;
        const DCS_FINAL: std::ops::RangeInclusive<u8> = 0x40..=0x7e;
        if DCS_PARAM.contains(&b) {
            self.seq.push(b);
            if self.seq.len() > MAX_DCS_INTRODUCER {
                self.flush_seq_to_plain();
                self.state = State::Ground;
            }
            return;
        }
        if DCS_FINAL.contains(&b) {
            self.seq.push(b);
            self.dcs_final();
            return;
        }
        // invalid byte inside dcs: abort the sequence, keep bytes as plain.
        self.flush_seq_to_plain();
        self.plain.push(b);
        self.state = State::Ground;
    }

    /// final byte of a DCS arrived: sixel if it is 'q', otherwise not graphics.
    fn dcs_final(&mut self) {
        let is_sixel = self.seq.last() == Some(&b'q');
        if is_sixel {
            self.state = State::Graphics(GraphicFormat::Sixel);
            return;
        }
        self.flush_seq_to_plain();
        self.state = State::Ground;
    }

    /// probing the `1337;File*` family inside an OSC after ESC ]. covers the
    /// single-shot `File=` and the chunked `MultipartFile=`/`FilePart=`/`FileEnd`
    /// commands, which carry no terminator distinguishing them at this layer.
    fn osc1337(&mut self, b: u8, segments: &mut Vec<Segment>) {
        self.seq.push(b);
        const CANDIDATES: &[&[u8]] = &[b"1337;File", b"1337;MultipartFile"];
        let body_start = if self.seq.first() == Some(&0x9d) {
            1
        } else {
            2
        };
        let body = &self.seq[body_start..];
        for cand in CANDIDATES {
            if body.len() <= cand.len() && cand.starts_with(body) {
                self.state = if body.len() == cand.len() {
                    State::Graphics(GraphicFormat::Iterm2)
                } else {
                    State::Osc1337
                };
                return;
            }
        }
        // not iterm2: dump accumulated bytes as plain and reprocess this byte.
        self.seq.pop();
        self.flush_seq_to_plain();
        self.state = State::Ground;
        self.ground(b, segments);
    }

    /// collecting a full graphic sequence until its terminator.
    fn collect(&mut self, b: u8, kind: GraphicFormat, segments: &mut Vec<Segment>) {
        // CAN and SUB abort a sequence in progress (VT100 rules); zellij's
        // APC filter aborts on them as well. without this an aborted
        // transmission kept the parser in the capturing state and the next
        // image was swallowed as text.
        if b == 0x18 || b == 0x1a {
            self.seq.clear();
            self.kitty_chunk = None;
            self.state = State::Ground;
            self.plain.push(b);
            self.flush_plain(segments);
            return;
        }
        self.seq.push(b);
        if self.seq.len() > MAX_GRAPHIC_SEQ {
            tracing::warn!("graphic sequence exceeded {MAX_GRAPHIC_SEQ} bytes; dropping");
            let terminated = self.seq.ends_with(b"\x1b\\")
                || self.seq.ends_with(&[0x9c])
                || (kind != GraphicFormat::Sixel && self.seq.ends_with(&[0x07]));
            let previous_escape = b == 0x1b;
            self.seq.clear();
            self.kitty_chunk = None; // Also clear any in-flight Kitty chunks
            self.state = if terminated {
                State::Ground
            } else {
                State::DiscardingGraphic(kind, previous_escape)
            };
            return;
        }

        let complete = match kind {
            GraphicFormat::Sixel => self.seq.ends_with(b"\x1b\\") || self.seq.ends_with(&[0x9c]),
            GraphicFormat::Kitty | GraphicFormat::Iterm2 => {
                self.seq.ends_with(b"\x1b\\")
                    || self.seq.ends_with(&[0x9c])
                    || self.seq.ends_with(b"\x07")
            }
        };

        if !complete {
            return;
        }

        // sequence is complete; process it based on kind
        match kind {
            GraphicFormat::Kitty => self.handle_kitty_sequence(segments),
            GraphicFormat::Sixel => self.handle_sixel_sequence(segments),
            GraphicFormat::Iterm2 => self.handle_iterm2_sequence(segments),
        }

        self.seq.clear();
        self.state = State::Ground;
        self.utf8_remaining = 0;
    }

    fn discard_graphic_byte(&mut self, byte: u8, kind: GraphicFormat, previous_escape: bool) {
        let terminated = (previous_escape && byte == b'\\')
            || byte == 0x9c
            || (kind != GraphicFormat::Sixel && byte == 0x07);
        self.state = if terminated {
            State::Ground
        } else {
            State::DiscardingGraphic(kind, byte == 0x1b)
        };
    }

    /// handle a complete Kitty graphics sequence, supporting chunked transmission.
    /// sequences with m=1 accumulate their payload into kitty_chunk.
    /// sequences with m=0 finalize the chunked transmission.
    fn handle_kitty_sequence(&mut self, segments: &mut Vec<Segment>) {
        let data = std::mem::take(&mut self.seq);
        let Some(ctl) = kitty_control_data(&data) else {
            tracing::debug!("invalid kitty control data");
            self.kitty_chunk = None;
            return;
        };
        let control = KittyControlData::parse(ctl);
        if control.a.as_deref() == Some("q") {
            self.plain.extend_from_slice(&data);
            return;
        }

        let continuation = kitty_chunk_continuation(ctl);
        if self.kitty_chunk_dropping {
            if let Some(marker) = continuation {
                if marker == 0 {
                    self.kitty_chunk_dropping = false;
                }
                return;
            }
            self.kitty_chunk_dropping = false;
        }

        let payload_start = data
            .iter()
            .position(|byte| *byte == b';')
            .map_or(data.len(), |p| p + 1);
        let payload_end = if data.ends_with(b"\x1b\\") {
            data.len() - 2
        } else if data.ends_with(&[0x9c]) || data.ends_with(b"\x07") {
            data.len() - 1
        } else {
            data.len()
        };
        let payload = data
            .get(payload_start..payload_end)
            .unwrap_or_default()
            .to_vec();

        if let Some(marker) = continuation {
            let Some(mut chunk) = self.kitty_chunk.take() else {
                return;
            };
            let Some(total_len) = chunk.payload.len().checked_add(payload.len()) else {
                self.kitty_chunk_dropping = marker == 1;
                tracing::warn!("kitty graphics transmission exceeded payload limit; dropping");
                return;
            };
            if total_len > MAX_KITTY_CHUNKED_PAYLOAD {
                self.kitty_chunk_dropping = marker == 1;
                tracing::warn!("kitty graphics transmission exceeded payload limit; dropping");
                return;
            }
            chunk.payload.extend_from_slice(&payload);
            if marker == 1 {
                self.kitty_chunk = Some(chunk);
                return;
            }
            chunk.control.m = None;
            self.flush_plain(segments);
            self.emit_kitty_graphic(segments, chunk.control, chunk.payload);
            return;
        }

        // A complete command inside a transfer invalidates the unfinished
        // image. Process the new command on its own instead of treating its
        // bytes as the image's final chunk.
        self.kitty_chunk = None;
        if let Some(marker) = control.m {
            if !matches!(control.a.as_deref(), None | Some("t") | Some("T")) {
                return;
            }
            if marker == 0 {
                let has_transmission = control.f.is_some() || control.t.is_some();
                if !has_transmission {
                    return;
                }
                let mut control = control;
                control.m = None;
                self.flush_plain(segments);
                self.emit_kitty_graphic(segments, control, payload);
                return;
            }
            if payload.len() > MAX_KITTY_CHUNKED_PAYLOAD {
                self.kitty_chunk_dropping = true;
                tracing::warn!("kitty graphics transmission exceeded payload limit; dropping");
                return;
            }
            self.kitty_chunk = Some(KittyChunkBuffer { control, payload });
            return;
        }

        self.flush_plain(segments);
        self.emit_kitty_graphic(segments, control, payload);
    }

    /// emit a complete Kitty graphics command (after chunking is resolved)
    fn emit_kitty_graphic(
        &mut self,
        segments: &mut Vec<Segment>,
        mut control: KittyControlData,
        payload: Vec<u8>,
    ) {
        // ALWAYS ensure q=2 is set for multiplexer compatibility (no host responses)
        // and drop any chunked-transfer marker: every command emitted here is a
        // complete, self-contained transmission.
        control.q = Some(2);
        control.m = None;
        // give the host explicit image/placement ids so the multiplexer can
        // reference the image later (delete placement and re-place). host-side
        // auto ids are unknowable without responses, which we suppress with q=2.
        if !matches!(control.a.as_deref(), Some("q") | Some("d")) {
            if control.i.is_none() {
                control.i = Some(next_image_id());
            }
            if control.p.is_none()
                && matches!(
                    control.a.as_deref(),
                    Some("T") | Some("t") | Some("p") | Some("A") | Some("a")
                )
            {
                control.p = Some(next_placement_id());
            }
        }

        // reconstruct the full escape sequence from control + payload
        let mut data = Vec::new();
        data.extend_from_slice(b"\x1b_G");
        data.extend_from_slice(control.to_string().as_bytes());
        data.extend_from_slice(b";");
        data.extend_from_slice(&payload);
        data.extend_from_slice(b"\x1b\\");

        // query commands go to plain (so responder sees them, not client)
        if control.a.as_deref() == Some("q") {
            self.plain.extend_from_slice(&data);
            self.flush_plain(segments);
            return;
        }

        let mut command = GraphicCommand {
            format: GraphicFormat::Kitty,
            seq: 0,
            row: 0,
            col: 0,
            alternate_screen: false,
            data,
            kitty_metadata: Some(control),
            reemit: None,
            pixel_size: None,
            file_bytes: None,
        };
        // read a file-backed transmission right here: the app deletes the
        // file (or unlinks the shared memory) the moment it sends the
        // command, so every microsecond between the parse and the read is a
        // chance to lose the pixels for good. kitten icat does exactly this.
        command.read_transmitted_file();
        segments.push(Segment::Graphic(Box::new(command)));
    }

    /// handle sixel sequence (simpler, no chunking)
    fn handle_sixel_sequence(&mut self, segments: &mut Vec<Segment>) {
        let data = std::mem::take(&mut self.seq);
        self.flush_plain(segments);
        let pixel_size = sixel_pixel_dimensions(&data);
        segments.push(Segment::Graphic(Box::new(GraphicCommand {
            format: GraphicFormat::Sixel,
            seq: 0,
            row: 0,
            col: 0,
            alternate_screen: false,
            data,
            kitty_metadata: None,
            reemit: None,
            pixel_size,
            file_bytes: None,
        })));
    }

    /// handle iterm2 sequence (simpler, no chunking)
    fn handle_iterm2_sequence(&mut self, segments: &mut Vec<Segment>) {
        let data = std::mem::take(&mut self.seq);
        self.flush_plain(segments);
        segments.push(Segment::Graphic(Box::new(GraphicCommand {
            format: GraphicFormat::Iterm2,
            seq: 0,
            row: 0,
            col: 0,
            alternate_screen: false,
            data,
            kitty_metadata: None,
            reemit: None,
            pixel_size: None,
            file_bytes: None,
        })));
    }

    fn flush_seq_to_plain(&mut self) {
        if !self.seq.is_empty() {
            let mut seq = std::mem::take(&mut self.seq);
            self.plain.append(&mut seq);
        }
    }

    /// convenience wrapper returning only complete graphics commands, ignoring
    /// plain spans.
    pub fn feed(&mut self, bytes: &[u8], cursor_row: u16, cursor_col: u16) -> Vec<GraphicCommand> {
        self.split(bytes)
            .into_iter()
            .filter_map(|seg| match seg {
                Segment::Graphic(mut cmd) => {
                    cmd.row = i32::from(cursor_row);
                    cmd.col = cursor_col;
                    Some(*cmd)
                }
                Segment::Plain(_) => None,
            })
            .collect()
    }
}

/// extracts the kitty control-data section (between `\x1b_G` and the first
/// `;`/terminator) from a sequence.
fn kitty_control_data(seq: &[u8]) -> Option<&str> {
    let body = if seq.starts_with(b"\x9fG") {
        &seq[2..]
    } else if let Some(start) = seq.windows(3).position(|w| w == b"\x1b_G") {
        &seq[start + 3..]
    } else {
        let start = seq.windows(2).position(|w| w == b"_G")?;
        &seq[start + 2..]
    };
    let ctl_end = body
        .iter()
        .position(|&b| b == b';' || b == 0x07 || b == 0x1b || b == 0x9c)
        .unwrap_or(body.len());
    std::str::from_utf8(&body[..ctl_end]).ok()
}

fn kitty_chunk_continuation(control: &str) -> Option<u8> {
    let mut marker = None;
    for field in control.split(',') {
        let (key, value) = field.split_once('=')?;
        match key {
            "m" if marker.is_none() => {
                let value = value.parse::<u8>().ok()?;
                if value > 1 {
                    return None;
                }
                marker = Some(value);
            }
            "q" => {
                if value.parse::<u8>().ok().is_none_or(|quiet| quiet > 2) {
                    return None;
                }
            }
            _ => return None,
        }
    }
    marker
}

fn rewrite_kitty_control(data: &[u8], control: &KittyControlData) -> Option<Vec<u8>> {
    let delimiter = data.iter().position(|byte| *byte == b';')?;
    let payload_end = if data.ends_with(b"\x1b\\") {
        data.len().checked_sub(2)?
    } else if data.ends_with(&[0x9c]) || data.ends_with(b"\x07") {
        data.len().checked_sub(1)?
    } else {
        data.len()
    };
    if payload_end < delimiter + 1 {
        return None;
    }
    let mut rewritten = Vec::with_capacity(data.len() + 16);
    rewritten.extend_from_slice(b"\x1b_G");
    rewritten.extend_from_slice(control.to_string().as_bytes());
    rewritten.push(b';');
    rewritten.extend_from_slice(&data[delimiter + 1..payload_end]);
    rewritten.extend_from_slice(b"\x1b\\");
    Some(rewritten)
}

/// builds the bytes for re-placing a kitty image whose pixels were already
/// transmitted: delete the old placement (keeping the image stored on the
/// host), then put a fresh placement at the current cursor using the stored
/// image id. the pixel payload is never re-sent. returns None for
/// non-transmit commands and anonymous images.
fn reemit_placement(metadata: &KittyControlData) -> Option<Vec<u8>> {
    // only reemit for transmit actions that have an image id
    let action = metadata.a.as_deref();
    if !matches!(action, Some("T") | Some("p")) {
        return None;
    }

    let id = metadata.i?;

    // delete only the placement; image pixels stay on the host. kitty selects
    // a placement with d=i plus both the image id and placement id.
    let del_ctl = KittyControlData {
        a: Some("d".to_string()),
        d: Some("i".to_string()),
        i: Some(id),
        p: metadata.p,
        q: Some(2),
        ..Default::default()
    };

    // build place command: `a=p,i=<id>,q=2` + placement geometry.
    // this is a *real* placement: it re-displays the already-transmitted pixels
    // at the cursor position (the client positions it with MoveTo). a virtual
    // (U=1) placement would draw nothing here because the placeholder cells
    // are only painted by the application itself.
    let mut place_ctl = KittyControlData {
        a: Some("p".to_string()),
        i: Some(id),
        p: metadata.p,
        q: Some(2), // Quiet mode: no response
        ..Default::default()
    };

    // copy placement keys from original command
    place_ctl.c = metadata.c;
    place_ctl.r = metadata.r;
    place_ctl.x = metadata.x;
    place_ctl.y = metadata.y;
    place_ctl.w = metadata.w;
    place_ctl.h = metadata.h;
    place_ctl.z = metadata.z;
    place_ctl.X = metadata.X;
    place_ctl.Y = metadata.Y;
    place_ctl.C = metadata.C;

    // combine delete and place into a single sequence
    let mut out = kitty_apc(&del_ctl);
    out.extend_from_slice(&kitty_apc(&place_ctl));
    Some(out)
}

/// clamps kitty placement bounds while leaving transmitted image bytes intact.
/// returns modified data only when an explicit or known native size overflows.
pub fn clamp_graphics_to_bounds(
    data: &[u8],
    img_col: u16,
    img_row: u16,
    pane_width: u16,
    pane_height: u16,
) -> Option<Vec<u8>> {
    let available_width = pane_width.saturating_sub(img_col);
    let available_height = pane_height.saturating_sub(img_row);
    if available_width == 0 || available_height == 0 {
        return None;
    }

    let mut result = Vec::with_capacity(data.len());
    let mut copied_through = 0;
    let mut search_from = 0;
    let mut changed = false;

    while let Some((start, marker_len)) = find_kitty_apc(data, search_from) {
        let control_start = start + marker_len;
        let (terminator_start, terminator_len) = find_kitty_terminator(data, control_start)?;
        let control_end = data[control_start..terminator_start]
            .iter()
            .position(|byte| *byte == b';')
            .map(|offset| control_start + offset)
            .unwrap_or(terminator_start);
        let Ok(control) = std::str::from_utf8(&data[control_start..control_end]) else {
            return None;
        };
        let payload_start = (control_end < terminator_start).then_some(control_end + 1);
        let payload = payload_start.map(|start| &data[start..terminator_start]);

        if let Some(rewritten) =
            fit_kitty_control(control, payload, available_width, available_height)
        {
            result.extend_from_slice(&data[copied_through..control_start]);
            result.extend_from_slice(rewritten.as_bytes());
            result.extend_from_slice(&data[control_end..terminator_start + terminator_len]);
            copied_through = terminator_start + terminator_len;
            changed = true;
        }
        search_from = terminator_start + terminator_len;
    }

    if !changed {
        return None;
    }
    result.extend_from_slice(&data[copied_through..]);
    Some(result)
}

fn find_kitty_apc(data: &[u8], from: usize) -> Option<(usize, usize)> {
    let esc = data
        .get(from..)?
        .windows(3)
        .position(|bytes| bytes == b"\x1b_G");
    let c1 = data
        .get(from..)?
        .windows(2)
        .position(|bytes| bytes == b"\x9fG");
    match (esc, c1) {
        (Some(esc), Some(c1)) if esc <= c1 => Some((from + esc, 3)),
        (Some(_), Some(c1)) => Some((from + c1, 2)),
        (Some(esc), None) => Some((from + esc, 3)),
        (None, Some(c1)) => Some((from + c1, 2)),
        (None, None) => None,
    }
}

fn find_kitty_terminator(data: &[u8], from: usize) -> Option<(usize, usize)> {
    let tail = data.get(from..)?;
    let mut best = None;
    for (needle, len) in [
        (b"\x1b\\".as_slice(), 2),
        (b"\x9c".as_slice(), 1),
        (b"\x07".as_slice(), 1),
    ] {
        if let Some(offset) = tail.windows(needle.len()).position(|bytes| bytes == needle)
            && best.is_none_or(|(best_offset, _)| offset < best_offset)
        {
            best = Some((offset, len));
        }
    }
    best.map(|(offset, len)| (from + offset, len))
}

fn fit_kitty_control(
    control: &str,
    payload: Option<&[u8]>,
    available_width: u16,
    available_height: u16,
) -> Option<String> {
    let fields: Vec<&str> = control.split(',').collect();
    let action = fields.iter().find_map(|field| field.strip_prefix("a="))?;
    if !matches!(action, "T" | "p") {
        return None;
    }

    let width = fields.iter().find_map(|field| {
        field
            .strip_prefix("c=")
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
    });
    let height = fields.iter().find_map(|field| {
        field
            .strip_prefix("r=")
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
    });
    let pixels = kitty_pixel_dimensions(&fields, payload);
    let max_width = u32::from(available_width);
    let max_height = u32::from(available_height);

    let (fitted_width, fitted_height, add_width, add_height) = match (width, height, pixels) {
        (Some(width), Some(height), _) => (
            Some(width.min(max_width)),
            Some(height.min(max_height)),
            false,
            false,
        ),
        (Some(width), None, Some((pixel_width, pixel_height))) => {
            let (target_width, target_height) =
                fit_pixel_size(pixel_width, pixel_height, width.min(max_width), max_height);
            (Some(target_width), Some(target_height), false, true)
        }
        (None, Some(height), Some((pixel_width, pixel_height))) => {
            let (target_width, target_height) =
                fit_pixel_size(pixel_width, pixel_height, max_width, height.min(max_height));
            (Some(target_width), Some(target_height), true, false)
        }
        (Some(width), None, None) => (Some(width.min(max_width)), None, false, false),
        (None, Some(height), None) => (None, Some(height.min(max_height)), false, false),
        (None, None, Some((pixel_width, pixel_height))) => {
            let (cell_w, cell_h) = crate::util::outer_term::cell_pixel_size();
            let natural_width = div_ceil_u32(pixel_width, cell_w);
            let natural_height = div_ceil_u32(pixel_height, cell_h);
            if natural_width <= max_width && natural_height <= max_height {
                return None;
            }
            let (target_width, target_height) =
                fit_pixel_size(pixel_width, pixel_height, max_width, max_height);
            (Some(target_width), Some(target_height), true, true)
        }
        (None, None, None) => return None,
    };

    let mut changed = false;
    let mut rewritten = Vec::with_capacity(fields.len() + 2);
    for field in &fields {
        if field.starts_with("c=")
            && let Some(width) = fitted_width
        {
            rewritten.push(format!("c={width}"));
            changed |= field
                .strip_prefix("c=")
                .and_then(|value| value.parse::<u32>().ok())
                != Some(width);
            continue;
        }
        if field.starts_with("r=")
            && let Some(height) = fitted_height
        {
            rewritten.push(format!("r={height}"));
            changed |= field
                .strip_prefix("r=")
                .and_then(|value| value.parse::<u32>().ok())
                != Some(height);
            continue;
        }
        rewritten.push((*field).to_string());
    }
    if add_width
        && !fields.iter().any(|field| field.starts_with("c="))
        && let Some(width) = fitted_width
    {
        rewritten.push(format!("c={width}"));
        changed = true;
    }
    if add_height
        && !fields.iter().any(|field| field.starts_with("r="))
        && let Some(height) = fitted_height
    {
        rewritten.push(format!("r={height}"));
        changed = true;
    }
    changed.then(|| rewritten.join(","))
}

/// reads width and height from the ihdr chunk of a base64 png payload.
/// only the leading bytes are decoded; the image itself is never parsed here.
pub(crate) fn png_dimensions_from_base64(payload: &[u8]) -> Option<(u32, u32)> {
    let prefix: Vec<u8> = payload
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .take(32)
        .collect();
    if prefix.len() != 32 {
        return None;
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(prefix)
        .ok()?;
    if decoded.get(..8)? != b"\x89PNG\r\n\x1a\n" || decoded.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(decoded.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(decoded.get(20..24)?.try_into().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

fn kitty_pixel_dimensions(fields: &[&str], payload: Option<&[u8]>) -> Option<(u32, u32)> {
    let number = |key: &str| {
        fields
            .iter()
            .find_map(|field| field.strip_prefix(key)?.parse::<u32>().ok())
    };
    if let (Some(width), Some(height)) = (number("s="), number("v=")) {
        return (width > 0 && height > 0).then_some((width, height));
    }
    if number("f=") != Some(100) {
        return None;
    }
    png_dimensions_from_base64(payload?)
}

/// returns the offset of the sixel data payload within a complete sequence,
/// past the DCS introducer, its P1;P2;P3 parameters and the `q` final byte.
/// 7-bit (`ESC P ... q`) and 8-bit (`0x90 ... q`) forms are both accepted.
fn sixel_payload_offset(data: &[u8]) -> Option<usize> {
    let mut i = if data.starts_with(b"\x1bP") {
        2
    } else if data.first() == Some(&0x90) {
        1
    } else {
        return None;
    };
    while let Some(&b) = data.get(i)
        && (b.is_ascii_digit() || b == b';')
    {
        i += 1;
    }
    (data.get(i) == Some(&b'q')).then_some(i + 1)
}

/// parses the dimensions of a sixel image from its sequence bytes.
///
/// returns (width, height) in pixels. raster attributes (`"Pan;Pad;Ph;Pv`,
/// chapter 14 of the vt330/vt340 programmer reference manual) are honoured as
/// a lower bound when present. if they are smaller, the size
/// is counted from the data: one pixel per column, six pixels per band. a
/// trailing band without data (e.g. the `-` chafa appends) does not extend the
/// height. repeat (`!n`) and graphics carriage return (`$`) semantics follow
/// the same manual. returns None for sequences whose size cannot be counted.
pub fn sixel_pixel_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let start = sixel_payload_offset(data)?;
    let body = &data[start..];

    let mut x: u32 = 0;
    let mut band: u32 = 0;
    let mut max_x: u32 = 0;
    let mut top_band: u32 = 0;
    let mut repeat: Option<u32> = None;
    let mut params: Vec<u64> = Vec::new();
    let mut number: u64 = 0;
    let mut number_seen = false;
    // command byte awaiting its parameters: '#', '!' or '"'
    let mut pending: Option<u8> = None;
    let mut raster: Option<(u32, u32)> = None;

    let flush = |pending: &mut Option<u8>,
                 params: &mut Vec<u64>,
                 repeat: &mut Option<u32>,
                 raster: &mut Option<(u32, u32)>| {
        match pending.take() {
            // ! Pn : the next data byte repeats Pn times (0 counts as 1)
            Some(b'!') => {
                *repeat = Some(
                    params
                        .first()
                        .copied()
                        .unwrap_or(1)
                        .clamp(1, u32::MAX as u64) as u32,
                );
            }
            // " Pan;Pad;Ph;Pv : raster attributes (only Ph;Pv carry geometry)
            Some(b'"') => {
                if let (Some(&w), Some(&h)) = (params.get(2), params.get(3))
                    && w > 0
                    && h > 0
                {
                    *raster = Some((w.min(u32::MAX as u64) as u32, h.min(u32::MAX as u64) as u32));
                }
            }
            // # Pc[;Pa]... : color register select/define, no geometry
            _ => {}
        }
        params.clear();
    };

    for &b in body {
        match b {
            // ST ends the sequence: everything after it is not sixel data
            0x1b | 0x90 | 0x98 | 0x9c => break,
            b'0'..=b'9' => {
                number = number
                    .saturating_mul(10)
                    .saturating_add(u64::from(b - b'0'));
                number_seen = true;
            }
            b';' => {
                params.push(if number_seen { number } else { 0 });
                number = 0;
                number_seen = false;
            }
            b'#' | b'!' | b'"' => {
                if number_seen {
                    params.push(number);
                }
                number = 0;
                number_seen = false;
                flush(&mut pending, &mut params, &mut repeat, &mut raster);
                pending = Some(b);
            }
            b'$' => {
                if number_seen {
                    params.push(number);
                }
                number = 0;
                number_seen = false;
                flush(&mut pending, &mut params, &mut repeat, &mut raster);
                x = 0;
            }
            b'-' => {
                if number_seen {
                    params.push(number);
                }
                number = 0;
                number_seen = false;
                flush(&mut pending, &mut params, &mut repeat, &mut raster);
                x = 0;
                band = band.saturating_add(1);
            }
            0x3f..=0x7e => {
                if number_seen {
                    params.push(number);
                }
                number = 0;
                number_seen = false;
                flush(&mut pending, &mut params, &mut repeat, &mut raster);
                x = x.saturating_add(repeat.take().unwrap_or(1));
                max_x = max_x.max(x);
                top_band = top_band.max(band);
            }
            // everything else is ignored, per the manual
            _ => {
                if number_seen {
                    params.push(number);
                }
                number = 0;
                number_seen = false;
                flush(&mut pending, &mut params, &mut repeat, &mut raster);
            }
        }
    }

    let counted = (max_x, (top_band + 1) * 6);
    let counted = if max_x == 0 { (0, 0) } else { counted };
    match raster {
        Some((w, h)) => Some((
            w.max(counted.0),
            h.max(if counted.1 == 0 { 0 } else { counted.1 }),
        )),
        None if counted.0 > 0 => Some(counted),
        None => None,
    }
}

fn fit_pixel_size(
    pixel_width: u32,
    pixel_height: u32,
    max_width: u32,
    max_height: u32,
) -> (u32, u32) {
    let (cell_width, cell_height) = crate::util::outer_term::cell_pixel_size();
    if pixel_width == 0 || pixel_height == 0 || max_width == 0 || max_height == 0 {
        return (max_width, max_height);
    }
    let scale = (max_width.saturating_mul(cell_width) as f64 / pixel_width as f64)
        .min(max_height.saturating_mul(cell_height) as f64 / pixel_height as f64)
        .min(1.0);
    let width =
        div_ceil_u32((pixel_width as f64 * scale).ceil() as u32, cell_width).clamp(1, max_width);
    let height =
        div_ceil_u32((pixel_height as f64 * scale).ceil() as u32, cell_height).clamp(1, max_height);
    (width, height)
}

fn div_ceil_u32(numerator: u32, denominator: u32) -> u32 {
    numerator / denominator + u32::from(!numerator.is_multiple_of(denominator))
}

/// store of active graphics placed inside a pane.
#[derive(Debug, Default, Clone)]
pub struct PaneGraphicsStore {
    commands: Vec<GraphicCommand>,
    next_seq: u64,
    kitty_ids: HashMap<(bool, u32), u32>,
}

/// retains every stored command except placements of the given kitty image id,
/// optionally restricted to a single placement id (a=d with the p key).
fn retain_except_placement(
    commands: &mut Vec<GraphicCommand>,
    id: u32,
    placement: Option<u32>,
    alternate_screen: bool,
) {
    commands.retain(|c| {
        if c.format != GraphicFormat::Kitty || c.alternate_screen != alternate_screen {
            return true;
        }
        let Some(ref m) = c.kitty_metadata else {
            return true;
        };
        if m.i != Some(id) {
            return true;
        }
        match placement {
            Some(p) => m.p != Some(p),
            None => false,
        }
    });
}

impl PaneGraphicsStore {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            next_seq: 1,
            kitty_ids: HashMap::new(),
        }
    }

    /// local image id (as written into the vt100 text grid by the app) →
    /// daemon-wide id (as transmitted to the host), per alternate_screen.
    pub fn kitty_id_map(&self) -> &HashMap<(bool, u32), u32> {
        &self.kitty_ids
    }

    pub fn push(&mut self, mut cmd: GraphicCommand) {
        // a file-backed transmission is read right away: the app deletes the
        // file as soon as the transmission is out, so the pixels have to be
        // kept to survive an ED2 repaint.
        cmd.read_transmitted_file();
        // an image re-drawn at the same cell replaces the previous one. without
        // this, every animation frame or preview refresh accumulates a full copy
        // in the store and every later frame re-transmits all of them
        if matches!(cmd.format, GraphicFormat::Sixel | GraphicFormat::Iterm2) {
            self.commands.retain(|existing| {
                !(existing.format == cmd.format
                    && existing.alternate_screen == cmd.alternate_screen
                    && existing.row == cmd.row
                    && existing.col == cmd.col)
            });
        }
        if cmd.format == GraphicFormat::Kitty
            && let Some(metadata) = cmd.kitty_metadata.as_mut()
            && matches!(metadata.a.as_deref(), Some("t") | Some("T"))
            && metadata.i.is_none()
        {
            metadata.i = Some(next_image_id());
            if let Some(data) = rewrite_kitty_control(&cmd.data, metadata) {
                cmd.data = data;
            }
        }
        if cmd.format == GraphicFormat::Kitty
            && let Some(metadata) = cmd.kitty_metadata.as_mut()
            && let Some(source_id) = metadata.i
        {
            let key = (cmd.alternate_screen, source_id);
            let mapped_id = if let Some(mapped_id) = self.kitty_ids.get(&key) {
                *mapped_id
            } else {
                // kitty ids are shared by the outer terminal across all panes.
                // give each transmitted image a daemon-wide id so pane-local
                // ids cannot collide during deletes or placements.
                let is_transmit = matches!(metadata.a.as_deref(), Some("t") | Some("T"));
                let mapped = if is_transmit {
                    next_image_id()
                } else {
                    source_id
                };
                self.kitty_ids.insert(key, mapped);
                mapped
            };
            if mapped_id != source_id {
                metadata.i = Some(mapped_id);
                if let Some(data) = rewrite_kitty_control(&cmd.data, metadata) {
                    cmd.data = data;
                }
            }
        }

        if cmd.format == GraphicFormat::Kitty
            && let Some(ref mut metadata) = cmd.kitty_metadata
        {
            // ensure q=2 is set (quiet mode to suppress protocol responses)
            if metadata.q != Some(2) {
                metadata.q = Some(2);
            }

            // a file-backed transmission was cached above; the app deletes
            // the file the moment it is sent, so rebuild the command as an
            // inline chunked `t=d` transmission now, with the final ids. the
            // first frame then already carries the pixels instead of a path
            // the host can no longer read.
            if cmd.file_bytes.is_some()
                && let Some(bytes) = cmd.file_bytes.clone()
            {
                let mut control = KittyControlData {
                    a: Some("T".to_string()),
                    f: metadata.f,
                    s: metadata.s,
                    v: metadata.v,
                    i: metadata.i,
                    p: metadata.p,
                    c: metadata.c,
                    r: metadata.r,
                    q: Some(2),
                    t: Some("d".to_string()),
                    ..Default::default()
                };
                if control.i.is_none() {
                    control.i = Some(next_image_id());
                    metadata.i = control.i;
                }
                cmd.data = kitty_chunked_apc(&control, &bytes);
                metadata.t = Some("d".to_string());
                metadata.a = Some("T".to_string());
            }

            // every transmit/placement needs a re-place payload: the host
            // keeps the pixels, so a hidden image can come back with
            // delete + `a=p` instead of re-sending the payload. without this
            // only placements that carry an explicit c/r box could be
            // re-placed, and every icat/fastfetch-style image (s/v only)
            // vanished for good once it scrolled out of view.
            if cmd.reemit.is_none()
                && matches!(metadata.a.as_deref(), Some("t") | Some("T") | Some("p"))
            {
                cmd.reemit = reemit_placement(metadata);
            }

            if metadata.a.as_deref() == Some("d") {
                match (&metadata.d, metadata.i) {
                    (Some(del_type), Some(id)) if del_type == "i" || del_type == "I" => {
                        let placement = metadata.p;
                        retain_except_placement(
                            &mut self.commands,
                            id,
                            placement,
                            cmd.alternate_screen,
                        );
                    }
                    // a=d,d=A/a erases all visible placements from the host.
                    // virtual (U=1) placements anchored to placeholder cells
                    // must survive full redraws so the image can be
                    // re-transmitted after an ED2; only real placements are
                    // dropped from the store.
                    (Some(del_type), _) if del_type == "a" || del_type == "A" => {
                        self.commands.retain(|c| {
                            c.format != GraphicFormat::Kitty
                                || c.alternate_screen != cmd.alternate_screen
                                || c.kitty_metadata.as_ref().is_some_and(|m| m.U == Some(1))
                        });
                    }
                    (None, _) => {
                        self.commands.retain(|c| {
                            c.format != GraphicFormat::Kitty
                                || c.alternate_screen != cmd.alternate_screen
                        });
                    }
                    _ => {
                        if let Some(id) = metadata.i {
                            let placement = metadata.p;
                            retain_except_placement(
                                &mut self.commands,
                                id,
                                placement,
                                cmd.alternate_screen,
                            );
                        }
                    }
                }

                // forward delete command to client
                cmd.seq = self.next_seq;
                self.next_seq = self.next_seq.wrapping_add(1).max(1);
                self.append_bounded(cmd);
                return;
            }

            if let (Some(action), Some(id)) = (metadata.a.as_deref(), metadata.i) {
                match action {
                    // a new transmission replaces the stored image data and any
                    // placements associated with the previous contents of id.
                    "t" | "T" => self.commands.retain(|c| {
                        c.format != GraphicFormat::Kitty
                            || c.alternate_screen != cmd.alternate_screen
                            || c.kitty_metadata.as_ref().and_then(|m| m.i) != Some(id)
                    }),
                    // a placement references separately transmitted pixels.
                    // keep that transmission and other placements for the same
                    // image; only replace a duplicate placement id.
                    "p" => {
                        let placement = metadata.p;
                        self.commands.retain(|c| {
                            c.format != GraphicFormat::Kitty
                                || c.alternate_screen != cmd.alternate_screen
                                || c.kitty_metadata.as_ref().is_none_or(|m| {
                                    m.i != Some(id)
                                        || m.a.as_deref() != Some("p")
                                        || m.p != placement
                                })
                        });
                    }
                    _ => {}
                }
            }
        }

        cmd.seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1).max(1);

        // compute reemit bytes if this is a Kitty transmit command with an id.
        // virtual (U=1, Unicode placeholder) placements are anchored to the
        // app's placeholder cells, which flow through the normal text path;
        // they are never re-placed by us.
        if cmd.format == GraphicFormat::Kitty
            && let Some(ref metadata) = cmd.kitty_metadata
        {
            if metadata.U == Some(1) {
                cmd.reemit = None;
            } else {
                cmd.reemit = reemit_placement(metadata);
            }
        }

        self.append_bounded(cmd);
    }

    /// marks a command as recently used, moving it to the back of the
    /// eviction queue. the store evicts the least recently used command
    /// first (zellij does the same with an LRU stamp on every image).
    pub fn touch(&mut self, seq: u64) {
        if let Some(index) = self.commands.iter().position(|command| command.seq == seq) {
            let command = self.commands.remove(index);
            self.commands.push(command);
        }
    }

    /// moves the placements that live in the grid to their new rows after the
    /// grid was re-wrapped (a resize). placements in the scrollback keep their
    /// rows: the scrollback is not re-wrapped.
    pub fn remap_rows(&mut self, row_map: &[(u16, u16)]) {
        for cmd in &mut self.commands {
            if cmd.row < 0 {
                continue;
            }
            let old = cmd.row as u16;
            if let Some((_, new)) = row_map.iter().find(|(from, _)| *from == old) {
                cmd.row = i32::from(*new);
            }
        }
    }

    /// drops the placements that scrolled out of the retained scrollback.
    ///
    /// a placement anchored more than `limit` rows above the live grid is
    /// no longer part of the history the terminal can scroll back to, so the
    /// host must drop the pixels too (zellij frees the image when its last
    /// placement scrolls away). returns the removed commands so the caller
    /// can delete them on the host.
    pub fn drop_scrolled_out(&mut self, limit: i32) -> Vec<GraphicCommand> {
        let mut removed = Vec::new();
        self.commands.retain(|command| {
            if command.row < limit {
                removed.push(command.clone());
                false
            } else {
                true
            }
        });
        if !removed.is_empty() {
            self.kitty_ids.retain(|key, _| {
                !removed
                    .iter()
                    .any(|c| c.kitty_metadata.as_ref().and_then(|m| m.i) == Some(key.1))
            });
        }
        removed
    }

    fn append_bounded(&mut self, cmd: GraphicCommand) {
        let includes_sixel = cmd.format == GraphicFormat::Sixel
            || self
                .commands
                .iter()
                .any(|command| command.format == GraphicFormat::Sixel);
        let max_bytes = if includes_sixel {
            MAX_STORED_SIXEL_BYTES
        } else {
            MAX_STORED_GRAPHIC_BYTES
        };
        if cmd.data.len() > max_bytes {
            tracing::warn!("stored graphics exceeded byte limit; dropping command");
            return;
        }
        while self.commands.len() >= MAX_STORED_GRAPHIC_COMMANDS
            || self
                .commands
                .iter()
                .map(|command| command.data.len())
                .sum::<usize>()
                > max_bytes.saturating_sub(cmd.data.len())
        {
            self.commands.remove(0);
        }
        self.commands.push(cmd);
    }

    pub fn active_commands(&self) -> &[GraphicCommand] {
        &self.commands
    }

    /// clears Kitty images on one virtual screen and queues targeted deletes.
    pub fn clear_screen_images(&mut self, alternate_screen: bool) {
        let image_ids: Vec<u32> = self
            .commands
            .iter()
            .filter(|command| {
                command.format == GraphicFormat::Kitty
                    && command.alternate_screen == alternate_screen
            })
            .filter_map(|command| {
                let metadata = command.kitty_metadata.as_ref()?;
                matches!(metadata.a.as_deref(), Some("T") | Some("p"))
                    .then_some(metadata.i)
                    .flatten()
            })
            .collect();
        self.commands.retain(|command| {
            command.alternate_screen != alternate_screen
                || !matches!(
                    command.format,
                    GraphicFormat::Kitty | GraphicFormat::Sixel | GraphicFormat::Iterm2
                )
        });
        for id in image_ids {
            let control = KittyControlData {
                a: Some("d".to_string()),
                d: Some("i".to_string()),
                i: Some(id),
                q: Some(2),
                ..Default::default()
            };
            let data = kitty_apc(&control);
            self.push(GraphicCommand {
                format: GraphicFormat::Kitty,
                seq: 0,
                row: 0,
                col: 0,
                alternate_screen,
                data,
                kitty_metadata: Some(control),
                reemit: None,
                pixel_size: None,
                file_bytes: None,
            });
        }
    }

    /// returns the kitty placements that live on the alternate screen.
    /// the commands stay in the store: the host keeps the image data, so the
    /// placement can be re-created later instead of losing the image.
    pub fn alternate_screen_placements(&self) -> Vec<GraphicCommand> {
        self.commands
            .iter()
            .filter(|command| {
                command.alternate_screen
                    && command.format == GraphicFormat::Kitty
                    && command.kitty_metadata.as_ref().is_some_and(|metadata| {
                        matches!(metadata.a.as_deref(), Some("T") | Some("p"))
                    })
            })
            .cloned()
            .collect()
    }

    /// removes and returns graphics created in the alternate screen buffer.
    pub fn remove_alternate_screen(&mut self) -> Vec<GraphicCommand> {
        self.kitty_ids.retain(|(alternate, _), _| !alternate);
        let mut removed = Vec::new();
        self.commands.retain(|command| {
            if command.alternate_screen {
                removed.push(command.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    /// shifts graphic row coordinates when scrollback content is added or user scrolls.
    ///
    /// graphics are preserved even when they move out of bounds, because they're referenced by
    /// the host terminal and may be re-placed when scrolling brings them back into view.
    /// only when explicitly deleted by the application should they be removed.
    /// shifts every stored graphic's anchor by `rows` (positive scrolls
    /// content up). anchors may go negative; they come back into view when
    /// the content scrolls back down.
    pub fn scroll_rows(&mut self, rows: i32) {
        for cmd in &mut self.commands {
            // only placements that live in the live grid move when the grid
            // scrolls. A placement that already scrolled into the scrollback
            // keeps its position: new output is appended below it, so shifting
            // it made images drift away from their text (the "logo moves way
            // up while the text stays put" symptom).
            if cmd.row >= 0 {
                cmd.row -= rows;
            }
        }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.kitty_ids.clear();
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    fn command(row: u16, col: u16, cols: Option<u32>, rows: Option<u32>) -> GraphicCommand {
        GraphicCommand {
            format: GraphicFormat::Kitty,
            seq: 0,
            row: i32::from(row),
            col,
            alternate_screen: false,
            data: b"\x1b_Ga=T\x1b\\".to_vec(),
            kitty_metadata: Some(KittyControlData {
                a: Some("T".to_string()),
                c: cols,
                r: rows,
                ..Default::default()
            }),
            reemit: None,
            pixel_size: None,
            file_bytes: None,
        }
    }

    #[test]
    fn placement_contained_checks_the_full_rectangle() {
        let fits = command(4, 2, Some(8), Some(5));
        assert!(fits.placement_contained(20, 20, 0));

        let too_wide = command(4, 15, Some(8), Some(5));
        assert!(!too_wide.placement_contained(20, 20, 0));

        let too_tall = command(18, 0, Some(4), Some(5));
        assert!(!too_tall.placement_contained(20, 20, 0));
    }

    #[test]
    fn placement_contained_respects_scrollback() {
        let command = command(3, 0, Some(4), Some(4));
        assert!(command.placement_contained(20, 20, 3));
        // pushed below the viewport once the user scrolls far enough back
        assert!(!command.placement_contained(20, 20, 30));
    }

    fn clipped_command(row: i32, src_h: u32) -> GraphicCommand {
        GraphicCommand {
            format: GraphicFormat::Kitty,
            seq: 0,
            row,
            col: 0,
            alternate_screen: false,
            data: b"\x1b_Ga=T,f=100,s=1,v=1,i=7;iVBORw0KGgo=\x1b\\".to_vec(),
            kitty_metadata: Some(KittyControlData {
                a: Some("T".to_string()),
                i: Some(7),
                h: Some(src_h),
                s: Some(10),
                v: Some(src_h),
                ..Default::default()
            }),
            reemit: None,
            pixel_size: Some((10, src_h)),
            file_bytes: None,
        }
    }

    #[test]
    fn visible_vertical_fragment_clips_above_and_below() {
        // 2 rows tall at row 3, fully contained
        let cmd = clipped_command(3, 40);
        assert_eq!(cmd.visible_vertical_fragment(20, 0), Some((3, 2)));
        // scrolled 4 rows up: anchor lands at row -1, only 1 row visible
        let mut cmd = clipped_command(3, 40);
        cmd.row -= 4;
        assert_eq!(cmd.row, -1);
        assert_eq!(cmd.visible_vertical_fragment(20, 0), Some((0, 1)));
        // scrolled 8 rows: entirely off-screen
        let mut cmd = clipped_command(3, 40);
        cmd.row -= 8;
        assert_eq!(cmd.visible_vertical_fragment(20, 0), None);
        // scrolled back into view: fragment advances with the scroll offset
        let mut cmd = clipped_command(3, 40);
        cmd.row -= 8;
        // viewport scrolled back 6 rows: top lands at row 1, both rows visible
        assert_eq!(cmd.visible_vertical_fragment(20, 6), Some((1, 2)));
        // and 8 rows: the fully restored image
        assert_eq!(cmd.visible_vertical_fragment(20, 8), Some((3, 2)));
    }

    #[test]
    fn crop_placement_clips_source_rect_to_visible_fragment() {
        // cell size in the sandbox falls back to 10x20 px; px_per_row = 20
        let mut cmd = clipped_command(3, 40);
        cmd.row -= 4; // anchor at -1, one row visible from px 20
        let (data, viewport_col, viewport_row) = cmd
            .crop_placement(20, 20, 0)
            .expect("partially visible image must crop, not hide");
        assert_eq!(viewport_row, 0);
        assert_eq!(viewport_col, 0);
        let text = String::from_utf8_lossy(&data);
        assert!(
            text.contains("a=d") && text.contains("d=i") && text.contains("i=7"),
            "delete precedes re-place: {text}"
        );
        assert!(text.contains("a=p,i=7"), "place follows delete: {text}");
        assert!(
            text.contains("y=20"),
            "source scrolled past the clipped row: {text}"
        );
        assert!(text.contains("h=20"), "source height is one cell: {text}");
        assert!(text.contains("r=1"), "placement occupies one row: {text}");
    }

    #[test]
    fn crop_placement_handles_hostile_source_coordinate_arithmetic() {
        let mut parser = GraphicsParser::new();
        let mut segments =
            parser.split(b"\x1b_Ga=p,i=7,s=2,v=2,c=1,r=2,y=2147483647,h=2147483647;AAAA\x1b\\");
        let Segment::Graphic(command) = &mut segments[0] else {
            panic!("expected kitty placement");
        };
        command.row = -1;

        let (data, _, _) = command
            .crop_placement(1, 1, 0)
            .expect("partially visible placement must be cropped");
        assert!(String::from_utf8_lossy(&data).contains("y=3221225471"));
    }

    #[test]
    fn crop_placement_returns_none_when_fully_contained() {
        let cmd = clipped_command(3, 40);
        assert_eq!(cmd.crop_placement(20, 20, 0), None);
    }
}
