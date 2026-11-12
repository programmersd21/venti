/// point in grid coordinates (line, column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GridPoint {
    pub line: usize,
    pub col: usize,
}

/// selection mode in pane contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Character,
    Line,
    Block,
}

/// active text selection range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub start: GridPoint,
    pub end: GridPoint,
    pub mode: SelectionMode,
}

impl Selection {
    pub fn new(start: GridPoint, end: GridPoint, mode: SelectionMode) -> Self {
        Self { start, end, mode }
    }

    /// returns the selection endpoints ordered start <= end.
    pub fn normalized(&self) -> (GridPoint, GridPoint) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// checks if a cell at (line, col) is within selection boundaries.
    pub fn contains(&self, line: usize, col: usize) -> bool {
        let (min_pt, max_pt) = self.normalized();

        match self.mode {
            SelectionMode::Character => {
                if line < min_pt.line || line > max_pt.line {
                    false
                } else if line == min_pt.line && line == max_pt.line {
                    col >= min_pt.col && col <= max_pt.col
                } else if line == min_pt.line {
                    col >= min_pt.col
                } else if line == max_pt.line {
                    col <= max_pt.col
                } else {
                    true
                }
            }
            SelectionMode::Line => line >= min_pt.line && line <= max_pt.line,
            SelectionMode::Block => {
                let min_col = self.start.col.min(self.end.col);
                let max_col = self.start.col.max(self.end.col);
                line >= min_pt.line && line <= max_pt.line && col >= min_col && col <= max_col
            }
        }
    }
}

/// returns the substring of `line` whose cells fall inside [start, end]
/// (both in display cells, inclusive).
fn slice_cells(line: &str, start: usize, end: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    let mut out = String::new();
    let mut col = 0usize;
    for ch in line.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
        let cell_start = col;
        let cell_end = col + width;
        col = cell_end;
        if cell_end <= start || cell_start > end {
            continue;
        }
        out.push(ch);
    }
    out
}

/// extracts the plain text of a selection over the pane's full buffer
/// (scrollback included).
///
/// `sel` line indices are viewport rows as produced by mouse and keyboard
/// selection. `scroll_offset` is the pane's current scrollback offset; it is
/// used to map viewport rows onto logical buffer lines.
pub fn selection_text(parser: &mut vt100::Parser, sel: &Selection, scroll_offset: usize) -> String {
    let base = crate::pane::search::scrollback_len(parser).saturating_sub(scroll_offset);
    let full = crate::pane::search::full_buffer_text(parser);
    let (min_pt, max_pt) = sel.normalized();
    let min_line = base + min_pt.line;
    let max_line = base + max_pt.line;
    let lines: Vec<&str> = full.lines().collect();
    let mut out = String::new();
    for (idx, raw) in lines.iter().enumerate() {
        if idx < min_line || idx > max_line {
            continue;
        }
        let line = raw.trim_end();
        let text = match sel.mode {
            SelectionMode::Line => line.to_string(),
            SelectionMode::Block => {
                let start = min_pt.col.min(max_pt.col);
                let end = min_pt.col.max(max_pt.col);
                slice_cells(line, start, end).trim_end().to_string()
            }
            SelectionMode::Character => {
                let start = if idx == min_line { min_pt.col } else { 0 };
                let end = if idx == max_line {
                    max_pt.col
                } else {
                    usize::MAX
                };
                slice_cells(line, start, end).trim_end().to_string()
            }
        };
        out.push_str(&text);
        if idx != max_line {
            out.push('\n');
        }
    }
    out
}
