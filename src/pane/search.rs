use unicode_width::UnicodeWidthChar;

use super::selection::GridPoint;

/// returns the number of lines in the pane's scrollback, without disturbing
/// the current scrollback offset.
pub fn scrollback_len(parser: &mut vt100::Parser) -> usize {
    let offset = parser.screen().scrollback();
    parser.screen_mut().set_scrollback(usize::MAX);
    let len = parser.screen().scrollback();
    parser.screen_mut().set_scrollback(offset);
    len
}

/// extracts the full text of the pane, including scrollback history above the
/// live viewport. each returned line is joined by `\n`; line indices in the
/// returned string match the logical scrollback indices used by `searchstate`.
pub fn full_buffer_text(parser: &mut vt100::Parser) -> String {
    let original_offset = parser.screen().scrollback();
    let (size_rows, size_cols) = {
        let size = parser.screen().size();
        (usize::from(size.0), usize::from(size.1))
    };
    let rows = size_rows.max(1);
    let len = scrollback_len(parser);

    let mut lines: Vec<String> = Vec::with_capacity(len + rows);
    let mut emitted = 0usize;
    for s in (0..=len).rev() {
        parser.screen_mut().set_scrollback(s);
        for (r, line) in parser.screen().rows(0, size_cols as u16).enumerate() {
            let idx = len - s + r;
            if idx >= emitted {
                lines.push(line);
                emitted = idx + 1;
            }
        }
    }
    parser.screen_mut().set_scrollback(original_offset);
    lines.join("\n")
}

/// active search highlight query state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchState {
    pub pattern: String,
    pub matches: Vec<(GridPoint, usize)>, // (start, length in cells)
    pub current_match: Option<usize>,
    pub reverse: bool,
}

impl SearchState {
    pub fn new(pattern: String, reverse: bool) -> Self {
        Self {
            pattern,
            matches: Vec::new(),
            current_match: None,
            reverse,
        }
    }

    /// pushes a character onto the query and recomputes matches.
    pub fn push(&mut self, c: char, screen_text: &str) {
        self.pattern.push(c);
        self.recompute(screen_text);
    }

    /// removes the last character of the query and recomputes matches.
    pub fn backspace(&mut self, screen_text: &str) {
        self.pattern.pop();
        self.recompute(screen_text);
    }

    /// recomputes matches from the visible screen contents (case-insensitive).
    pub fn recompute(&mut self, screen_text: &str) {
        self.matches.clear();
        self.current_match = None;
        let pattern = self.pattern.trim();
        if pattern.is_empty() {
            return;
        }
        let needle = pattern.to_lowercase().chars().collect::<Vec<_>>();
        for (line_idx, line) in screen_text.lines().enumerate() {
            let haystack: Vec<char> = line.chars().flat_map(|c| c.to_lowercase()).collect();
            let mut from = 0usize;
            while let Some(found) = find_subslice(&haystack[from..], &needle) {
                let start = from + found;
                let col = haystack[..start]
                    .iter()
                    .map(|c| UnicodeWidthChar::width(*c).unwrap_or(1))
                    .sum();
                let len: usize = haystack[start..start + needle.len()]
                    .iter()
                    .map(|c| UnicodeWidthChar::width(*c).unwrap_or(1))
                    .sum();
                self.matches.push((
                    GridPoint {
                        line: line_idx,
                        col,
                    },
                    len.max(1),
                ));
                from = start + needle.len();
            }
        }
        if !self.matches.is_empty() {
            self.current_match = Some(0);
        }
    }

    /// advances to the next match, wrapping around.
    pub fn next_match(&mut self) {
        if self.matches.is_empty() {
            self.current_match = None;
            return;
        }
        let next = self
            .current_match
            .map(|i| (i + 1) % self.matches.len())
            .unwrap_or(0);
        self.current_match = Some(next);
    }

    /// goes to the previous match, wrapping around.
    pub fn prev_match(&mut self) {
        if self.matches.is_empty() {
            self.current_match = None;
            return;
        }
        let prev = self
            .current_match
            .map(|i| (i + self.matches.len() - 1) % self.matches.len())
            .unwrap_or(0);
        self.current_match = Some(prev);
    }

    /// checks if a cell at (line, col) falls within any search match.
    pub fn is_match(&self, line: usize, col: usize) -> bool {
        self.matches
            .iter()
            .any(|(pt, len)| pt.line == line && col >= pt.col && col < pt.col + *len)
    }

    /// checks if a cell is part of the current search match.
    pub fn is_current_match(&self, line: usize, col: usize) -> bool {
        let Some(i) = self.current_match else {
            return false;
        };
        let Some((pt, len)) = self.matches.get(i) else {
            return false;
        };
        pt.line == line && col >= pt.col && col < pt.col + *len
    }
}

fn find_subslice(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
