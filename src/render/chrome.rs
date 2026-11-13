use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::schema::{BorderStyle, SidebarDensity, WorkspaceBarPosition};
use crate::config::theme::Theme;
use crate::input::modes::InputMode;
use crate::layout::WorkspaceId;
use crate::render::glyphs::Glyphs;
use crate::render::style::bar_badge;

/// reserved width for the right-aligned session name.
pub const SESSION_RESERVE: u16 = 25;
/// reserved width for the '+' new-workspace button column.
pub const ADD_BTN_RESERVE: u16 = 3;
/// columns set aside for a scroll indicator (glyph + 1 click-only col).
const ARROW_ZONE: u16 = 2;

/// the pill's text: index, name, and a trailing busy dot when any pane in
/// the workspace is running a foreground process.
fn pill_text(ws: usize, name: &str, busy: bool) -> String {
    if busy {
        format!(" {ws} {name} ● ")
    } else {
        format!(" {ws} {name} ")
    }
}

/// total columns one tab consumes, including its trailing separator.
fn tab_total_width(ws: usize, name: &str, busy: bool) -> u16 {
    let pill_w = pill_text(ws, name, busy).chars().count() as u16;
    pill_w + 2 + ARROW_ZONE // close button + separator dot
}

/// returns true when `workspaces[start..]` all fit in the tab area.
fn tabs_fit_from(area: Rect, workspaces: &[(usize, String, bool)], start: usize) -> bool {
    let left_reserve: u16 = if start > 0 { ARROW_ZONE } else { 0 };
    let tab_area_width = area
        .width
        .saturating_sub(SESSION_RESERVE)
        .saturating_sub(ADD_BTN_RESERVE)
        .saturating_sub(left_reserve);
    if tab_area_width <= ARROW_ZONE {
        return start >= workspaces.len();
    }
    let right_anchor = area.x + left_reserve + tab_area_width - ARROW_ZONE;
    let mut cursor_x = area.x + 1 + left_reserve;
    for (ws, name, busy) in workspaces.iter().skip(start) {
        let total = tab_total_width(*ws, name, *busy);
        if cursor_x.saturating_add(total) > right_anchor {
            return false;
        }
        cursor_x += total;
    }
    true
}

/// largest scroll offset that still keeps every remaining tab visible.
pub fn max_tab_scroll_offset(area: Rect, workspaces: &[(usize, String, bool)]) -> usize {
    let len = workspaces.len();
    for start in 0..len {
        if tabs_fit_from(area, workspaces, start) {
            return start;
        }
    }
    len.saturating_sub(1)
}

/// a visible workspace tab in the bar: pill start/width and close button start/width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabSlot {
    pub ws: usize,
    pub pill_x: u16,
    pub pill_w: u16,
    pub close_x: u16,
    pub close_w: u16,
}

/// shared geometry for the workspace bar, used by both the renderer and mouse
/// hit-testing so glyphs and click zones can never disagree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TabBarLayout {
    /// x of the '<' scroll-left indicator, present only when scrolled right.
    pub left_arrow_x: Option<u16>,
    /// x of the '>' scroll-right indicator, present only when more tabs overflow.
    /// anchored to the right edge of the tab area so it never drifts.
    pub right_arrow_x: Option<u16>,
    /// x of the '+' new-workspace button.
    pub add_x: Option<u16>,
    /// visible tabs in render order.
    pub tabs: Vec<TabSlot>,
}

/// computes the shared tab bar geometry.
///
/// arrows are pinned to fixed columns (< at the left edge, > at the right edge
/// of the tab area) so rapid workspace closes can never shift them.
pub fn layout_workspace_bar(
    area: Rect,
    workspaces: &[(usize, String, bool)],
    scroll_offset: usize,
) -> TabBarLayout {
    let mut layout = TabBarLayout::default();
    if area.width == 0 {
        return layout;
    }
    let scroll_offset = scroll_offset.min(max_tab_scroll_offset(area, workspaces));

    // reserve 2 cols on the left for '<' when scrolled
    let left_reserve: u16 = if scroll_offset > 0 { ARROW_ZONE } else { 0 };
    let tab_area_width = area
        .width
        .saturating_sub(SESSION_RESERVE)
        .saturating_sub(ADD_BTN_RESERVE)
        .saturating_sub(left_reserve);
    if tab_area_width <= ARROW_ZONE {
        return layout;
    }

    let tab_end = area.x + left_reserve + tab_area_width;
    let right_anchor = tab_end - ARROW_ZONE;

    if scroll_offset > 0 {
        layout.left_arrow_x = Some(area.x + 1);
    }

    let mut cursor_x = area.x + 1 + left_reserve;
    for (ws, name, busy) in workspaces.iter().skip(scroll_offset) {
        let pill_w = pill_text(*ws, name, *busy).chars().count() as u16;
        let total = tab_total_width(*ws, name, *busy);
        if cursor_x.saturating_add(total) > right_anchor {
            layout.right_arrow_x = Some(right_anchor);
            break;
        }
        layout.tabs.push(TabSlot {
            ws: *ws,
            pill_x: cursor_x,
            pill_w,
            close_x: cursor_x + pill_w,
            close_w: 2,
        });
        cursor_x += total;
    }

    layout.add_x = (cursor_x < right_anchor).then_some(cursor_x);
    layout
}

/// renders workspace bar pill tabs at top or bottom.
#[allow(clippy::too_many_arguments)]
pub fn render_workspace_bar(
    area: Rect,
    buf: &mut Buffer,
    workspaces: &[(WorkspaceId, String, bool)],
    active: WorkspaceId,
    session_name: &str,
    theme: &Theme,
    pos: WorkspaceBarPosition,
    scroll_offset: usize,
) {
    if pos == WorkspaceBarPosition::Off || area.width == 0 || area.height == 0 {
        return;
    }

    let bar_style = Style::default().bg(Color::Reset).fg(theme.muted_color());

    for x in area.x..area.x + area.width {
        buf[(x, area.y)].set_char(' ').set_style(bar_style);
    }

    let ws_pairs: Vec<(usize, String, bool)> = workspaces
        .iter()
        .map(|(id, name, busy)| (id.0, name.clone(), *busy))
        .collect();
    let layout = layout_workspace_bar(area, &ws_pairs, scroll_offset);

    let arrow_style = Style::default()
        .bg(Color::Reset)
        .fg(theme.accent_color())
        .add_modifier(Modifier::BOLD);
    let separator_style = crate::render::style::separator_style(theme);
    let add_style = Style::default().bg(Color::Reset).fg(theme.muted_color());

    // '<' scroll-left indicator: always pinned to the bar's left edge
    if let Some(x) = layout.left_arrow_x {
        buf[(x, area.y)].set_char('<').set_style(arrow_style);
    }

    for tab in &layout.tabs {
        let is_active = tab.ws == active.0;
        // active tab: filled surface pill; idle tab stays transparent and quiet
        let row_bg = if is_active {
            theme.surface_color()
        } else {
            Color::Reset
        };
        let num_style = Style::default()
            .fg(if is_active {
                theme.accent_color()
            } else {
                theme.subtext_color()
            })
            .bg(row_bg)
            .add_modifier(Modifier::BOLD);
        let name_style = Style::default()
            .fg(if is_active {
                theme.text_color()
            } else {
                theme.muted_color()
            })
            .bg(row_bg)
            .add_modifier(if is_active {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        let close_style = Style::default()
            .fg(if is_active {
                theme.alert_color()
            } else {
                theme.overlay_color()
            })
            .bg(row_bg);

        let (name, busy) = ws_pairs
            .iter()
            .find(|entry| entry.0 == tab.ws)
            .map(|(_, name, busy)| (name.as_str(), *busy))
            .unwrap_or(("", false));
        if is_active {
            for x in tab.pill_x..tab.close_x + tab.close_w {
                buf[(x, area.y)].set_char(' ').set_style(
                    Style::default()
                        .bg(theme.surface_color())
                        .fg(theme.text_color()),
                );
            }
        }
        let num_text = format!(" {} ", tab.ws);
        for (i, ch) in num_text.chars().enumerate() {
            buf[(tab.pill_x + i as u16, area.y)]
                .set_char(ch)
                .set_style(num_style);
        }
        let name_x = tab.pill_x + num_text.chars().count() as u16;
        buf.set_stringn(
            name_x,
            area.y,
            name,
            tab.pill_w
                .saturating_sub(num_text.chars().count() as u16 + 1) as usize,
            name_style,
        );
        let mut cursor = name_x + name.chars().count() as u16;
        if busy {
            buf[(cursor, area.y)]
                .set_char(' ')
                .set_style(Style::default().bg(row_bg));
            buf[(cursor + 1, area.y)]
                .set_char('●')
                .set_style(Style::default().fg(theme.warn_color()).bg(row_bg));
            cursor += 2;
        }
        for x in cursor..tab.close_x {
            buf[(x, area.y)]
                .set_char(' ')
                .set_style(Style::default().bg(row_bg));
        }
        buf[(tab.close_x, area.y)]
            .set_char('×')
            .set_style(close_style);
        buf[(tab.close_x + 1, area.y)]
            .set_char(' ')
            .set_style(close_style);

        buf[(tab.close_x + tab.close_w, area.y)]
            .set_char('│')
            .set_style(separator_style);
    }

    // '>' scroll-right indicator: always pinned to the tab area's right edge
    if let Some(x) = layout.right_arrow_x {
        buf[(x, area.y)].set_char('>').set_style(arrow_style);
    }

    // '+' button to create new workspace
    if let Some(x) = layout.add_x {
        buf[(x, area.y)].set_char('+').set_style(add_style);
    }

    // right-aligned session indicator: live dot + name, both muted.
    // the leading space keeps the dot visually separated from the name.
    let session_str = format!(" {} ", session_name);
    let session_width = session_str.chars().count() as u16;
    let right_start = area
        .x
        .saturating_add(area.width)
        .saturating_sub(session_width)
        .saturating_sub(1);

    // only render session indicator if it fits
    let cursor_x = layout
        .add_x
        .map(|x| x + 1)
        .unwrap_or(area.x + layout.left_arrow_x.map_or(0, |x| x + 2));
    if right_start > area.x && right_start >= cursor_x {
        // liveness dot: one quiet success-colored pixel before the name
        if right_start >= 2 {
            buf[(right_start - 1, area.y)]
                .set_char('●')
                .set_style(Style::default().fg(theme.success_color()).bg(Color::Reset));
        }
        for (i, ch) in session_str.chars().enumerate() {
            let x = right_start.saturating_add(i as u16);
            if x < area.x + area.width {
                buf[(x, area.y)]
                    .set_char(ch)
                    .set_style(Style::default().fg(theme.muted_color()).bg(Color::Reset));
            }
        }
    }
}

/// rows each tab occupies: collapsed and compact are single-row; comfortable
/// gives a second row for the focused-pane hint.
fn sidebar_rows_per_tab(area: Rect, density: SidebarDensity, collapsed: bool) -> u16 {
    if collapsed || density == SidebarDensity::Compact || area.width < 8 {
        1
    } else {
        2
    }
}

/// how many slots fit in `rows_left`, reserving the pinned '+' row and an
/// overflow arrow row when not everything fits.
fn sidebar_fit_capacity(rows_left: u16, remaining: u16, rows_per_tab: u16) -> (usize, bool) {
    let capacity = rows_left.saturating_sub(1); // '+' button row
    let max_before_arrow = capacity / rows_per_tab;
    let down_arrow = remaining > max_before_arrow;
    let capacity = if down_arrow {
        capacity.saturating_sub(1)
    } else {
        capacity
    };
    ((capacity / rows_per_tab) as usize, down_arrow)
}

/// largest scroll offset that still keeps every remaining workspace slot visible.
pub fn max_sidebar_scroll_offset(
    area: Rect,
    workspaces: &[(usize, String, bool)],
    density: SidebarDensity,
    collapsed: bool,
    header: bool,
) -> usize {
    let len = workspaces.len();
    if len == 0 || area.width == 0 || area.height == 0 {
        return 0;
    }
    let rows_per_tab = sidebar_rows_per_tab(area, density, collapsed);
    let header_rows: u16 = if header { 3 } else { 0 };
    let bottom = area.y + area.height;
    for start in 0..len {
        let mut cursor = area.y + header_rows;
        if start > 0 {
            cursor += 1; // ↑ scroll-up row
        }
        let rows_left = bottom.saturating_sub(cursor);
        let remaining = len.saturating_sub(start) as u16;
        let (slots, _) = sidebar_fit_capacity(rows_left, remaining, rows_per_tab);
        if slots >= len - start {
            return start;
        }
    }
    len.saturating_sub(1)
}

/// one visible workspace tab in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarSlot {
    /// workspace id stored in the slot.
    pub ws: usize,
    /// row of the first line of the slot.
    pub y: u16,
    /// number of rows the slot occupies (1 or 2).
    pub row_h: u16,
    /// x of the close button (two columns wide).
    pub close_x: u16,
}

/// shared geometry for the vertical sidebar, used by both the renderer and
/// mouse hit-testing so glyphs and click zones can never disagree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SidebarLayout {
    /// rows reserved at the top for the session header (0 or 2).
    pub header_rows: u16,
    /// visible tab slots in render order.
    pub slots: Vec<SidebarSlot>,
    /// row of the '↑' scroll-up indicator, present only when scrolled down.
    pub up_arrow_y: Option<u16>,
    /// row of the '↓' scroll-down indicator, present only when tabs overflow.
    pub down_arrow_y: Option<u16>,
    /// row of the '+' new-workspace button (always the last row).
    pub add_y: Option<u16>,
}

/// computes the shared vertical sidebar geometry.
///
/// `workspaces` is the ordered list of all workspaces (unused rows are dropped);
/// `scroll_offset` is the index of the first visible workspace; overflowing
/// content is indicated by pinned ↑/↓ rows, the '+' button is pinned to the
/// bottom row, and the session header spans the first two rows when enabled.
pub fn layout_workspace_sidebar(
    area: Rect,
    workspaces: &[(usize, String, bool)],
    scroll_offset: usize,
    density: SidebarDensity,
    collapsed: bool,
    header: bool,
) -> SidebarLayout {
    let mut layout = SidebarLayout::default();
    if area.width == 0 || area.height == 0 {
        return layout;
    }
    let scroll_offset = scroll_offset.min(max_sidebar_scroll_offset(
        area, workspaces, density, collapsed, header,
    ));

    let rows_per_tab = sidebar_rows_per_tab(area, density, collapsed);

    let header_rows: u16 = if header { 3 } else { 0 };
    layout.header_rows = header_rows;

    let bottom = area.y + area.height;
    let mut cursor = area.y + header_rows;
    if scroll_offset > 0 {
        layout.up_arrow_y = Some(cursor);
        cursor += 1;
    }

    let rows_left = bottom.saturating_sub(cursor);
    // reserve the last row for the '+' button; if tabs still overflow, one row
    // more for the ↓ arrow pinned above it
    let remaining = workspaces.len().saturating_sub(scroll_offset) as u16;
    let (max_slots, down_arrow) = sidebar_fit_capacity(rows_left, remaining, rows_per_tab);
    if down_arrow {
        layout.down_arrow_y = Some(bottom - 2);
    }

    for (ws, _name, _) in workspaces.iter().skip(scroll_offset).take(max_slots) {
        layout.slots.push(SidebarSlot {
            ws: *ws,
            y: cursor,
            row_h: rows_per_tab,
            close_x: area.x + area.width.saturating_sub(2),
        });
        cursor = cursor.saturating_add(rows_per_tab);
    }

    layout.add_y = Some(bottom - 1);
    layout
}

/// renders the sidebar title, workspace count, and current session context.
fn render_sidebar_header(
    area: Rect,
    buf: &mut Buffer,
    session_name: &str,
    workspace_count: usize,
    theme: &Theme,
) {
    let right = area.x.saturating_add(area.width);

    // "workspaces" + workspace-count pill
    buf.set_stringn(
        area.x.saturating_add(1),
        area.y,
        "workspaces",
        usize::from(area.width.saturating_sub(3)),
        Style::default()
            .fg(theme.text_color())
            .bg(Color::Reset)
            .add_modifier(Modifier::BOLD),
    );
    let pill = format!(" {workspace_count} ");
    let pill_w = pill.chars().count() as u16;
    if area.width >= pill_w.saturating_add(12) {
        let pill_x = right.saturating_sub(pill_w).saturating_sub(1);
        buf.set_stringn(
            pill_x,
            area.y,
            &pill,
            usize::from(pill_w),
            Style::default()
                .fg(theme.base_color())
                .bg(theme.accent_color())
                .add_modifier(Modifier::BOLD),
        );
    }

    // ● session name
    let (end_x, _) = buf.set_stringn(
        area.x.saturating_add(1),
        area.y + 1,
        "● ",
        2,
        Style::default().fg(theme.success_color()).bg(Color::Reset),
    );
    let (_, _) = buf.set_stringn(
        end_x,
        area.y + 1,
        session_name,
        usize::from(area.width.saturating_sub(3)),
        Style::default().fg(theme.subtext_color()).bg(Color::Reset),
    );

    // full-width hairline divider before the list
    if area.height >= 3 {
        for x in area.x + 1..right {
            buf[(x, area.y + 2)]
                .set_char('─')
                .set_style(Style::default().fg(theme.overlay_color()).bg(Color::Reset));
        }
    }
}

/// renders a vertical workspace tab list with a focused-pane subtitle.
///
/// `workspaces` is the ordered list of (id, workspace name, focused pane title).
/// `active` highlights the current workspace. when `collapsed` the sidebar is an
/// icon rail showing only the workspace numbers; `density` selects one or two
/// rows per tab; `header` renders the session header at the top.
#[allow(clippy::too_many_arguments)]
pub fn render_workspace_sidebar(
    area: Rect,
    buf: &mut Buffer,
    workspaces: &[(WorkspaceId, String, String, bool)],
    active: WorkspaceId,
    session_name: &str,
    theme: &Theme,
    collapsed: bool,
    density: SidebarDensity,
    header: bool,
    scroll_offset: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let bar_style = Style::default().bg(Color::Reset).fg(theme.muted_color());

    // fill the background first
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            buf[(x, y)].set_char(' ').set_style(bar_style);
        }
    }

    let ws_pairs: Vec<(usize, String, bool)> = workspaces
        .iter()
        .map(|(id, name, _, busy)| (id.0, name.clone(), *busy))
        .collect();
    let layout =
        layout_workspace_sidebar(area, &ws_pairs, scroll_offset, density, collapsed, header);

    if header && layout.header_rows == 3 {
        if collapsed {
            // icon rail: centered count pill + divider
            let pill = format!(" {} ", workspaces.len());
            let pill_w = pill.chars().count() as u16;
            let pill_x = area.x + area.width.saturating_sub(pill_w) / 2;
            buf.set_stringn(
                pill_x,
                area.y,
                &pill,
                usize::from(pill_w),
                Style::default()
                    .fg(theme.base_color())
                    .bg(theme.accent_color())
                    .add_modifier(Modifier::BOLD),
            );
            for x in area.x..area.x + area.width {
                buf[(x, area.y + 2)]
                    .set_char('─')
                    .set_style(Style::default().fg(theme.overlay_color()).bg(Color::Reset));
            }
        } else {
            render_sidebar_header(
                Rect::new(area.x, area.y, area.width, 3),
                buf,
                session_name,
                workspaces.len(),
                theme,
            );
        }
    }

    if collapsed {
        for slot in &layout.slots {
            let is_active = slot.ws == active.0;
            let busy = workspaces
                .iter()
                .find(|entry| entry.0.0 == slot.ws)
                .map(|entry| entry.3)
                .unwrap_or(false);
            let row_bg = if is_active {
                theme.surface_color()
            } else {
                Color::Reset
            };
            let style = if busy {
                Style::default()
                    .fg(theme.warn_color())
                    .bg(row_bg)
                    .add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default()
                    .fg(theme.accent_color())
                    .bg(row_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted_color()).bg(row_bg)
            };
            if is_active {
                for x in area.x..area.x + area.width {
                    buf[(x, slot.y)]
                        .set_char(' ')
                        .set_style(Style::default().bg(row_bg));
                }
            }
            // e.g. " 4 " centered
            let text = format!("{}", slot.ws);
            let pad = area.width.saturating_sub(text.chars().count() as u16) / 2;
            let mut x = area.x + pad;
            for ch in text.chars() {
                if x < area.x + area.width {
                    buf[(x, slot.y)].set_char(ch).set_style(style);
                    x += 1;
                }
            }
            if is_active {
                buf[(area.x, slot.y)]
                    .set_char('▌')
                    .set_style(style.fg(theme.accent_color()));
            }
        }
        if let Some(y) = layout.add_y {
            buf[(area.x + area.width / 2, y)].set_char('+').set_style(
                Style::default()
                    .fg(theme.accent_color())
                    .bg(Color::Reset)
                    .add_modifier(Modifier::BOLD),
            );
        }
        if let Some(y) = layout.up_arrow_y {
            buf[(area.x + area.width / 2, y)].set_char('↑').set_style(
                Style::default()
                    .fg(theme.accent_color())
                    .bg(Color::Reset)
                    .add_modifier(Modifier::BOLD),
            );
        }
        if let Some(y) = layout.down_arrow_y {
            buf[(area.x + area.width / 2, y)].set_char('↓').set_style(
                Style::default()
                    .fg(theme.accent_color())
                    .bg(Color::Reset)
                    .add_modifier(Modifier::BOLD),
            );
        }
        return;
    }

    let add_style = Style::default().fg(theme.muted_color()).bg(Color::Reset);
    let arrow_style = Style::default()
        .fg(theme.accent_color())
        .bg(Color::Reset)
        .add_modifier(Modifier::BOLD);
    let area_right = area.x.saturating_add(area.width);

    for slot in &layout.slots {
        let is_active = slot.ws == active.0;
        let first_row = slot.y;
        let (name, hint, busy) = workspaces
            .iter()
            .find(|entry| entry.0.0 == slot.ws)
            .map(|entry| (entry.1.as_str(), entry.2.as_str(), entry.3))
            .unwrap_or(("", "", false));
        let row_bg = if is_active {
            theme.surface_color()
        } else {
            Color::Reset
        };
        let name_fg = if is_active {
            theme.text_color()
        } else {
            theme.subtext_color()
        };
        let row_style = Style::default().fg(name_fg).bg(row_bg);
        let marker_style = Style::default()
            .fg(if is_active {
                theme.accent_color()
            } else {
                theme.muted_color()
            })
            .bg(row_bg);
        let name_style = row_style.add_modifier(if is_active {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
        let index_style = Style::default()
            .fg(if busy {
                theme.warn_color()
            } else if is_active {
                theme.accent_color()
            } else {
                theme.overlay_color()
            })
            .bg(row_bg)
            .add_modifier(Modifier::BOLD);
        let close_style = Style::default()
            .fg(if is_active {
                theme.alert_color()
            } else {
                theme.overlay_color()
            })
            .bg(row_bg);

        // vertical-tab treatment: one highlighted title row with a slim
        // selection rail, followed by a quiet focused-pane subtitle.
        if is_active {
            for y in first_row..first_row.saturating_add(slot.row_h) {
                for x in area.x..area_right {
                    buf[(x, y)].set_char(' ').set_style(row_style);
                }
                buf[(area.x, y)].set_char('▌').set_style(marker_style);
            }
        } else {
            buf[(area.x, first_row)]
                .set_char(' ')
                .set_style(marker_style);
        }

        // first line: number, workspace title, and a close control aligned to
        // the edge like a vertical session-tab list.
        let index_x = area.x.saturating_add(1);
        let index_text = format!("{:>2}", slot.ws);
        let badge_width = slot.close_x.saturating_sub(index_x).min(2);
        buf.set_stringn(
            index_x,
            first_row,
            &index_text,
            usize::from(badge_width),
            index_style,
        );
        let name_x = index_x.saturating_add(badge_width).saturating_add(1);
        let name_width = slot.close_x.saturating_sub(name_x);
        let display_name = if name.is_empty() {
            format!("workspace {}", slot.ws)
        } else {
            name.to_string()
        };
        buf.set_stringn(
            name_x,
            first_row,
            display_name,
            usize::from(name_width),
            name_style,
        );
        buf[(slot.close_x, first_row)]
            .set_char('×')
            .set_style(close_style);
        if slot.close_x.saturating_add(1) < area_right {
            buf[(slot.close_x.saturating_add(1), first_row)]
                .set_char(' ')
                .set_style(close_style);
        }

        // the second row gives each workspace a useful pane hint without
        // turning every tab into a filled card.
        if slot.row_h == 2 {
            let hint_row = first_row + 1;
            let hint_style = Style::default()
                .fg(if is_active {
                    theme.subtext_color()
                } else {
                    theme.muted_color()
                })
                .bg(row_bg);
            let hint_x = area.x.saturating_add(2);
            let hint_width = area_right.saturating_sub(hint_x);
            buf.set_stringn(
                hint_x,
                hint_row,
                "·",
                1,
                Style::default().fg(theme.overlay_color()).bg(row_bg),
            );
            let hint_text = if hint.is_empty() {
                "no focused pane"
            } else {
                hint
            };
            buf.set_stringn(
                hint_x.saturating_add(2),
                hint_row,
                hint_text,
                usize::from(hint_width.saturating_sub(2)),
                hint_style,
            );
        }
    }

    if let Some(y) = layout.add_y {
        let plus_style = Style::default()
            .fg(theme.accent_color())
            .bg(Color::Reset)
            .add_modifier(Modifier::BOLD);
        let plus_x = area
            .x
            .saturating_add(1)
            .min(area.x.saturating_add(area.width).saturating_sub(1));
        buf[(plus_x, y)].set_char('+').set_style(plus_style);
        if area.width > 3 {
            buf.set_stringn(
                plus_x.saturating_add(2),
                y,
                "new workspace",
                usize::from(area.width.saturating_sub(4)),
                add_style,
            );
        }
    }
    if let Some(y) = layout.up_arrow_y {
        let ax = area.x + area.width.saturating_sub(5);
        buf[(ax, y)].set_char('↑').set_style(arrow_style);
    }
    if let Some(y) = layout.down_arrow_y {
        let ax = area.x + area.width.saturating_sub(5);
        buf[(ax, y)].set_char('↓').set_style(arrow_style);
    }
}

/// renders the persistent hint / status bar: mode badge, contextual hints,
/// and a right-aligned clock.
pub fn render_hint_bar(
    area: Rect,
    buf: &mut Buffer,
    mode: InputMode,
    hints: &str,
    theme: &Theme,
    clock: bool,
    clock_format: &str,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let bar_style = Style::default().bg(Color::Reset).fg(theme.muted_color());
    for x in area.x..area.x + area.width {
        buf[(x, area.y)].set_char(' ').set_style(bar_style);
    }

    let badge_fg = match mode {
        InputMode::Normal => theme.info_color(),
        InputMode::Resize => theme.accent_color(),
        InputMode::Copy => theme.success_color(),
        InputMode::Search => theme.alert_color(),
        InputMode::VimCmd => theme.accent_color(),
        _ => theme.muted_color(),
    };

    let mut cur_x = bar_badge(buf, area.x + 1, area.y, &mode.to_string(), badge_fg, theme);

    if !hints.is_empty() {
        cur_x += 2;
        let hints_style = Style::default().fg(theme.muted_color()).bg(Color::Reset);
        // reserve room for the badge (left) and clock (right)
        let available_width = area.width.saturating_sub(cur_x - area.x + 15) as usize;
        if available_width > 5 {
            if hints.chars().count() > available_width {
                let truncated: String = hints
                    .chars()
                    .take(available_width.saturating_sub(1))
                    .collect();
                let truncated = format!("{truncated}…");
                buf.set_stringn(cur_x, area.y, &truncated, available_width, hints_style);
            } else {
                buf.set_stringn(cur_x, area.y, hints, available_width, hints_style);
            }
        }
    }

    if clock {
        let time_disp = clock_text(clock_format);
        let time_len = time_disp.chars().count() as u16;
        let start_x = area.x + area.width.saturating_sub(time_len + 1);
        if start_x >= area.x + 2 {
            buf.set_stringn(
                start_x,
                area.y,
                &time_disp,
                time_len as usize,
                Style::default().fg(theme.muted_color()).bg(Color::Reset),
            );
        }
    }
}

/// returns the current local time formatted for the status bar.
///
/// supported escapes: `%H` `%I` `%M` `%S` `%p` `%R` `%T` `%Y` `%m` (month)
/// `%d` `%e` `%A` `%a` `%B` `%b` `%%`; unknown escapes are passed through.
pub fn clock_text(format: &str) -> String {
    use time::{Month, OffsetDateTime, Weekday};
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let hours = now.hour();
    let minutes = now.minute();
    let seconds = now.second();

    let weekday = match now.weekday() {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    };
    let weekday_short = &weekday[..3];
    let month = match now.month() {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    };
    let month_short = &month[..3];
    let day = now.day();
    let year = now.year();
    let hour12 = {
        let h = hours % 12;
        if h == 0 { 12 } else { h }
    };
    let ampm = if hours < 12 { "AM" } else { "PM" };

    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            match chars.next() {
                Some('H') => out.push_str(&format!("{hours:02}")),
                Some('I') => out.push_str(&format!("{hour12:02}")),
                Some('M') => out.push_str(&format!("{minutes:02}")),
                Some('S') => out.push_str(&format!("{seconds:02}")),
                Some('p') => out.push_str(ampm),
                Some('R') => out.push_str(&format!("{hours:02}:{minutes:02}")),
                Some('T') => out.push_str(&format!("{hours:02}:{minutes:02}:{seconds:02}")),
                Some('Y') => out.push_str(&format!("{year}")),
                Some('m') => out.push_str(&format!("{:02}", u8::from(now.month()))),
                Some('d') => out.push_str(&format!("{day:02}")),
                Some('e') => out.push_str(&format!("{day:2}")),
                Some('A') => out.push_str(weekday),
                Some('a') => out.push_str(weekday_short),
                Some('B') => out.push_str(month),
                Some('b') => out.push_str(month_short),
                Some('%') => out.push('%'),
                Some(other) => {
                    out.push('%');
                    out.push(other);
                }
                None => out.push('%'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// renders pane border chrome, title and an optional scroll indicator.
///
/// the indicator is a small right-aligned badge in the top border, e.g.
/// "↑12", shown only when the pane is scrolled back from the live bottom.
#[allow(clippy::too_many_arguments)]
pub fn render_pane_border(
    rect: Rect,
    buf: &mut Buffer,
    title: &str,
    is_focused: bool,
    theme: &Theme,
    ascii: bool,
    border_style: BorderStyle,
    scroll_back: Option<usize>,
) {
    if rect.width < 2 || rect.height < 2 {
        return;
    }

    let glyphs = Glyphs::styled(ascii, border_style);

    // semantic borders: accented for focused pane, muted for unfocused
    let border_color = if is_focused {
        theme.accent_color()
    } else {
        theme.muted_color()
    };
    let style = Style::default().fg(border_color).bg(Color::Reset);

    for x in rect.x..rect.x + rect.width {
        buf[(x, rect.y)].set_char(glyphs.h).set_style(style);
        buf[(x, rect.y + rect.height - 1)]
            .set_char(glyphs.h)
            .set_style(style);
    }
    for y in rect.y..rect.y + rect.height {
        buf[(rect.x, y)].set_char(glyphs.v).set_style(style);
        buf[(rect.x + rect.width - 1, y)]
            .set_char(glyphs.v)
            .set_style(style);
    }
    buf[(rect.x, rect.y)]
        .set_char(glyphs.top_left)
        .set_style(style);
    buf[(rect.x + rect.width - 1, rect.y)]
        .set_char(glyphs.top_right)
        .set_style(style);
    buf[(rect.x, rect.y + rect.height - 1)]
        .set_char(glyphs.bottom_left)
        .set_style(style);
    buf[(rect.x + rect.width - 1, rect.y + rect.height - 1)]
        .set_char(glyphs.bottom_right)
        .set_style(style);

    if !title.is_empty() && rect.width > 8 {
        let title_str = format!(" {} ", title);
        let max_title_len = (rect.width.saturating_sub(4)) as usize;
        let title_display = if title_str.len() > max_title_len {
            let truncated = title_str.chars().take(max_title_len).collect::<String>();
            format!("{}…", truncated.trim_end())
        } else {
            title_str
        };

        let title_style = if is_focused {
            Style::default()
                .fg(theme.accent_color())
                .bg(Color::Reset)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.muted_color())
                .bg(Color::Reset)
                .add_modifier(Modifier::DIM)
        };

        let title_start = rect.x + (rect.width.saturating_sub(title_display.len() as u16)) / 2;
        for (i, ch) in title_display.chars().enumerate() {
            let x = title_start + (i as u16);
            if x >= rect.x + 2 && x < rect.x + rect.width - 2 {
                buf[(x, rect.y)].set_char(ch).set_style(title_style);
            }
        }
    }

    // scroll indicator: right-aligned in the top border, only while the pane
    // is scrolled back from the live bottom.
    if let Some(lines) = scroll_back
        && lines > 0
        && rect.width > 12
    {
        let label = format!("\u{2191}{lines} ");
        let style = Style::default()
            .fg(theme.accent_color())
            .bg(Color::Reset)
            .add_modifier(Modifier::BOLD);
        let start = rect.x + rect.width.saturating_sub(label.chars().count() as u16 + 1);
        for (i, ch) in label.chars().enumerate() {
            let x = start + i as u16;
            if x > rect.x + 1 && x < rect.x + rect.width - 1 {
                buf[(x, rect.y)].set_char(ch).set_style(style);
            }
        }
    }
}
