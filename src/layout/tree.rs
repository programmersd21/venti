use ratatui::layout::Rect;

use serde::{Deserialize, Serialize};
use std::fmt;

/// monotonically allocated pane identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u32);

impl fmt::Display for PaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// split orientation axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitAxis {
    Horizontal, // Split along columns (left / right)
    Vertical,   // Split along rows (top / bottom)
}

/// direction to walk from a split into one of its children
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    First,
    Second,
}

/// dwindle layout binary tree node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutNode {
    Leaf(PaneId),
    Split {
        axis: SplitAxis,
        ratio: f32, // clamped to 0.1..=0.9
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    /// creates a leaf node holding a pane.
    pub fn leaf(id: PaneId) -> Self {
        Self::Leaf(id)
    }

    /// creates a split node with two children, clamping ratio to 0.1..=0.9.
    pub fn split(axis: SplitAxis, ratio: f32, first: Self, second: Self) -> Self {
        Self::Split {
            axis,
            ratio: ratio.clamp(0.1, 0.9),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// collects all pane ids in depth-first order.
    /// returns the axis of the split immediately enclosing `target`.
    pub fn parent_split_axis(&self, target: PaneId) -> Option<SplitAxis> {
        match self {
            Self::Leaf(_) => None,
            Self::Split {
                axis,
                first,
                second,
                ..
            } => {
                if matches!(&**first, Self::Leaf(id) if *id == target)
                    || matches!(&**second, Self::Leaf(id) if *id == target)
                {
                    return Some(*axis);
                }
                first
                    .parent_split_axis(target)
                    .or_else(|| second.parent_split_axis(target))
            }
        }
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    fn collect_ids(&self, out: &mut Vec<PaneId>) {
        match self {
            Self::Leaf(id) => out.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_ids(out);
                second.collect_ids(out);
            }
        }
    }

    /// checks if a pane id exists in this subtree.
    pub fn contains(&self, target: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == target,
            Self::Split { first, second, .. } => first.contains(target) || second.contains(target),
        }
    }

    /// splits the target leaf pane with a new pane, inserting a new split node.
    pub fn split_pane(
        self,
        target: PaneId,
        new_pane: PaneId,
        axis: SplitAxis,
        ratio: f32,
    ) -> Result<Self, Self> {
        self.split_pane_directed(target, new_pane, axis, ratio, SplitDirection::Second)
    }

    /// splits the target leaf pane with a new pane, with control over which side goes first.
    /// SplitDirection::First places new_pane on the left/top, SplitDirection::Second on the right/bottom.
    pub fn split_pane_directed(
        self,
        target: PaneId,
        new_pane: PaneId,
        axis: SplitAxis,
        ratio: f32,
        direction: SplitDirection,
    ) -> Result<Self, Self> {
        match self {
            Self::Leaf(id) => {
                if id == target {
                    let (first, second) = match direction {
                        SplitDirection::First => (Self::Leaf(new_pane), Self::Leaf(id)),
                        SplitDirection::Second => (Self::Leaf(id), Self::Leaf(new_pane)),
                    };
                    Ok(Self::split(axis, ratio, first, second))
                } else {
                    Err(Self::Leaf(id))
                }
            }
            Self::Split {
                axis: a,
                ratio: r,
                first,
                second,
            } => {
                if first.contains(target) {
                    let new_first =
                        match first.split_pane_directed(target, new_pane, axis, ratio, direction) {
                            Ok(t) => t,
                            Err(t) => {
                                return Err(Self::Split {
                                    axis: a,
                                    ratio: r,
                                    first: Box::new(t),
                                    second,
                                });
                            }
                        };
                    Ok(Self::Split {
                        axis: a,
                        ratio: r,
                        first: Box::new(new_first),
                        second,
                    })
                } else if second.contains(target) {
                    let new_second = match second
                        .split_pane_directed(target, new_pane, axis, ratio, direction)
                    {
                        Ok(t) => t,
                        Err(t) => {
                            return Err(Self::Split {
                                axis: a,
                                ratio: r,
                                first,
                                second: Box::new(t),
                            });
                        }
                    };
                    Ok(Self::Split {
                        axis: a,
                        ratio: r,
                        first,
                        second: Box::new(new_second),
                    })
                } else {
                    Err(Self::Split {
                        axis: a,
                        ratio: r,
                        first,
                        second,
                    })
                }
            }
        }
    }

    /// removes a pane from the tree, collapsing its parent split so the sibling takes its space.
    /// returns none if the tree contained only the removed pane.
    pub fn remove_pane(self, target: PaneId) -> Result<Option<Self>, Self> {
        match self {
            Self::Leaf(id) => {
                if id == target {
                    Ok(None)
                } else {
                    Err(Self::Leaf(id))
                }
            }
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                if let Self::Leaf(id) = *first
                    && id == target
                {
                    return Ok(Some(*second));
                }
                if let Self::Leaf(id) = *second
                    && id == target
                {
                    return Ok(Some(*first));
                }

                if first.contains(target) {
                    match first.remove_pane(target) {
                        Ok(Some(new_first)) => Ok(Some(Self::Split {
                            axis,
                            ratio,
                            first: Box::new(new_first),
                            second,
                        })),
                        Ok(None) => Ok(Some(*second)),
                        Err(new_first) => Err(Self::Split {
                            axis,
                            ratio,
                            first: Box::new(new_first),
                            second,
                        }),
                    }
                } else if second.contains(target) {
                    match second.remove_pane(target) {
                        Ok(Some(new_second)) => Ok(Some(Self::Split {
                            axis,
                            ratio,
                            first,
                            second: Box::new(new_second),
                        })),
                        Ok(None) => Ok(Some(*first)),
                        Err(new_second) => Err(Self::Split {
                            axis,
                            ratio,
                            first,
                            second: Box::new(new_second),
                        }),
                    }
                } else {
                    Err(Self::Split {
                        axis,
                        ratio,
                        first,
                        second,
                    })
                }
            }
        }
    }

    /// swaps the position of two pane leaves in the tree.
    pub fn swap_panes(&mut self, a: PaneId, b: PaneId) -> bool {
        let mut leaf_a = None;
        let mut leaf_b = None;

        fn find_leaves<'a>(
            node: &'a mut LayoutNode,
            a: PaneId,
            b: PaneId,
            leaf_a: &mut Option<&'a mut PaneId>,
            leaf_b: &mut Option<&'a mut PaneId>,
        ) {
            match node {
                LayoutNode::Leaf(id) => {
                    if *id == a {
                        *leaf_a = Some(id);
                    } else if *id == b {
                        *leaf_b = Some(id);
                    }
                }
                LayoutNode::Split { first, second, .. } => {
                    find_leaves(first, a, b, leaf_a, leaf_b);
                    find_leaves(second, a, b, leaf_a, leaf_b);
                }
            }
        }

        find_leaves(self, a, b, &mut leaf_a, &mut leaf_b);

        if let (Some(ref_a), Some(ref_b)) = (leaf_a, leaf_b) {
            std::mem::swap(ref_a, ref_b);
            true
        } else {
            false
        }
    }

    /// equalizes all splits recursively to a 0.5 ratio.
    pub fn equalize(&mut self) {
        if let Self::Split {
            ratio,
            first,
            second,
            ..
        } = self
        {
            *ratio = 0.5;
            first.equalize();
            second.equalize();
        }
    }

    /// rotates the nearest ancestor split containing the target pane.
    pub fn rotate_split(&mut self, target: PaneId) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                axis,
                first,
                second,
                ..
            } => {
                if first.contains(target) {
                    if !first.rotate_split(target) {
                        *axis = match *axis {
                            SplitAxis::Horizontal => SplitAxis::Vertical,
                            SplitAxis::Vertical => SplitAxis::Horizontal,
                        };
                    }
                    true
                } else if second.contains(target) {
                    if !second.rotate_split(target) {
                        *axis = match *axis {
                            SplitAxis::Horizontal => SplitAxis::Vertical,
                            SplitAxis::Vertical => SplitAxis::Horizontal,
                        };
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    /// finds the depth-first path to the split whose boundary the mouse is over.
    pub fn split_at(
        &self,
        area: Rect,
        gap_inner: u16,
        col: u16,
        row: u16,
    ) -> Option<(SplitAxis, Vec<SplitDirection>)> {
        // add tolerance around split borders for easier mouse targeting (3 cells on each side)
        let tolerance = 3u16;
        self.find_split_at(area, gap_inner, col, row, tolerance)
    }

    fn find_split_at(
        &self,
        rect: Rect,
        gap_inner: u16,
        col: u16,
        row: u16,
        tolerance: u16,
    ) -> Option<(SplitAxis, Vec<SplitDirection>)> {
        match self {
            Self::Leaf(_) => None,
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let r = *ratio;
                let r = if r.is_finite() {
                    r.clamp(0.0, 1.0)
                } else {
                    0.5
                };
                if *axis == SplitAxis::Horizontal {
                    let total_w = rect.width.saturating_sub(gap_inner);
                    let first_w = (total_w as f32 * r).floor() as u16;
                    let boundary = rect.x.saturating_add(first_w);
                    let boundary_end = boundary.saturating_add(gap_inner);
                    // expand hit zone: tolerance cells before and after
                    let hit_start = boundary.saturating_sub(tolerance);
                    let hit_end = boundary_end.saturating_add(tolerance);
                    if col >= hit_start
                        && col < hit_end
                        && row >= rect.y
                        && row < rect.y.saturating_add(rect.height)
                    {
                        return Some((*axis, Vec::new()));
                    }
                    let first_rect = Rect {
                        x: rect.x,
                        y: rect.y,
                        width: first_w,
                        height: rect.height,
                    };
                    if let Some((a, mut path)) =
                        first.find_split_at(first_rect, gap_inner, col, row, tolerance)
                    {
                        path.insert(0, SplitDirection::First);
                        return Some((a, path));
                    }
                    let second_rect = Rect {
                        x: boundary_end,
                        y: rect.y,
                        width: total_w.saturating_sub(first_w),
                        height: rect.height,
                    };
                    if let Some((a, mut path)) =
                        second.find_split_at(second_rect, gap_inner, col, row, tolerance)
                    {
                        path.insert(0, SplitDirection::Second);
                        return Some((a, path));
                    }
                    None
                } else {
                    let total_h = rect.height.saturating_sub(gap_inner);
                    let first_h = (total_h as f32 * r).floor() as u16;
                    let boundary = rect.y.saturating_add(first_h);
                    let boundary_end = boundary.saturating_add(gap_inner);
                    // expand hit zone: tolerance cells before and after
                    let hit_start = boundary.saturating_sub(tolerance);
                    let hit_end = boundary_end.saturating_add(tolerance);
                    if row >= hit_start
                        && row < hit_end
                        && col >= rect.x
                        && col < rect.x.saturating_add(rect.width)
                    {
                        return Some((*axis, Vec::new()));
                    }
                    let first_rect = Rect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: first_h,
                    };
                    if let Some((a, mut path)) =
                        first.find_split_at(first_rect, gap_inner, col, row, tolerance)
                    {
                        path.insert(0, SplitDirection::First);
                        return Some((a, path));
                    }
                    let second_rect = Rect {
                        x: rect.x,
                        y: boundary_end,
                        width: rect.width,
                        height: total_h.saturating_sub(first_h),
                    };
                    if let Some((a, mut path)) =
                        second.find_split_at(second_rect, gap_inner, col, row, tolerance)
                    {
                        path.insert(0, SplitDirection::Second);
                        return Some((a, path));
                    }
                    None
                }
            }
        }
    }

    /// drags a split separator to a new position, changing the split ratio.
    pub fn drag_split(
        &mut self,
        area: Rect,
        gap_inner: u16,
        path: &[SplitDirection],
        axis: SplitAxis,
        col: u16,
        row: u16,
    ) {
        let mut cur_rect = area;
        let mut cur_node = &mut *self;
        for step in path {
            match cur_node {
                LayoutNode::Split {
                    axis: a,
                    ratio,
                    first,
                    second,
                } => {
                    let r = if ratio.is_finite() {
                        ratio.clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    if *a == SplitAxis::Horizontal {
                        let total_w = cur_rect.width.saturating_sub(gap_inner);
                        let first_w = (total_w as f32 * r).floor() as u16;
                        let boundary_end =
                            cur_rect.x.saturating_add(first_w).saturating_add(gap_inner);
                        match step {
                            SplitDirection::First => {
                                cur_rect = Rect {
                                    x: cur_rect.x,
                                    y: cur_rect.y,
                                    width: first_w,
                                    height: cur_rect.height,
                                };
                                cur_node = first;
                            }
                            SplitDirection::Second => {
                                cur_rect = Rect {
                                    x: boundary_end,
                                    y: cur_rect.y,
                                    width: total_w.saturating_sub(first_w),
                                    height: cur_rect.height,
                                };
                                cur_node = second;
                            }
                        }
                    } else {
                        let total_h = cur_rect.height.saturating_sub(gap_inner);
                        let first_h = (total_h as f32 * r).floor() as u16;
                        let boundary_end =
                            cur_rect.y.saturating_add(first_h).saturating_add(gap_inner);
                        match step {
                            SplitDirection::First => {
                                cur_rect = Rect {
                                    x: cur_rect.x,
                                    y: cur_rect.y,
                                    width: cur_rect.width,
                                    height: first_h,
                                };
                                cur_node = first;
                            }
                            SplitDirection::Second => {
                                cur_rect = Rect {
                                    x: cur_rect.x,
                                    y: boundary_end,
                                    width: cur_rect.width,
                                    height: total_h.saturating_sub(first_h),
                                };
                                cur_node = second;
                            }
                        }
                    }
                }
                LayoutNode::Leaf(_) => return,
            }
        }
        let total = match axis {
            SplitAxis::Horizontal => cur_rect.width.saturating_sub(gap_inner),
            SplitAxis::Vertical => cur_rect.height.saturating_sub(gap_inner),
        };
        let origin = match axis {
            SplitAxis::Horizontal => cur_rect.x,
            SplitAxis::Vertical => cur_rect.y,
        };
        let dim = match axis {
            SplitAxis::Horizontal => col,
            SplitAxis::Vertical => row,
        };
        if let LayoutNode::Split { ratio, .. } = cur_node {
            let new_first = dim as i32 - origin as i32;
            if total > 0 {
                let new_ratio = (new_first as f32 / total as f32).clamp(0.1, 0.9);
                if new_ratio.is_finite() {
                    *ratio = new_ratio;
                }
            }
        }
    }

    /// adjusts split ratio for the nearest ancestor split along the given axis containing target.
    /// `delta` is applied directly to the ratio (positive = boundary moves down/right, negative = up/left),
    /// regardless of which side of the split the focused pane sits on, so arrow-key resizes are directional.
    /// returns true if a split was adjusted, false if the target exists but no split on this axis does.
    pub fn resize_pane(&mut self, target: PaneId, axis: SplitAxis, delta: f32) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                axis: a,
                ratio,
                first,
                second,
            } => {
                let first_contains = first.contains(target);
                let second_contains = !first_contains && second.contains(target);
                if !first_contains && !second_contains {
                    return false;
                }
                // prefer a deeper split on the same axis; fall back to this one.
                let child = if first_contains {
                    first.as_mut()
                } else {
                    second.as_mut()
                };
                if child.resize_pane(target, axis, delta) {
                    return true;
                }
                if *a == axis {
                    *ratio = (*ratio + delta).clamp(0.1, 0.9);
                    return true;
                }
                false
            }
        }
    }
}
