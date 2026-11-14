use super::tree::{LayoutNode, PaneId, SplitAxis};
use ratatui::layout::Rect;

/// resolves a layout tree into pane rectangles given available area and constraints.
pub fn resolve(
    node: &LayoutNode,
    area: Rect,
    gap_inner: u16,
    gap_outer: u16,
) -> Vec<(PaneId, Rect)> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }

    // clamp each outer gap to the half-size of its axis. This keeps even an
    // oversized configured gap inside the parent rectangle.
    let gap_x = gap_outer.min(area.width / 2);
    let gap_y = gap_outer.min(area.height / 2);
    let usable_x = area.x.saturating_add(gap_x);
    let usable_y = area.y.saturating_add(gap_y);
    let usable_w = area.width.saturating_sub(gap_x.saturating_mul(2));
    let usable_h = area.height.saturating_sub(gap_y.saturating_mul(2));

    let usable = Rect {
        x: usable_x,
        y: usable_y,
        width: usable_w,
        height: usable_h,
    };

    let mut out = Vec::new();
    resolve_node(node, usable, gap_inner, &mut out);
    out
}

fn split_length(total: u16, ratio: f32) -> u16 {
    // defensive clamp for ratios that bypass layoutnode::split (e.g. serde payloads).
    let ratio = if ratio.is_finite() {
        ratio.clamp(0.0, 1.0)
    } else {
        0.5
    };
    ((total as f32) * ratio).floor() as u16
}

fn resolve_node(node: &LayoutNode, rect: Rect, gap_inner: u16, out: &mut Vec<(PaneId, Rect)>) {
    match node {
        LayoutNode::Leaf(id) => {
            out.push((*id, rect));
        }
        LayoutNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            match axis {
                SplitAxis::Horizontal => {
                    // split width between first (left) and second (right)
                    let total_w = rect.width.saturating_sub(gap_inner);
                    let first_w = split_length(total_w, *ratio);
                    let second_w = total_w.saturating_sub(first_w);

                    let first_rect = Rect {
                        x: rect.x,
                        y: rect.y,
                        width: first_w,
                        height: rect.height,
                    };
                    let second_rect = Rect {
                        x: rect.x.saturating_add(first_w).saturating_add(gap_inner),
                        y: rect.y,
                        width: second_w,
                        height: rect.height,
                    };

                    resolve_node(first, first_rect, gap_inner, out);
                    resolve_node(second, second_rect, gap_inner, out);
                }
                SplitAxis::Vertical => {
                    // split height between first (top) and second (bottom)
                    let total_h = rect.height.saturating_sub(gap_inner);
                    let first_h = split_length(total_h, *ratio);
                    let second_h = total_h.saturating_sub(first_h);

                    let first_rect = Rect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: first_h,
                    };
                    let second_rect = Rect {
                        x: rect.x,
                        y: rect.y.saturating_add(first_h).saturating_add(gap_inner),
                        width: rect.width,
                        height: second_h,
                    };

                    resolve_node(first, first_rect, gap_inner, out);
                    resolve_node(second, second_rect, gap_inner, out);
                }
            }
        }
    }
}

/// chooses the split axis according to terminal cell aspect ratio (roughly 1:2).
pub fn choose_aspect_split(rect: Rect) -> SplitAxis {
    // when width >= height * 2, split horizontally (left/right columns)
    if (rect.width as u32) >= (rect.height as u32) * 2 {
        SplitAxis::Horizontal
    } else {
        SplitAxis::Vertical
    }
}
