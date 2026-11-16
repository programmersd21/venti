use super::tree::PaneId;
use ratatui::layout::Rect;
use std::collections::HashMap;

/// direction for spatial focus navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

/// tracks focus state and per-leaf focus recency clocks.
#[derive(Debug, Clone, Default)]
pub struct FocusManager {
    focused: Option<PaneId>,
    focus_clock: HashMap<PaneId, u64>,
    counter: u64,
}

impl FocusManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// sets focus to the specified pane, updating recency clock.
    pub fn set_focus(&mut self, id: PaneId) {
        self.counter += 1;
        self.focus_clock.insert(id, self.counter);
        self.focused = Some(id);
    }

    /// removes pane from recency tracking.
    pub fn remove(&mut self, id: PaneId) {
        self.focus_clock.remove(&id);
        if self.focused == Some(id) {
            self.focused = None;
        }
    }

    /// selects the most recently focused pane among a candidate list.
    pub fn most_recent(&self, candidates: &[PaneId]) -> Option<PaneId> {
        candidates
            .iter()
            .max_by_key(|id| self.focus_clock.get(id).copied().unwrap_or(0))
            .copied()
    }

    /// computes the spatial neighbor of a focused rect in the specified direction.
    pub fn spatial_neighbor(
        &self,
        current: PaneId,
        dir: Direction,
        rects: &[(PaneId, Rect)],
    ) -> Option<PaneId> {
        let current_rect = rects.iter().find(|(id, _)| *id == current)?.1;

        let mut best_candidate: Option<(PaneId, u32, u64)> = None;

        for &(candidate_id, r) in rects {
            if candidate_id == current {
                continue;
            }

            let (is_in_direction, distance, overlap) = match dir {
                Direction::Left => {
                    let r_end = (r.x as u32).saturating_add(r.width as u32);
                    let cur_x = current_rect.x as u32;
                    let in_dir = r_end <= cur_x;
                    let dist = cur_x.saturating_sub(r_end);
                    let overlap = overlap_span(r.y, r.height, current_rect.y, current_rect.height);
                    (in_dir, dist, overlap)
                }
                Direction::Right => {
                    let r_x = r.x as u32;
                    let cur_end = (current_rect.x as u32).saturating_add(current_rect.width as u32);
                    let in_dir = r_x >= cur_end;
                    let dist = r_x.saturating_sub(cur_end);
                    let overlap = overlap_span(r.y, r.height, current_rect.y, current_rect.height);
                    (in_dir, dist, overlap)
                }
                Direction::Up => {
                    let r_end = (r.y as u32).saturating_add(r.height as u32);
                    let cur_y = current_rect.y as u32;
                    let in_dir = r_end <= cur_y;
                    let dist = cur_y.saturating_sub(r_end);
                    let overlap = overlap_span(r.x, r.width, current_rect.x, current_rect.width);
                    (in_dir, dist, overlap)
                }
                Direction::Down => {
                    let r_y = r.y as u32;
                    let cur_end =
                        (current_rect.y as u32).saturating_add(current_rect.height as u32);
                    let in_dir = r_y >= cur_end;
                    let dist = r_y.saturating_sub(cur_end);
                    let overlap = overlap_span(r.x, r.width, current_rect.x, current_rect.width);
                    (in_dir, dist, overlap)
                }
            };

            if is_in_direction && overlap > 0 {
                let recency = self.focus_clock.get(&candidate_id).copied().unwrap_or(0);
                match best_candidate {
                    None => {
                        best_candidate = Some((candidate_id, distance, recency));
                    }
                    Some((_, best_dist, best_recency)) => {
                        if distance < best_dist || (distance == best_dist && recency > best_recency)
                        {
                            best_candidate = Some((candidate_id, distance, recency));
                        }
                    }
                }
            }
        }

        best_candidate.map(|(id, _, _)| id)
    }
}

fn overlap_span(start1: u16, len1: u16, start2: u16, len2: u16) -> u16 {
    let end1 = (start1 as u32).saturating_add(len1 as u32);
    let end2 = (start2 as u32).saturating_add(len2 as u32);
    let overlap_start = start1.max(start2) as u32;
    let overlap_end = end1.min(end2);
    overlap_end
        .saturating_sub(overlap_start)
        .min(u16::MAX as u32) as u16
}
