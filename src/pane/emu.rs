use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use vt100::Parser;

use super::pty::{PtyHandle, spawn_pty_with_graphics};
use super::query::QueryResponder;
use super::search::SearchState;
use super::selection::Selection;
use crate::layout::PaneId;
use crate::pane::graphics::{GraphicsParser, PaneGraphicsStore};

/// rewrites `ESC [ <params> f` to `ESC [ <params> H` inside a plain segment.
/// `<params>` may contain digits, semicolons and question marks (private marker),
/// matching the shape of CUP (`\x1b[H`) and friends. sequences inside DCS/APC/OSC
/// payloads never pass through here, so only real CSI introducers are affected.
fn normalize_csi_f_to_h(bytes: &mut [u8]) {
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] != 0x1b || bytes[i + 1] != b'[' {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b';' || bytes[j] == b'?')
        {
            j += 1;
        }
        if j > i + 2 && bytes.get(j) == Some(&b'f') {
            bytes[j] = b'H';
            i = j + 1;
        } else {
            i += 1;
        }
    }
}

/// removes DECSET mode 12 (cursor blink control) and DECSCUSR (cursor shape)
/// sequences from child process output. Applications like yazi/vim may send
/// \x1b[?12h/l for blink or \x1b[N q for cursor shape (N=0-6), but venti
/// controls all cursor state exclusively. These sequences can affect cursor
/// appearance, so we filter them out before feeding output to the vt100 parser.
fn remove_cursor_blink_sequences(bytes: &mut Vec<u8>, pending: &mut Vec<u8>) {
    let mut combined = std::mem::take(pending);
    combined.extend_from_slice(bytes);
    *bytes = combined;
    if let Some(start) = bytes.iter().rposition(|byte| *byte == 0x1b)
        && bytes.len() - start <= 2
        && (bytes.get(start + 1).is_none() || bytes.get(start + 1) == Some(&b'['))
    {
        pending.extend_from_slice(&bytes[start..]);
        bytes.truncate(start);
    } else if let Some(start) = bytes.windows(2).rposition(|window| window == b"\x1b[")
        && !bytes[start + 2..]
            .iter()
            .any(|byte| (0x40..=0x7e).contains(byte))
    {
        pending.extend_from_slice(&bytes[start..]);
        bytes.truncate(start);
    }
    let mut filtered = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // look for DECSCUSR pattern: ESC [ N SP q  (where N is 0-6, cursor shape)
        // examples: \x1b[0 q (default), \x1b[2 q (block), \x1b[4 q (underline), \x1b[6 q (bar)
        if i + 5 <= bytes.len()
            && bytes[i] == 0x1b
            && bytes[i + 1] == b'['
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3] == b' '
            && bytes[i + 4] == b'q'
        {
            // skip the entire DECSCUSR sequence
            i += 5;
            continue;
        }

        // look for the pattern: ESC [ ? 1 2 (h|l|;...)
        if i + 6 <= bytes.len()
            && bytes[i] == 0x1b
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'?'
            && bytes[i + 3] == b'1'
            && bytes[i + 4] == b'2'
        {
            // skip DECSET 12h/l (cursor blink enable/disable)
            // could be: \x1b[?12h or \x1b[?12l
            if bytes[i + 5] == b'h' || bytes[i + 5] == b'l' {
                i += 6;
                continue;
            }
            // or could be followed by semicolon for additional parameters: \x1b[?12;...
            if bytes[i + 5] == b';' {
                // skip until we find the terminator (h/l)
                let mut j = i + 5;
                while j < bytes.len() && bytes[j] != b'h' && bytes[j] != b'l' {
                    j += 1;
                }
                if j < bytes.len() && (bytes[j] == b'h' || bytes[j] == b'l') {
                    i = j + 1;
                } else {
                    i = j;
                }
                continue;
            }
        }
        filtered.push(bytes[i]);
        i += 1;
    }
    *bytes = filtered;
}

/// mirrors kitty's default cursor advance for placements inside the virtual
/// pane screen. kitty's outer cursor is reset from this screen after each
/// frame, so failing to apply the same movement shifts following child output.
fn advance_kitty_cursor(
    parser: &mut vt100::Parser,
    command: &crate::pane::graphics::GraphicCommand,
) {
    let Some(metadata) = command.kitty_metadata.as_ref() else {
        return;
    };
    if !matches!(metadata.a.as_deref(), Some("T") | Some("p"))
        || metadata.C == Some(1)
        || metadata.U == Some(1)
    {
        return;
    }
    let Some((cols, rows)) = kitty_placement_cells(command, metadata) else {
        return;
    };
    if cols == 0 || rows == 0 {
        return;
    }
    let (screen_rows, screen_cols) = parser.screen().size();
    let cols = cols.min(u32::from(screen_cols));
    let rows = rows.min(u32::from(screen_rows));
    let movement = format!("\x1b[{cols}C\x1b[{rows}B");
    parser.process(movement.as_bytes());
}

/// resolves Kitty's optional placement dimension from the image aspect ratio.
/// fastfetch commonly supplies `c` but omits `r`; the terminal computes the
/// missing dimension from the image and cell pixel aspect ratio. The query
/// responder reports 10x20 pixel cells, so use the same geometry here.
fn kitty_placement_cells(
    command: &crate::pane::graphics::GraphicCommand,
    metadata: &crate::pane::graphics::KittyControlData,
) -> Option<(u32, u32)> {
    let (pw, ph) = crate::util::outer_term::cell_pixel_size();
    let cell_width = u64::from(pw);
    let cell_height = u64::from(ph);

    let pixels = command.kitty_pixel_size();
    let cols = metadata.c.map(u64::from);
    let rows = metadata.r.map(u64::from);
    let (cols, rows) = match (cols, rows) {
        (Some(cols), Some(rows)) => (cols, rows),
        (Some(cols), None) => {
            let (pixel_width, pixel_height) = pixels?;
            let numerator = cols
                .saturating_mul(cell_width)
                .saturating_mul(u64::from(pixel_height));
            let denominator = u64::from(pixel_width).saturating_mul(cell_height);
            (cols, div_ceil(numerator, denominator)?)
        }
        (None, Some(rows)) => {
            let (pixel_width, pixel_height) = pixels?;
            let numerator = rows
                .saturating_mul(cell_height)
                .saturating_mul(u64::from(pixel_width));
            let denominator = u64::from(pixel_height).saturating_mul(cell_width);
            (div_ceil(numerator, denominator)?, rows)
        }
        (None, None) => {
            let (pixel_width, pixel_height) = pixels?;
            (
                div_ceil(u64::from(pixel_width), cell_width)?,
                div_ceil(u64::from(pixel_height), cell_height)?,
            )
        }
    };
    Some((u32::try_from(cols).ok()?, u32::try_from(rows).ok()?))
}

fn div_ceil(numerator: u64, denominator: u64) -> Option<u64> {
    if denominator == 0 {
        return None;
    }
    Some(numerator.div_ceil(denominator))
}

/// feeds a plain segment to the vt100 parser while detecting screen clears.
///
/// returns a bitmask of the screens that were erased at the moment the child
/// sent the erase: bit 0 = main screen, bit 1 = alternate screen. only ED2
/// ("erase all") and ED3 ("erase saved lines") clear the graphics: ED0/ED1
/// are partial erases. RIS (`ESC c`) clears both. the parser is fed up to and
/// including each clear before the active screen is sampled, so a clear that
/// follows an alternate-screen switch is attributed to the right grid.
fn process_plain_tracking_clears(parser: &mut vt100::Parser, bytes: &[u8]) -> u8 {
    let clear_bit = |parser: &vt100::Parser| {
        if parser.screen().alternate_screen() {
            2u8
        } else {
            1u8
        }
    };
    let mut cleared = 0u8;
    let mut start = 0usize;
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] != 0x1b {
            i += 1;
            continue;
        }
        match bytes[i + 1] {
            b'[' => {
                let mut j = i + 2;
                while j < bytes.len() && (0x30..=0x3f).contains(&bytes[j]) {
                    j += 1;
                }
                if j >= bytes.len() {
                    break;
                }
                if bytes[j] == b'J' {
                    let params = &bytes[i + 2..j];
                    let mut value: u32 = 0;
                    let mut valid = !params.is_empty();
                    for &b in params {
                        if b.is_ascii_digit() {
                            value = value.saturating_mul(10).saturating_add(u32::from(b - b'0'));
                        } else {
                            valid = false;
                            break;
                        }
                    }
                    // ED2 (ESC[2J) / ED3 (ESC[3J): explicit full display clear.
                    if valid && (value == 2 || value == 3) {
                        parser.process(&bytes[start..=j]);
                        cleared |= clear_bit(parser);
                        start = j + 1;
                    }
                }
                i = j + 1;
            }
            b'c' => {
                parser.process(&bytes[start..=i + 1]);
                cleared |= 3;
                start = i + 2;
                i += 2;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        parser.process(&bytes[start..]);
    }
    cleared
}

/// how many grid rows a sixel with `pixel_height` occupies, given the cell height in pixels.
fn sixel_rows(pixel_height: u32, cell_height: u32) -> u16 {
    let cell_height = u64::from(cell_height.max(1));
    u16::try_from(pixel_height.div_ceil(cell_height as u32).max(1)).unwrap_or(u16::MAX)
}

fn screen_row_hashes(parser: &vt100::Parser) -> Vec<u64> {
    use std::hash::{Hash, Hasher};

    let (rows, cols) = parser.screen().size();
    (0..rows)
        .map(|row| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for col in 0..cols {
                parser
                    .screen()
                    .cell(row, col)
                    .map(|cell| cell.contents())
                    .unwrap_or(" ")
                    .hash(&mut hasher);
            }
            hasher.finish()
        })
        .collect()
}

fn detected_grid_scroll(previous: &[u64], current: &[u64]) -> i32 {
    if previous.len() != current.len() || previous.is_empty() {
        return 0;
    }
    let len = previous.len() as i32;
    let mut best_shift = 0;
    let mut best_matches = 0usize;
    let mut unchanged_matches = 0usize;
    for shift in -(len - 1)..len {
        let matches = (0..len)
            .filter(|&row| {
                let old_row = row + shift;
                old_row >= 0 && old_row < len && current[row as usize] == previous[old_row as usize]
            })
            .count();
        if shift == 0 {
            unchanged_matches = matches;
        } else if matches > best_matches {
            best_shift = shift;
            best_matches = matches;
        }
    }
    if best_matches > unchanged_matches && best_matches >= previous.len().min(2) {
        best_shift
    } else {
        0
    }
}

/// vt100 surgery for a sixel image, applied at the exact stream position of the
/// image so the app-visible grid stays consistent with what the host terminal paints.
///
/// - places the cursor on the last row occupied by the image (DEC placement,
///   as implemented by foot and wezterm), and when the image crosses the bottom
///   margin, scrolls the page by the number of rows needed to fit the image
///   (DECSDM, sixel scrolling mode). that is also why anything venti renders
///   over the image area is redrawn by a full repaint
fn sixel_erase_and_advance(
    p: &mut vt100::Parser,
    pixel_size: (u32, u32),
    anchor_row: u16,
    anchor_col: u16,
) {
    let (_, cell_height) = crate::util::outer_term::cell_pixel_size();
    let img_rows = sixel_rows(pixel_size.1, cell_height);
    let (total_rows, _) = p.screen().size();
    let total_rows = total_rows.max(1);
    // if the image is taller than the space below the cursor, DECSDM scrolls
    // the page so the image's bottom edge ends on the bottom margin; the
    // image then starts that many rows above the emission anchor
    let desired_top = anchor_row
        .checked_add(img_rows)
        .filter(|end| *end <= total_rows)
        .map(|_| anchor_row)
        .unwrap_or_else(|| total_rows.saturating_sub(img_rows));
    let scroll = anchor_row - desired_top;
    let last_row = desired_top + img_rows - 1;

    // keep the text grid intact: sixel pixels overlay cells, and blanking those
    // cells here made transparent image areas erase TUI content on redraw.
    p.process(format!("\x1b[{};{}H", last_row + 1, anchor_col + 1).as_bytes());

    // DECSDM (sixel scrolling mode): scroll the page so the image's bottom
    // edge fits. an LF at the bottom margin scrolls one row.
    if scroll > 0 {
        let newlines = vec![b'\n'; usize::from(scroll)];
        p.process(&newlines);
    }
}

/// resizes the visible grid when the pane gets narrower and returns the
/// old-row -> new-row mapping so graphics anchored in the grid follow their
/// content.
///
/// the vt100 fork reflows the grid in `set_size`: soft-wrapped rows are
/// re-wrapped instead of truncating the tail of long lines, and the cursor
/// keeps its cell. re-feeding the captured text here would duplicate the
/// reflow and, when the cursor sits on the bottom row (a shell prompt), scroll
/// the whole pane into the scrollback and leave the visible grid blank.
fn reflow_on_resize(p: &mut vt100::Parser, new_cols: u16) -> Vec<(u16, u16)> {
    let (rows, cols) = p.screen().size();
    if new_cols >= cols {
        // growing never truncates
        p.screen_mut().set_size(rows, new_cols);
        return Vec::new();
    }
    let contents = p.screen().contents();
    let width = usize::from(new_cols).max(1);
    // old row -> new row, so graphics anchored in the grid follow their content
    let mut row_map: Vec<(u16, u16)> = Vec::new();
    let mut new_row = 0usize;
    for (index, line) in contents.split('\n').enumerate() {
        if index > 0 {
            new_row += 1;
        }
        row_map.push((index as u16, new_row.min(u16::MAX as usize) as u16));
        new_row += wrap_line(line, width).len().saturating_sub(1);
    }
    p.screen_mut().set_size(rows, new_cols);
    row_map
}

/// splits a line into chunks of at most `width` cells, never inside a
/// character.
fn wrap_line(line: &str, width: usize) -> Vec<&str> {
    if line.is_empty() {
        return vec![line];
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut count = 0usize;
    for (index, ch) in line.char_indices() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if count + ch_width > width && count > 0 {
            chunks.push(&line[start..index]);
            start = index;
            count = 0;
        }
        count += ch_width.max(1);
    }
    chunks.push(&line[start..]);
    chunks
}

fn reserve_input_bytes(counter: &AtomicUsize, len: usize, limit: usize) -> bool {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        let Some(next) = current.checked_add(len) else {
            return false;
        };
        if next > limit {
            return false;
        }
        match counter.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(actual) => current = actual,
        }
    }
}

/// terminal pane owning pty, emulator, title, scrollback, and selection state.
pub struct Pane {
    pub id: PaneId,
    pub pty_master: Box<dyn portable_pty::MasterPty + Send>,
    pub pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub parser: Arc<Mutex<Parser>>,
    pub dirty: Arc<AtomicBool>,
    /// set when a graphics command is pushed to the pane's store and cleared
    /// when the pane is painted; guarantees that a frame follows a
    /// graphic-only output chunk.
    pub gfx_dirty: Arc<AtomicBool>,
    pub title: String,
    pub cwd: Option<PathBuf>,
    pub scroll_offset: usize,
    pub selection: Option<Selection>,
    pub search: Option<SearchState>,
    pub mouse_reporting: Arc<AtomicBool>,
    /// set by the pty reader when an alt-screen application exits (vt100's
    /// alternate_screen flips false). the core loop hard-drops the pane's
    /// kitty placements once, so dead TUI images do not linger on the host.
    pub alt_exited: Arc<AtomicBool>,
    /// set by the pty reader when the child erases the screen (ED2/ED3 or
    /// RIS). stored images for that screen are dropped and the core forces a
    /// full repaint, otherwise a "clear" would resurrect the old sixel on
    /// the next redraw.
    pub screen_cleared: Arc<AtomicBool>,
    /// monotonic rows scrolled by the child, including alternate-screen scrolls.
    pub graphics_scrolls: Arc<std::sync::atomic::AtomicI64>,
    pub child_exited: Arc<AtomicBool>,
    pub exit_status: Arc<Mutex<Option<i32>>>,
    pub child_pid: Option<u32>,
    pub graphics: Arc<Mutex<PaneGraphicsStore>>,
    /// shared capability probe; updated when a client attaches so panes
    /// upgrade from "unknown" to the attached terminal's real capabilities.
    pub graphics_probe: crate::pane::query::GraphicsProbe,
    input_tx: std::sync::mpsc::Sender<Vec<u8>>,
    queued_bytes: Arc<AtomicUsize>,
}

/// parameters for spawning a child shell inside a pty.  bundles the fields
/// so `spawn_with_spawn_config` stays under clippy's argument limit.
#[derive(Debug, Clone)]
pub struct PaneSpawn<'a> {
    pub id: PaneId,
    pub cols: u16,
    pub rows: u16,
    pub scrollback: usize,
    pub shell: &'a str,
    pub cwd: Option<&'a PathBuf>,
    pub session_name: &'a str,
    pub graphics: crate::util::outer_term::GraphicsSpawn,
}

impl Pane {
    /// spawns a new pane with a child shell process.
    pub fn spawn(
        id: PaneId,
        cols: u16,
        rows: u16,
        scrollback: usize,
        shell: &str,
        cwd: Option<&PathBuf>,
        session_name: &str,
    ) -> Result<
        (
            Self,
            std::thread::JoinHandle<()>,
            std::thread::JoinHandle<()>,
        ),
        String,
    > {
        Self::spawn_with_spawn_config(PaneSpawn {
            id,
            cols,
            rows,
            scrollback,
            shell,
            cwd,
            session_name,
            graphics: crate::util::outer_term::GraphicsSpawn::from_env(),
        })
    }

    /// spawns a pane using the attached terminal's graphics capabilities.
    pub fn spawn_with_spawn_config(
        opts: PaneSpawn<'_>,
    ) -> Result<
        (
            Self,
            std::thread::JoinHandle<()>,
            std::thread::JoinHandle<()>,
        ),
        String,
    > {
        let PaneSpawn {
            id,
            cols,
            rows,
            scrollback,
            shell,
            cwd,
            session_name,
            graphics: spawn_ctx,
        } = opts;
        let graphics_probe = crate::pane::query::new_graphics_probe(spawn_ctx.graphics);
        let graphics_probe_reader = graphics_probe.clone();
        let spawn_ctx_owned = spawn_ctx.clone();

        let PtyHandle {
            master,
            mut child,
            mut reader,
            writer,
        } = spawn_pty_with_graphics(cols, rows, shell, cwd, session_name, id.0, spawn_ctx)?;
        let child_pid = child.process_id();

        let parser = Arc::new(Mutex::new(Parser::new(
            rows.max(1),
            cols.max(1),
            scrollback,
        )));
        let dirty = Arc::new(AtomicBool::new(true));
        let child_exited = Arc::new(AtomicBool::new(false));
        let mouse_reporting = Arc::new(AtomicBool::new(false));
        let pty_writer = Arc::new(Mutex::new(writer));
        // dedicated input writer thread: a child that stops reading stdin fills the
        // pty buffer; without this thread the shared mutex would propagate that
        // block to the core loop through write_input().
        let (input_tx, input_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        {
            let writer_pty = Arc::clone(&pty_writer);
            let writer_queued = Arc::clone(&queued_bytes);
            let writer_id = id;
            std::thread::Builder::new()
                .name(format!("venti-pane-input-{}", writer_id.0))
                .spawn(move || {
                    while let Ok(bytes) = input_rx.recv() {
                        if let Ok(mut w) = writer_pty.lock()
                            && let Err(e) = w.write_all(&bytes).and_then(|()| w.flush())
                        {
                            tracing::debug!("pane {} input write failed: {e}", writer_id.0);
                        }
                        writer_queued.fetch_sub(bytes.len(), Ordering::Relaxed);
                    }
                })
                .map_err(|e| format!("failed to spawn pane input writer thread: {e}"))?;
        }

        let pty_writer_clone = Arc::clone(&pty_writer);
        let parser_reader = Arc::clone(&parser);
        let dirty_reader = Arc::clone(&dirty);
        let mouse_reporting_reader = Arc::clone(&mouse_reporting);
        let graphics = Arc::new(Mutex::new(PaneGraphicsStore::new()));
        let gfx_dirty = Arc::new(AtomicBool::new(false));
        let alt_exited = Arc::new(AtomicBool::new(false));
        let screen_cleared = Arc::new(AtomicBool::new(false));
        let graphics_scrolls = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let graphics_reader = Arc::clone(&graphics);
        let alt_exited_reader = Arc::clone(&alt_exited);
        let screen_cleared_reader = Arc::clone(&screen_cleared);
        let graphics_scrolls_reader = Arc::clone(&graphics_scrolls);
        let gfx_dirty_reader = Arc::clone(&gfx_dirty);
        let spawn_ctx_clone = spawn_ctx_owned;
        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 65536];
            let mut responder = QueryResponder::with_probe(
                &spawn_ctx_clone.term,
                spawn_ctx_clone.graphics,
                graphics_probe_reader,
            );
            let mut gfx_parser = GraphicsParser::new();
            let mut was_alt_screen = false;
            let mut cursor_mode_pending = Vec::new();
            // drain until eof so trailing child output is not lost if the waiter thread sees the exit first
            'pty_reader: loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let bytes = &buf[..n];
                        // split the chunk into plain and graphics segments and
                        // process them in order so cursor positions recorded
                        // for graphics are exact.
                        for seg in gfx_parser.split(bytes) {
                            match seg {
                                crate::pane::graphics::Segment::Plain(mut plain) => {
                                    // filter out DECSET mode 12 (cursor blink control) from child
                                    // processes to prevent cursor appearance changes. Venti controls
                                    // cursor state exclusively. Applications like yazi send \x1b[?12h
                                    // or \x1b[?12l which can affect cursor appearance in some terminals.
                                    remove_cursor_blink_sequences(
                                        &mut plain,
                                        &mut cursor_mode_pending,
                                    );

                                    // some vt100 dialects (btop, others) emit CUP as ESC [ N ; M f
                                    // instead of the canonical ESC [ N ; M H; the bundled vt100
                                    // does not treat 'f' as a synonym, so rewrite it in place.
                                    normalize_csi_f_to_h(&mut plain);
                                    let tracks_graphics = graphics_reader
                                        .lock()
                                        .map(|store| !store.active_commands().is_empty())
                                        .unwrap_or(false);
                                    let previous_alt_rows = tracks_graphics
                                        .then(|| parser_reader.lock().ok())
                                        .flatten()
                                        .and_then(|parser| {
                                            parser
                                                .screen()
                                                .alternate_screen()
                                                .then(|| screen_row_hashes(&parser))
                                        });
                                    let screen = parser_reader.lock().ok().map(|p| {
                                        let (row, col) = p.screen().cursor_position();
                                        let (rows, cols) = p.screen().size();
                                        super::query::ScreenState {
                                            cursor_row: row,
                                            cursor_col: col,
                                            rows,
                                            cols,
                                        }
                                    });
                                    if let Some(screen) = screen {
                                        let Ok(mut w) = pty_writer_clone.lock() else {
                                            tracing::error!(
                                                "pane {} query response writer lock poisoned",
                                                id.0
                                            );
                                            break 'pty_reader;
                                        };
                                        if let Err(error) = responder.process_queries(
                                            &plain,
                                            &mut **w,
                                            Some(screen),
                                        ) {
                                            tracing::error!(
                                                "pane {} failed to write terminal query response: {error}",
                                                id.0
                                            );
                                            break 'pty_reader;
                                        }
                                    }
                                    let cleared = if let Ok(mut p) = parser_reader.lock() {
                                        let cleared = process_plain_tracking_clears(&mut p, &plain);
                                        let now_alt = p.screen().alternate_screen();
                                        if let Some(previous) = previous_alt_rows.as_deref()
                                            && now_alt
                                        {
                                            let shift = detected_grid_scroll(
                                                previous,
                                                &screen_row_hashes(&p),
                                            );
                                            if shift != 0 {
                                                graphics_scrolls_reader
                                                    .fetch_add(i64::from(shift), Ordering::AcqRel);
                                            }
                                        }
                                        if was_alt_screen && !now_alt {
                                            alt_exited_reader.store(true, Ordering::Release);
                                        }
                                        was_alt_screen = now_alt;
                                        let reporting = p.screen().mouse_protocol_mode()
                                            != vt100::MouseProtocolMode::None;
                                        mouse_reporting_reader.store(reporting, Ordering::Release);
                                        cleared
                                    } else {
                                        0
                                    };
                                    if cleared != 0 {
                                        if let Ok(mut store) = graphics_reader.lock() {
                                            if cleared & 1 != 0 {
                                                store.clear_screen_images(false);
                                            }
                                            if cleared & 2 != 0 {
                                                store.clear_screen_images(true);
                                            }
                                        }
                                        screen_cleared_reader.store(true, Ordering::Release);
                                    }
                                }
                                crate::pane::graphics::Segment::Graphic(mut cmd) => {
                                    if let Ok(mut p) = parser_reader.lock() {
                                        let pos = p.screen().cursor_position();
                                        let (rows, cols) = p.screen().size();
                                        cmd.row = i32::from(pos.0);
                                        cmd.col = pos.1;
                                        cmd.alternate_screen = p.screen().alternate_screen();
                                        cmd.fit_kitty_to_bounds(
                                            cols.saturating_sub(pos.1).max(1),
                                            rows.saturating_sub(pos.0).max(1),
                                        );
                                        advance_kitty_cursor(&mut p, &cmd);
                                        if cmd.format == crate::pane::graphics::GraphicFormat::Sixel
                                            && let Some(pixel_size) = cmd.pixel_size
                                        {
                                            sixel_erase_and_advance(
                                                &mut p, pixel_size, pos.0, pos.1,
                                            );
                                        }
                                        if let Some(metadata) = cmd.kitty_metadata.as_mut() {
                                            metadata.C = Some(1);
                                            if let Some(data) =
                                                crate::pane::graphics::rewrite_kitty_control(
                                                    &cmd.data, metadata,
                                                )
                                            {
                                                cmd.data = data;
                                            }
                                        }
                                    }
                                    if let Ok(mut store) = graphics_reader.lock() {
                                        store.push(*cmd);
                                    }
                                    gfx_dirty_reader.store(true, Ordering::Release);
                                }
                            }
                        }
                        dirty_reader.store(true, Ordering::Release);
                    }
                    Err(_) => break,
                }
            }
            if !cursor_mode_pending.is_empty()
                && let Ok(mut parser) = parser_reader.lock()
            {
                parser.process(&cursor_mode_pending);
            }
        });

        let exit_status = Arc::new(Mutex::new(None));
        let exit_status_waiter = Arc::clone(&exit_status);
        let child_exited_waiter = Arc::clone(&child_exited);
        let dirty_waiter = Arc::clone(&dirty);
        let waiter_thread = std::thread::spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code() as i32);
            if let Ok(mut slot) = exit_status_waiter.lock() {
                *slot = code;
            }
            child_exited_waiter.store(true, Ordering::Release);
            dirty_waiter.store(true, Ordering::Release);
        });

        let resolved_shell = if shell.is_empty() {
            std::env::var("SHELL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "/bin/sh".to_string())
        } else {
            shell.to_string()
        };
        let shell_program = resolved_shell
            .split_whitespace()
            .next()
            .unwrap_or("/bin/sh");
        let shell_name = std::path::Path::new(shell_program)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(shell_program)
            .to_string();
        let initial_title = child_pid
            .and_then(crate::util::process::foreground_process_name)
            .unwrap_or(shell_name);
        let pane = Self {
            id,
            pty_master: master,
            pty_writer,
            parser,
            dirty,
            gfx_dirty,
            title: initial_title,
            cwd: cwd.cloned(),
            scroll_offset: 0,
            selection: None,
            search: None,
            mouse_reporting,
            alt_exited,
            screen_cleared,
            graphics_scrolls,
            child_exited,
            exit_status,
            child_pid,
            graphics,
            graphics_probe,
            input_tx,
            queued_bytes,
        };

        Ok((pane, reader_thread, waiter_thread))
    }

    /// resizes the pty and underlying vt100 parser grid.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        // cap dimensions to prevent pixel calculation overflow
        let cols = cols.clamp(1, 6553);
        let rows = rows.clamp(1, 3276);
        // report non-zero pixel dimensions so image tools in the pane
        // (icat, yazi, fastfetch) proceed through their winsize gate. prefer
        // the attached terminal's real cell pixel size when it reported one.
        let (cell_w, cell_h) = crate::util::outer_term::cell_pixel_size();
        let pixel_width = (u32::from(cols) * cell_w).min(u32::from(u16::MAX)) as u16;
        let pixel_height = (u32::from(rows) * cell_h).min(u32::from(u16::MAX)) as u16;

        if let Err(error) = self.pty_master.resize(portable_pty::PtySize {
            rows,
            cols,
            pixel_width,
            pixel_height,
        }) {
            tracing::warn!("pane {} pty resize failed: {error}", self.id.0);
            return;
        }
        // keep the emulator in sync with the PTY: never publish a size the
        // child did not accept.
        if let Ok(mut p) = self.parser.lock() {
            let (old_rows, old_cols) = p.screen().size();
            let scrollback_before = crate::pane::search::scrollback_len(&mut p);
            // the height change first: the rows that no longer fit move into
            // the scrollback, exactly like a scroll. Then the width change
            // re-wraps the remaining rows. Both steps move the cells, so the
            // graphics anchored in them have to move as well.
            if rows < old_rows {
                p.screen_mut().set_size(rows, old_cols);
            }
            let row_map = reflow_on_resize(&mut p, cols);
            if rows >= old_rows {
                p.screen_mut().set_size(rows, cols);
            }
            let scrollback_after = crate::pane::search::scrollback_len(&mut p);
            let pushed = scrollback_after.saturating_sub(scrollback_before) as i32;
            if pushed > 0 || !row_map.is_empty() {
                // graphics anchored in the grid must follow their content: the
                // re-wrap and the scroll into the scrollback moved the cells,
                // so the placements move with them. without this a sixel/kitty
                // image stayed on the old row and was drawn over the pane
                // border, outside the pane, or twice.
                if let Ok(mut store) = self.graphics.lock() {
                    if pushed > 0 {
                        store.scroll_rows(pushed);
                    }
                    if !row_map.is_empty() {
                        store.remap_rows(&row_map);
                    }
                }
            }
        }
        self.dirty.store(true, Ordering::Release);
    }

    /// asks the child process tree to terminate (sighup now, sigkill shortly after).
    pub fn kill_tree(&self) {
        if !self.child_exited.load(Ordering::Acquire)
            && let Some(pid) = self.child_pid
        {
            crate::util::process::kill_process_tree(pid);
        }
    }

    /// queues raw bytes for the child's stdin.
    ///
    /// returns immediately; the actual write happens on the pane's input writer
    /// thread so a full pty buffer can never stall the caller. if more than 16
    /// mib of input is already queued for this pane the bytes are dropped and
    /// logged, which bounds memory under a hostile or stuck child.
    pub fn write_input(&self, bytes: &[u8]) {
        const MAX_QUEUED_BYTES: usize = 16 * 1024 * 1024;
        if !reserve_input_bytes(&self.queued_bytes, bytes.len(), MAX_QUEUED_BYTES) {
            tracing::warn!(
                "pane {} input queue full; dropping {} bytes",
                self.id.0,
                bytes.len()
            );
            return;
        }
        if self.input_tx.send(bytes.to_vec()).is_err() {
            self.queued_bytes.fetch_sub(bytes.len(), Ordering::Relaxed);
            tracing::debug!(
                "pane {} input writer thread gone; dropping input",
                self.id.0
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        detected_grid_scroll, normalize_csi_f_to_h, reserve_input_bytes, sixel_erase_and_advance,
        sixel_rows,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn sixel_rows_rounds_up_and_saturates() {
        assert_eq!(sixel_rows(6, 20), 1);
        assert_eq!(sixel_rows(20, 20), 1);
        assert_eq!(sixel_rows(21, 20), 2);
        assert_eq!(sixel_rows(60, 20), 3);
        assert_eq!(sixel_rows(1, 0), 1);
    }

    #[test]
    fn reflow_keeps_long_lines_when_the_pane_shrinks() {
        let mut parser = vt100::Parser::new(10, 40, 1000);
        parser.process(b"LONGLINE-XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX\r\n");
        assert!(parser.screen().contents().contains("LONGLINE"));
        super::reflow_on_resize(&mut parser, 20);
        let contents = parser.screen().contents();
        assert!(
            contents.contains("LONGLINE"),
            "the text must survive a narrower pane: {contents:?}"
        );
        assert!(
            contents.contains('X'),
            "the tail of the line must be reflowed, not dropped: {contents:?}"
        );
    }

    #[test]
    fn sixel_advance_preserves_text_under_the_image() {
        let mut parser = vt100::Parser::new(10, 40, 0);
        parser.process(b"\x1b[2;1Hjunk");
        sixel_erase_and_advance(&mut parser, (64, 20), 1, 0);
        let contents = parser.screen().contents();
        assert!(
            contents.contains("junk"),
            "text under the image must remain in the grid: {contents:?}"
        );
    }

    #[test]
    fn sixel_erase_and_advance_places_cursor_on_last_image_row() {
        let mut parser = vt100::Parser::new(10, 40, 0);
        parser.process(b"\x1b[2;1H");
        sixel_erase_and_advance(&mut parser, (64, 60), 1, 0);
        // 60px at 20px cells = 3 rows; cursor lands on the last occupied row
        assert_eq!(parser.screen().cursor_position(), (3, 0));
    }

    #[test]
    fn sixel_crossing_bottom_margin_scrolls_page() {
        let mut parser = vt100::Parser::new(5, 40, 100);
        parser.process(b"[5;1Hjunk");
        sixel_erase_and_advance(&mut parser, (20, 60), 4, 0);
        // image is 3 rows starting at row 4 of 5: two rows overflow, so the
        // page scrolls up by two and the cursor stays on the bottom row
        assert_eq!(parser.screen().cursor_position().0, 4);
        let contents = parser.screen().contents();
        assert!(contents.contains("junk"));
    }

    #[test]
    fn sixel_taller_than_screen_anchors_to_top() {
        let mut parser = vt100::Parser::new(6, 40, 100);
        parser.process(b"\x1b[6;1H");
        sixel_erase_and_advance(&mut parser, (20, 200), 5, 0);
        // 200px at 20px cells = 10 rows > 6-row screen: image anchors to row 0,
        // page scrolls 5 rows, cursor stays on the bottom margin
        assert_eq!(parser.screen().cursor_position().0, 5);
    }

    #[test]
    fn cursor_style_sequences_split_across_pty_reads_are_filtered() {
        let mut pending = Vec::new();
        let mut first = b"before\x1b[5".to_vec();
        super::remove_cursor_blink_sequences(&mut first, &mut pending);
        assert_eq!(first, b"before");

        let mut second = b" qafter".to_vec();
        super::remove_cursor_blink_sequences(&mut second, &mut pending);
        assert_eq!(second, b"after");
        assert!(pending.is_empty());
    }

    /// DEC private-mode reset covered by the test below: restores vt100
    /// defaults for cursor visibility, mouse reporting, bracketed paste, and
    /// related modes.
    const MODE_RESET_SEQUENCE: &[u8] = b"\x1b[?1l\x1b[?6l\x1b[?7h\x1b[?25h\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?2004l\x1b[r\x1b[m\x1b(B";

    #[test]
    fn mode_reset_sequence_restores_cursor_mouse_and_paste() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"\x1b[?25l\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?1h");
        assert!(parser.screen().hide_cursor());
        parser.process(MODE_RESET_SEQUENCE);
        assert!(!parser.screen().hide_cursor());
        assert_eq!(
            parser.screen().mouse_protocol_mode(),
            vt100::MouseProtocolMode::None
        );
        assert!(!parser.screen().bracketed_paste());
    }

    #[test]
    fn detects_grid_scrolls_in_both_directions() {
        assert_eq!(detected_grid_scroll(&[1, 2, 3, 4], &[2, 3, 4, 5]), 1);
        assert_eq!(detected_grid_scroll(&[1, 2, 3, 4], &[9, 1, 2, 3]), -1);
        assert_eq!(detected_grid_scroll(&[1, 2, 3, 4], &[1, 2, 3, 5]), 0);
    }

    #[test]
    fn incomplete_csi_at_chunk_end_does_not_panic() {
        let mut bytes = b"\x1b[1;".to_vec();
        normalize_csi_f_to_h(&mut bytes);
        assert_eq!(bytes, b"\x1b[1;");
    }

    #[test]
    fn graphics_commands_follow_the_childs_active_screen_buffer() {
        let mut graphics = crate::pane::graphics::GraphicsParser::new();
        let mut parser = vt100::Parser::new(24, 80, 100);
        let mut captured = Vec::new();
        for bytes in [
            b"\x1b[?1049h\x1b_Ga=T,f=24,i=1;AAAA\x1b\\".as_slice(),
            b"\x1b[?1049l\x1b_Ga=T,f=24,i=2;BBBB\x1b\\",
        ] {
            for segment in graphics.split(bytes) {
                match segment {
                    crate::pane::graphics::Segment::Plain(bytes) => parser.process(&bytes),
                    crate::pane::graphics::Segment::Graphic(mut command) => {
                        command.alternate_screen = parser.screen().alternate_screen();
                        captured.push(command.alternate_screen);
                    }
                }
            }
        }
        assert_eq!(captured, [true, false]);
    }

    #[test]
    fn input_queue_reservation_rejects_limit_and_overflow() {
        let counter = AtomicUsize::new(8);
        assert!(!reserve_input_bytes(&counter, 3, 10));
        assert_eq!(counter.load(Ordering::Relaxed), 8);

        let counter = AtomicUsize::new(usize::MAX - 1);
        assert!(!reserve_input_bytes(&counter, 2, usize::MAX));
        assert_eq!(counter.load(Ordering::Relaxed), usize::MAX - 1);
    }
}
