use super::tree::{LayoutNode, PaneId, SplitAxis};
use ratatui::layout::Rect;

/// floating pane with absolute position and z-index.
#[derive(Debug, Clone, PartialEq)]
pub struct FloatingPane {
    pub id: PaneId,
    pub rect: Rect,
    pub z_index: u32,
    /// layout tree captured when the pane was floated.
    pub restore_tree: Option<LayoutNode>,
    /// expected tree after the pane is removed for floating (used to detect no intervening tiled changes).
    pub restore_after: Option<LayoutNode>,
    /// leaf index of the pane at the time it floated (used for fallback re-insertion).
    pub restore_order: Option<usize>,
    /// axis of the split immediately enclosing the pane at float time.
    pub restore_axis: Option<SplitAxis>,
}

impl FloatingPane {
    /// creates a floating pane with rect and initial z-index.
    pub fn new(id: PaneId, rect: Rect, z_index: u32) -> Self {
        Self {
            id,
            rect,
            z_index,
            restore_tree: None,
            restore_after: None,
            restore_order: None,
            restore_axis: None,
        }
    }

    /// centers the floating pane within container boundaries.
    pub fn center(&mut self, container: Rect) {
        if self.rect.width >= container.width {
            self.rect.x = container.x;
            self.rect.width = container.width;
        } else {
            self.rect.x = container.x + (container.width - self.rect.width) / 2;
        }

        if self.rect.height >= container.height {
            self.rect.y = container.y;
            self.rect.height = container.height;
        } else {
            self.rect.y = container.y + (container.height - self.rect.height) / 2;
        }
    }

    /// clamps the floating pane inside container boundaries.
    pub fn clamp(&mut self, container: Rect) {
        clamp_rect(&mut self.rect, container);
    }
}

/// clamps `rect` inside `container` boundaries.
pub fn clamp_rect(rect: &mut Rect, container: Rect) {
    rect.width = rect.width.min(container.width);
    rect.height = rect.height.min(container.height);

    let cx = container.x as i64;
    let cy = container.y as i64;
    let right = cx + container.width as i64;
    let bottom = cy + container.height as i64;

    let rect_right = rect.x as i64 + rect.width as i64;
    if (rect.x as i64) < cx {
        rect.x = container.x;
    } else if rect_right > right {
        rect.x = (right - rect.width as i64).max(cx) as u16;
    }

    let rect_bottom = rect.y as i64 + rect.height as i64;
    if (rect.y as i64) < cy {
        rect.y = container.y;
    } else if rect_bottom > bottom {
        rect.y = (bottom - rect.height as i64).max(cy) as u16;
    }
}
