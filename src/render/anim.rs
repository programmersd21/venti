use crate::config::schema::EasingType;
use crate::layout::PaneId;
use ratatui::layout::Rect;
use std::collections::HashMap;
use std::time::Instant;

/// evaluates easing functions over normalized progress `t` in `0.0..=1.0`.
pub fn ease(t: f32, easing: EasingType) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        EasingType::Linear => t,
        EasingType::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        EasingType::EaseInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        EasingType::EaseOutQuint => 1.0 - (1.0 - t).powi(5),
    }
}

/// interpolates between two `Rect`s, rounding to nearest to avoid stutter.
fn lerp_rect(from: Rect, to: Rect, t: f32) -> Rect {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: u16, b: u16| ((a as f32) + (b as f32 - a as f32) * t).round() as u16;
    Rect {
        x: lerp(from.x, to.x),
        y: lerp(from.y, to.y),
        width: lerp(from.width, to.width),
        height: lerp(from.height, to.height),
    }
}

/// tracks a timed animation with an easing curve.
#[derive(Debug, Clone)]
pub struct AnimationTracker {
    pub start: Instant,
    pub duration_ms: u64,
    pub easing: EasingType,
    pub enabled: bool,
}

impl AnimationTracker {
    pub fn new(duration_ms: u64, easing: EasingType, enabled: bool) -> Self {
        Self {
            start: Instant::now(),
            duration_ms,
            easing,
            enabled,
        }
    }

    /// returns current progress in `0.0..=1.0`.
    pub fn progress(&self) -> f32 {
        if !self.enabled || self.duration_ms == 0 {
            return 1.0;
        }
        let t = (self.start.elapsed().as_millis() as f32 / self.duration_ms as f32).min(1.0);
        ease(t, self.easing)
    }

    pub fn is_finished(&self) -> bool {
        !self.enabled || self.start.elapsed().as_millis() >= self.duration_ms as u128
    }
}

/// animation state for a single pane transitioning between layout rects.
#[derive(Debug, Clone)]
pub struct PaneAnimationState {
    pub from_rect: Rect,
    pub to_rect: Rect,
    pub tracker: AnimationTracker,
}

impl PaneAnimationState {
    pub fn new(from: Rect, to: Rect, duration_ms: u64, easing: EasingType, enabled: bool) -> Self {
        Self {
            from_rect: from,
            to_rect: to,
            tracker: AnimationTracker::new(duration_ms, easing, enabled),
        }
    }

    pub fn current_rect(&self) -> Rect {
        lerp_rect(self.from_rect, self.to_rect, self.tracker.progress())
    }
}

/// manages per-pane layout animations.
#[derive(Debug, Clone, Default)]
pub struct PaneAnimationManager {
    pub animations: HashMap<PaneId, PaneAnimationState>,
}

impl PaneAnimationManager {
    pub fn animate_pane(
        &mut self,
        pane_id: PaneId,
        from: Rect,
        to: Rect,
        duration_ms: u64,
        easing: EasingType,
        enabled: bool,
    ) {
        self.animations.insert(
            pane_id,
            PaneAnimationState::new(from, to, duration_ms, easing, enabled),
        );
    }

    /// returns the interpolated rect for a pane if its animation is still running.
    pub fn get_animated_rect(&self, pane_id: PaneId) -> Option<Rect> {
        self.animations.get(&pane_id).and_then(|anim| {
            if anim.tracker.is_finished() {
                None
            } else {
                Some(anim.current_rect())
            }
        })
    }

    pub fn cleanup_finished(&mut self) {
        self.animations
            .retain(|_, anim| !anim.tracker.is_finished());
    }

    pub fn has_active(&self) -> bool {
        !self.animations.is_empty()
    }

    pub fn is_animating(&self, pane_id: PaneId) -> bool {
        self.animations
            .get(&pane_id)
            .map(|anim| !anim.tracker.is_finished())
            .unwrap_or(false)
    }
}
