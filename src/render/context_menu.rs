//! tab context menu opened by right-clicking a workspace tab.
//!
//! modelled on the context menus zellij (pane/tab/bar) and iterm render
//! for the same gesture: a compact bordered list anchored at the click
//! point, one row per action, the accelerator key right-aligned, a
//! separator isolating the destructive action, and hover-to-select plus
//! click-to-run for the mouse.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::config::schema::BorderStyle;
use crate::config::theme::Theme;
use crate::render::glyphs::Glyphs;
use crate::render::style::fill;

/// an action the tab context menu can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuAction {
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    label: &'static str,
    accel: char,
    destructive: bool,
    action: ContextMenuAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Item(Item),
    Separator,
}

// keep zellij's ordering (new first, destructive last, separated) and
// iterm's right-aligned accelerator hints.
pub const ROWS: &[Row] = &[
    Row::Item(Item {
        label: "new workspace",
        accel: 'n',
        destructive: false,
        action: ContextMenuAction::NewWorkspace,
    }),
    Row::Item(Item {
        label: "rename workspace",
        accel: 'r',
        destructive: false,
        action: ContextMenuAction::RenameWorkspace,
    }),
    Row::Separator,
    Row::Item(Item {
        label: "close workspace",
        accel: 'c',
        destructive: true,
        action: ContextMenuAction::CloseWorkspace,
    }),
];

const MENU_WIDTH: u16 = 30;

/// state for the tab context menu.
#[derive(Debug, Clone, Default)]
pub struct ContextMenuState {
    /// workspace the menu targets.
    pub target_ws: usize,
    /// selected row index; always an item row.
    pub selected_idx: usize,
    /// click position the menu anchors to.
    pub anchor_col: u16,
    pub anchor_row: u16,
}

pub fn action_for_row(row: usize) -> Option<ContextMenuAction> {
    match ROWS.get(row) {
        Some(Row::Item(item)) => Some(item.action),
        _ => None,
    }
}

pub fn row_for_accel(accel: char) -> Option<usize> {
    ROWS.iter()
        .position(|row| matches!(row, Row::Item(item) if item.accel == accel))
}

/// select the next/previous item row, wrapping and skipping separators.
pub fn step_row(current: usize, delta: isize) -> usize {
    let len = ROWS.len() as isize;
    let mut idx = current.min(ROWS.len() - 1) as isize;
    loop {
        idx = (idx + delta).rem_euclid(len);
        if matches!(ROWS[idx as usize], Row::Item(_)) {
            return idx as usize;
        }
    }
}

/// geometry of the menu: anchored at the click, shifted into view when it
/// would overflow the area. returns the outer rect and inner content rect.
pub fn context_menu_geometry(area: Rect, anchor_col: u16, anchor_row: u16) -> (Rect, Rect) {
    let width = MENU_WIDTH.min(area.width.saturating_sub(1)).max(12);
    let height = (ROWS.len() as u16 + 2).min(area.height.saturating_sub(1));
    let x = anchor_col.min(area.x + area.width.saturating_sub(width));
    let below = anchor_row.saturating_add(1);
    let y = if below + height <= area.y + area.height {
        below
    } else {
        anchor_row.saturating_sub(height)
    };
    (
        Rect::new(x, y, width, height),
        Rect::new(
            x + 1,
            y + 1,
            width.saturating_sub(2),
            height.saturating_sub(2),
        ),
    )
}

/// maps a row inside the menu to its row index; ignores separators.
pub fn context_menu_index_at(inner: Rect, row: u16) -> Option<usize> {
    if row < inner.y || row >= inner.y + inner.height {
        return None;
    }
    let idx = (row - inner.y) as usize;
    matches!(ROWS.get(idx), Some(Row::Item(_))).then_some(idx)
}

pub fn render_context_menu(
    area: Rect,
    buf: &mut Buffer,
    state: &ContextMenuState,
    theme: &Theme,
    ascii: bool,
) {
    let (modal, inner) = context_menu_geometry(area, state.anchor_col, state.anchor_row);
    if modal.width < 12 || modal.height < 4 {
        return;
    }

    let glyphs = Glyphs::styled(ascii, BorderStyle::Rounded);
    let surface = theme.base_color();
    let border = Style::default().fg(theme.overlay_color()).bg(surface);
    fill(
        buf,
        modal,
        Style::default().fg(theme.text_color()).bg(surface),
    );

    for x in modal.x..modal.x + modal.width {
        buf[(x, modal.y)].set_char(glyphs.h).set_style(border);
        buf[(x, modal.y + modal.height - 1)]
            .set_char(glyphs.h)
            .set_style(border);
    }
    for y in modal.y + 1..modal.y + modal.height.saturating_sub(1) {
        buf[(modal.x, y)].set_char(glyphs.v).set_style(border);
        buf[(modal.x + modal.width - 1, y)]
            .set_char(glyphs.v)
            .set_style(border);
    }
    buf[(modal.x, modal.y)]
        .set_char(glyphs.top_left)
        .set_style(border);
    buf[(modal.x + modal.width - 1, modal.y)]
        .set_char(glyphs.top_right)
        .set_style(border);
    buf[(modal.x, modal.y + modal.height - 1)]
        .set_char(glyphs.bottom_left)
        .set_style(border);
    buf[(modal.x + modal.width - 1, modal.y + modal.height - 1)]
        .set_char(glyphs.bottom_right)
        .set_style(border);

    for (idx, row) in ROWS.iter().enumerate() {
        let y = inner.y + idx as u16;
        match *row {
            Row::Separator => {
                let style = Style::default().fg(theme.overlay_color()).bg(surface);
                for x in inner.x..inner.x + inner.width {
                    buf[(x, y)].set_char(glyphs.h).set_style(style);
                }
            }
            Row::Item(item) => {
                let selected = idx == state.selected_idx;
                let row_style = if selected {
                    Style::default()
                        .bg(theme.surface_color())
                        .fg(theme.accent_color())
                        .add_modifier(Modifier::BOLD)
                } else if item.destructive {
                    Style::default().bg(surface).fg(theme.alert_color())
                } else {
                    Style::default().bg(surface).fg(theme.text_color())
                };
                let accel_style = if selected {
                    row_style
                } else {
                    Style::default().bg(surface).fg(theme.muted_color())
                };
                for x in inner.x..inner.x + inner.width {
                    buf[(x, y)].set_char(' ').set_style(row_style);
                }
                buf.set_stringn(
                    inner.x + 2,
                    y,
                    item.label,
                    inner.width.saturating_sub(4) as usize,
                    row_style,
                );
                buf.set_stringn(
                    inner.x + inner.width - 2,
                    y,
                    item.accel.to_string(),
                    1,
                    accel_style,
                );
            }
        }
    }
}
