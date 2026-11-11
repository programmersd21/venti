use crate::layout::{
    Direction, FloatingPane, FocusManager, LayoutNode, PaneId, SplitAxis, choose_aspect_split,
    resolve,
};
use crate::pane::Pane;
use ratatui::layout::Rect;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// remaps old pane ids onto freshly spawned ids, pruning failed leaves.
fn remap_node(
    node: &LayoutNode,
    map: &HashMap<u32, PaneId>,
    placed: &mut std::collections::HashSet<u32>,
) -> Option<LayoutNode> {
    match node {
        LayoutNode::Leaf(id) => placed
            .insert(id.0)
            .then(|| map.get(&id.0).copied())
            .flatten()
            .map(LayoutNode::Leaf),
        LayoutNode::Split {
            axis,
            ratio,
            first,
            second,
        } => match (
            remap_node(first, map, placed),
            remap_node(second, map, placed),
        ) {
            (Some(a), Some(b)) => Some(LayoutNode::Split {
                axis: *axis,
                ratio: *ratio,
                first: Box::new(a),
                second: Box::new(b),
            }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        },
    }
}

/// outcome of toggling the scratchpad pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScratchAction {
    Created,
    Shown,
    Hidden,
}

#[derive(Debug, Clone, Copy)]
pub struct RestoreLimits {
    pub max_panes: usize,
    pub max_workspaces: usize,
}

/// multiplexer session owning workspaces, panes, and layout state.
///
/// each workspace has its own binary tiling tree. panes themselves are owned
/// by the session so they survive workspace switches.
pub struct Session {
    pub name: String,
    /// graphics environment used for panes created in this session.
    pub outer_term: String,
    pub outer_graphics: crate::util::outer_term::OuterGraphics,
    pub panes: HashMap<PaneId, Pane>,
    pub workspaces: BTreeMap<usize, LayoutNode>,
    pub floating_panes: BTreeMap<usize, Vec<FloatingPane>>,
    pub workspace_names: BTreeMap<usize, String>,
    pub active_workspace: usize,
    pub focused_pane: Option<PaneId>,
    pub focus_clock: FocusManager,
    pub zoomed: Option<PaneId>,
    pub scratch_pane: Option<PaneId>,
    scratch_hidden: Option<FloatingPane>,
    /// when true every frame resolves to only the focused pane (plus floats).
    pub monocle: bool,
}

impl Drop for Session {
    fn drop(&mut self) {
        for pane in self.panes.values() {
            pane.kill_tree();
        }
    }
}

impl Session {
    pub fn new(name: impl Into<String>) -> Self {
        let outer_term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
        let outer_graphics = crate::util::outer_term::from_client_term(&outer_term);
        Self {
            name: name.into(),
            outer_term,
            outer_graphics,
            panes: HashMap::new(),
            workspaces: BTreeMap::new(),
            floating_panes: BTreeMap::new(),
            workspace_names: BTreeMap::new(),
            active_workspace: 1,
            focused_pane: None,
            focus_clock: FocusManager::new(),
            zoomed: None,
            scratch_pane: None,
            scratch_hidden: None,
            monocle: false,
        }
    }

    /// sets the human readable names for the default workspaces.
    pub fn set_workspace_names(&mut self, names: &[String]) {
        for (idx, name) in names.iter().enumerate() {
            self.workspace_names.insert(idx + 1, name.clone());
        }
    }

    /// returns the name for a workspace, falling back to a numbered label.
    pub fn workspace_name(&self, id: usize) -> String {
        self.workspace_names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("ws{id}"))
    }

    /// union of workspace ids present in the trees, floating panes and names.
    fn workspace_id_set(&self) -> std::collections::BTreeSet<usize> {
        self.workspaces
            .keys()
            .chain(self.floating_panes.keys())
            .chain(self.workspace_names.keys())
            .copied()
            .collect()
    }

    /// returns the sorted list of workspace ids, including configured names and the active one.
    pub fn workspace_ids(&self) -> Vec<usize> {
        let mut ids = self.workspace_id_set();
        ids.insert(self.active_workspace);
        ids.into_iter().collect()
    }

    /// returns the tiling tree for the active workspace, if any.
    pub fn active_tree(&self) -> Option<&LayoutNode> {
        self.workspaces.get(&self.active_workspace)
    }

    /// returns the tiling tree for the active workspace mutably, if any.
    pub fn active_tree_mut(&mut self) -> Option<&mut LayoutNode> {
        self.workspaces.get_mut(&self.active_workspace)
    }

    /// returns the pane that would be focused when switching to `ws`:
    /// the most recently focused pane in that workspace, or the first if none.
    pub fn focused_pane_in(&self, ws: usize) -> Option<PaneId> {
        let ids = self.workspace_pane_ids(ws);
        self.focus_clock
            .most_recent(&ids)
            .or_else(|| ids.first().copied())
    }

    pub fn workspace_pane_ids(&self, ws: usize) -> Vec<PaneId> {
        let mut ids = self
            .workspaces
            .get(&ws)
            .map(LayoutNode::pane_ids)
            .unwrap_or_default();
        if let Some(floating) = self.floating_panes.get(&ws) {
            for f in floating {
                if !ids.contains(&f.id) {
                    ids.push(f.id);
                }
            }
        }
        ids
    }

    /// spawns a new pane process but does not attach it to a layout tree.
    fn spawn_pane(
        &mut self,
        cols: u16,
        rows: u16,
        scrollback: usize,
        shell: &str,
        cwd: Option<&PathBuf>,
    ) -> Result<PaneId, String> {
        // reuse lowest available pane id so repeatedly closing and adding panes doesn't balloon ids
        let mut id_candidate = 1;
        while self.panes.contains_key(&PaneId(id_candidate)) {
            id_candidate += 1;
        }
        let id = PaneId(id_candidate);

        let (pane, _, _) = Pane::spawn_with_spawn_config(crate::pane::emu::PaneSpawn {
            id,
            cols,
            rows,
            scrollback,
            shell,
            cwd,
            session_name: &self.name,
            graphics: crate::util::outer_term::GraphicsSpawn {
                term: self.outer_term.clone(),
                graphics: self.outer_graphics,
            },
        })?;
        self.panes.insert(id, pane);
        Ok(id)
    }

    /// spawns a new pane in this session, splitting the focused pane.
    ///
    /// when the active workspace is empty the pane simply becomes its root.
    pub fn new_pane(
        &mut self,
        cols: u16,
        rows: u16,
        scrollback: usize,
        shell: &str,
        cwd: Option<&PathBuf>,
        axis: Option<SplitAxis>,
    ) -> Result<PaneId, String> {
        let target = self.focused_pane;
        let id = self.spawn_pane(cols, rows, scrollback, shell, cwd)?;
        self.insert_pane_into_tree(self.active_workspace, id, target, axis);
        self.focused_pane = Some(id);
        self.zoomed = None;
        self.focus_clock.set_focus(id);
        Ok(id)
    }

    fn insert_pane_into_tree(
        &mut self,
        ws: usize,
        id: PaneId,
        target: Option<PaneId>,
        axis: Option<SplitAxis>,
    ) {
        // axis::auto resolves from the target surface's aspect; no container is
        // available here, so assume a wide terminal-shaped area and split right.
        let split_axis = axis.unwrap_or_else(|| choose_aspect_split(Rect::new(0, 0, 120, 40)));

        match self.workspaces.remove(&ws) {
            Some(tree) => {
                let target = target
                    .filter(|t| tree.contains(*t))
                    .or_else(|| tree.pane_ids().first().copied());
                let new_tree = match target {
                    Some(t) => tree
                        .split_pane_directed(
                            t,
                            id,
                            split_axis,
                            0.5,
                            crate::layout::SplitDirection::Second,
                        )
                        .unwrap_or_else(|old| old),
                    None => LayoutNode::leaf(id),
                };
                self.workspaces.insert(ws, new_tree);
            }
            None => {
                self.workspaces.insert(ws, LayoutNode::leaf(id));
            }
        }
    }

    /// inserts `id` into the tree at roughly the same leaf order it occupied
    /// before it was floated, preserving any tiled changes made since then.
    fn insert_pane_at_order(
        &mut self,
        ws: usize,
        id: PaneId,
        order: usize,
        container: Rect,
        axis: Option<SplitAxis>,
    ) {
        let split_axis = axis.unwrap_or_else(|| choose_aspect_split(container));
        match self.workspaces.remove(&ws) {
            Some(tree) => {
                let ids = tree.pane_ids();
                let new_tree = if order == 0 {
                    ids.first().map(|first| {
                        tree.split_pane_directed(
                            *first,
                            id,
                            split_axis,
                            0.5,
                            crate::layout::SplitDirection::First,
                        )
                        .unwrap_or_else(|old| old)
                    })
                } else {
                    ids.get(order.saturating_sub(1).min(ids.len().saturating_sub(1)))
                        .map(|target| {
                            tree.split_pane_directed(
                                *target,
                                id,
                                split_axis,
                                0.5,
                                crate::layout::SplitDirection::Second,
                            )
                            .unwrap_or_else(|old| old)
                        })
                };
                self.workspaces
                    .insert(ws, new_tree.unwrap_or_else(|| LayoutNode::leaf(id)));
            }
            None => {
                self.workspaces.insert(ws, LayoutNode::leaf(id));
            }
        }
    }

    /// resolves pane rectangles for the active workspace (tiled plus floating).
    pub fn layout(&self, area: Rect, gap_inner: u16, gap_outer: u16) -> Vec<(PaneId, Rect)> {
        let mut res = if let Some(z) = self.zoomed {
            vec![(z, area)]
        } else if self.monocle {
            match self.focused_pane {
                Some(id) if self.panes.contains_key(&id) => vec![(id, area)],
                _ => Vec::new(),
            }
        } else {
            match self.active_tree() {
                Some(tree) => resolve(tree, area, gap_inner, gap_outer),
                None => Vec::new(),
            }
        };

        let fullscreened = if let Some(z) = self.zoomed {
            Some(z)
        } else if self.monocle {
            self.focused_pane.filter(|id| self.panes.contains_key(id))
        } else {
            None
        };

        if let Some(floating) = self.floating_panes.get(&self.active_workspace) {
            for f in floating {
                // a fullscreened float (zoom/monocle of the float itself) renders
                // only as fullscreen, not also at its float rect.
                if Some(f.id) == fullscreened {
                    continue;
                }
                // keep floating rects drawable inside the (possibly shrunk) content area.
                let mut f = f.clone();
                f.clamp(area);
                res.push((f.id, f.rect));
            }
        }

        res
    }

    /// checks if a pane is currently in floating mode in the active workspace.
    pub fn is_pane_floating(&self, id: PaneId) -> bool {
        self.floating_panes
            .get(&self.active_workspace)
            .map(|list| list.iter().any(|f| f.id == id))
            .unwrap_or(false)
    }

    /// closes a specific pane by id, returning the new focused pane if any.
    pub fn close_pane(&mut self, id: PaneId) -> Option<PaneId> {
        if let Some(pane) = self.panes.remove(&id) {
            pane.kill_tree();
        }
        if self.zoomed == Some(id) {
            self.zoomed = None;
        }

        // locate the workspace that held the pane before removing anything.
        let floating_ws = self
            .floating_panes
            .iter()
            .find(|(_, list)| list.iter().any(|f| f.id == id))
            .map(|(ws, _)| *ws);
        let tiled_ws = self
            .workspaces
            .iter()
            .find(|(_, tree)| tree.contains(id))
            .map(|(ws, _)| *ws);

        for floating in self.floating_panes.values_mut() {
            floating.retain(|f| f.id != id);
        }
        self.floating_panes.retain(|_, list| !list.is_empty());

        let found_ws = floating_ws.or(tiled_ws);
        if let Some(ws) = tiled_ws
            && let Some(tree) = self.workspaces.remove(&ws)
        {
            match tree.remove_pane(id) {
                Ok(Some(new_tree)) => {
                    self.workspaces.insert(ws, new_tree);
                }
                Ok(None) => {}
                Err(tree) => {
                    self.workspaces.insert(ws, tree);
                }
            }
        }

        self.focus_clock.remove(id);

        // unified focus fallback across tiled and floating panes of the originating workspace.
        if self.focused_pane == Some(id) {
            let ws_for_focus = found_ws.unwrap_or(self.active_workspace);
            let ids = self.workspace_pane_ids(ws_for_focus);
            self.focused_pane = self
                .focus_clock
                .most_recent(&ids)
                .or_else(|| ids.first().copied());
        }

        if let Some(f) = self.focused_pane {
            self.focus_clock.set_focus(f);
        }
        if self.scratch_pane == Some(id) {
            self.scratch_pane = None;
            self.scratch_hidden = None;
        }
        self.focused_pane
    }

    /// sets focus to the given pane if it exists in the active workspace.
    pub fn focus_pane(&mut self, id: PaneId) -> bool {
        if self.workspace_pane_ids(self.active_workspace).contains(&id) {
            if self.zoomed != Some(id) {
                self.zoomed = None;
            }
            // raise a floating pane above the others (vec order is z order).
            if let Some(list) = self.floating_panes.get_mut(&self.active_workspace)
                && let Some(pos) = list.iter().position(|f| f.id == id)
            {
                let f = list.remove(pos);
                list.push(f);
                for (i, fl) in list.iter_mut().enumerate() {
                    fl.z_index = i as u32 + 1;
                }
            }
            self.focused_pane = Some(id);
            self.focus_clock.set_focus(id);
            true
        } else {
            false
        }
    }

    /// moves focus spatially using the resolved pane rectangles.
    pub fn focus_neighbor(&mut self, dir: Direction, rects: &[(PaneId, Rect)]) -> bool {
        let current = self.focused_pane.or_else(|| {
            let ids = self.workspace_pane_ids(self.active_workspace);
            self.focus_clock
                .most_recent(&ids)
                .or_else(|| ids.first().copied())
        });
        let Some(current) = current else {
            return false;
        };
        if let Some(next) = self.focus_clock.spatial_neighbor(current, dir, rects) {
            self.focused_pane = Some(next);
            self.focus_clock.set_focus(next);
            true
        } else {
            false
        }
    }

    /// swaps the focused pane with its spatial neighbor.
    pub fn swap_neighbor(&mut self, dir: Direction, rects: &[(PaneId, Rect)]) -> bool {
        let Some(current) = self.focused_pane else {
            return false;
        };
        let Some(other) = self.focus_clock.spatial_neighbor(current, dir, rects) else {
            return false;
        };
        if let Some(tree) = self.workspaces.get_mut(&self.active_workspace) {
            return tree.swap_panes(current, other);
        }
        false
    }

    /// swaps two specific panes by ID (for title bar drag-to-swap).
    pub fn swap_panes(&mut self, a: PaneId, b: PaneId) -> bool {
        if let Some(tree) = self.workspaces.get_mut(&self.active_workspace) {
            return tree.swap_panes(a, b);
        }
        false
    }

    /// resizes along the axis of the given direction, moving the shared boundary
    /// in that direction regardless of which side of the split holds focus.
    pub fn resize_focused(&mut self, dir: Direction, delta: f32) -> bool {
        let Some(current) = self.focused_pane else {
            return false;
        };
        let (axis, signed) = match dir {
            Direction::Left => (SplitAxis::Horizontal, -delta),
            Direction::Right => (SplitAxis::Horizontal, delta),
            Direction::Up => (SplitAxis::Vertical, -delta),
            Direction::Down => (SplitAxis::Vertical, delta),
        };
        if let Some(tree) = self.workspaces.get_mut(&self.active_workspace) {
            return tree.resize_pane(current, axis, signed);
        }
        false
    }

    /// rotates the split containing the focused pane.
    pub fn rotate_focused(&mut self) -> bool {
        let Some(current) = self.focused_pane else {
            return false;
        };
        if let Some(tree) = self.workspaces.get_mut(&self.active_workspace) {
            return tree.rotate_split(current);
        }
        false
    }

    /// equalizes all split ratios in the active workspace.
    pub fn equalize(&mut self) {
        if let Some(tree) = self.workspaces.get_mut(&self.active_workspace) {
            tree.equalize();
        }
    }

    /// toggles zoom (monocle) for the focused pane.
    pub fn toggle_zoom(&mut self) {
        if self.zoomed.is_some() {
            self.zoomed = None;
        } else {
            self.zoomed = self.focused_pane;
        }
    }

    /// switches to the given workspace, creating it if necessary.
    pub fn ensure_workspace(&mut self, ws: usize) {
        self.active_workspace = ws;
        self.zoomed = None;
        let ids = self.workspace_pane_ids(ws);
        self.focused_pane = self
            .focus_clock
            .most_recent(&ids)
            .or_else(|| ids.first().copied());
    }

    /// moves the focused pane into another workspace.
    pub fn move_focused_to_workspace(&mut self, ws: usize) -> bool {
        let Some(id) = self.focused_pane else {
            return false;
        };
        if ws == self.active_workspace {
            return false;
        }

        let from = self.active_workspace;
        let target_focus = self
            .workspaces
            .get(&ws)
            .and_then(|t| self.focus_clock.most_recent(&t.pane_ids()))
            .or_else(|| {
                self.workspaces
                    .get(&ws)
                    .and_then(|t| t.pane_ids().first().copied())
            })
            .or_else(|| {
                self.floating_panes
                    .get(&ws)
                    .and_then(|list| list.last().map(|f| f.id))
            });

        // floating pane: carry the float entry into the target workspace untouched.
        if let Some(list) = self.floating_panes.get_mut(&from)
            && let Some(pos) = list.iter().position(|f| f.id == id)
        {
            let float = list.remove(pos);
            if list.is_empty() {
                self.floating_panes.remove(&from);
            }
            self.floating_panes.entry(ws).or_default().push(float);
            if self.zoomed == Some(id) {
                self.zoomed = None;
            }
            let ids = self.workspace_pane_ids(from);
            if self.focused_pane == Some(id) {
                self.focused_pane = self
                    .focus_clock
                    .most_recent(&ids)
                    .or_else(|| ids.first().copied());
            }
            self.active_workspace = ws;
            self.focused_pane = Some(id);
            self.focus_clock.set_focus(id);
            self.zoomed = None;
            return true;
        }

        if let Some(tree) = self.workspaces.remove(&from) {
            match tree.remove_pane(id) {
                Ok(Some(new_tree)) => {
                    let ids = new_tree.pane_ids();
                    self.workspaces.insert(from, new_tree);
                    if from == self.active_workspace {
                        self.focused_pane = self
                            .focus_clock
                            .most_recent(&ids)
                            .or_else(|| ids.first().copied());
                    }
                }
                Ok(None) => {
                    self.focused_pane = None;
                }
                Err(tree) => {
                    self.workspaces.insert(from, tree);
                }
            }
        }

        self.insert_pane_into_tree(ws, id, target_focus, None);
        self.active_workspace = ws;
        self.focused_pane = Some(id);
        self.focus_clock.set_focus(id);
        self.zoomed = None;
        true
    }

    /// creates a new workspace with the next available id and makes it active.
    pub fn new_workspace(&mut self) -> usize {
        let mut next_id = 1;
        while self.workspaces.contains_key(&next_id)
            || self.workspace_names.contains_key(&next_id)
            || self.floating_panes.contains_key(&next_id)
        {
            next_id += 1;
        }
        self.active_workspace = next_id;
        self.zoomed = None;
        self.focused_pane = None;
        next_id
    }

    /// closes a workspace, moving active workspace to previous one (not workspace 1).
    pub fn close_workspace(&mut self, ws: usize) -> Option<usize> {
        // find all pane ids in this workspace and remove them
        if let Some(tree) = self.workspaces.remove(&ws) {
            let pane_ids = tree.pane_ids();
            for pid in pane_ids {
                if let Some(pane) = self.panes.remove(&pid) {
                    pane.kill_tree();
                }
                self.focus_clock.remove(pid);
                if self.zoomed == Some(pid) {
                    self.zoomed = None;
                }
            }
        }
        if let Some(floating) = self.floating_panes.remove(&ws) {
            for f in floating {
                if let Some(pane) = self.panes.remove(&f.id) {
                    pane.kill_tree();
                }
                self.focus_clock.remove(f.id);
                if self.zoomed == Some(f.id) {
                    self.zoomed = None;
                }
            }
        }
        self.workspace_names.remove(&ws);

        // if closing active workspace, return the workspace to switch to.
        // `workspace_ids()` always pushes `active_workspace` (the closed one), so
        // this must use the raw union instead.
        if self.active_workspace == ws {
            let remaining: Vec<usize> = self.workspace_id_set().into_iter().collect();
            // switch to the last remaining id below the closed one; if there is
            // none, the first remaining workspace; if none remain, stay put.
            let next = remaining
                .iter()
                .rfind(|&&id| id < ws)
                .copied()
                .or_else(|| remaining.first().copied())
                .unwrap_or(ws);
            self.ensure_workspace(next);
            Some(next)
        } else {
            None
        }
    }

    /// captures a full snapshot of the workspaces, layout trees, floating panes,
    /// names, zoom and focus for persistence to disk.
    pub fn snapshot(&self) -> crate::server::persist::SessionSnapshot {
        use crate::server::persist::{
            FloatingSnapshot, RectSnapshot, SNAPSHOT_VERSION, SessionSnapshot, WorkspaceSnapshot,
        };

        let mut ids = self.workspace_id_set();
        ids.insert(self.active_workspace);

        let workspaces = ids
            .into_iter()
            .map(|ws| WorkspaceSnapshot {
                workspace: ws,
                name: self
                    .workspace_names
                    .get(&ws)
                    .cloned()
                    .filter(|n| !n.is_empty()),
                tree: self.workspaces.get(&ws).cloned(),
                floating: self
                    .floating_panes
                    .get(&ws)
                    .map(|list| {
                        list.iter()
                            .map(|f| FloatingSnapshot {
                                id: f.id.0,
                                rect: RectSnapshot::from_rect(f.rect),
                                z_index: f.z_index,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();

        SessionSnapshot {
            version: SNAPSHOT_VERSION,
            name: self.name.clone(),
            active_workspace: self.active_workspace,
            focused: self.focused_pane.map(|p| p.0),
            zoomed: self.zoomed.map(|p| p.0),
            workspaces,
        }
    }

    /// rebuilds a session from a disk snapshot, spawning fresh shells for every
    /// saved pane. returns the number of panes restored.
    pub fn restore_snapshot(
        &mut self,
        snap: &crate::server::persist::SessionSnapshot,
        cols: u16,
        rows: u16,
        scrollback: usize,
        shell: &str,
    ) -> usize {
        self.restore_snapshot_with_limits(
            snap,
            cols,
            rows,
            scrollback,
            shell,
            RestoreLimits {
                max_panes: 1000,
                max_workspaces: 100,
            },
        )
    }

    /// restores a snapshot while enforcing the configured pane and workspace bounds.
    pub fn restore_snapshot_with_limits(
        &mut self,
        snap: &crate::server::persist::SessionSnapshot,
        cols: u16,
        rows: u16,
        scrollback: usize,
        shell: &str,
        limits: RestoreLimits,
    ) -> usize {
        use crate::server::persist::SNAPSHOT_VERSION;

        if snap.version != SNAPSHOT_VERSION {
            return 0;
        }

        let mut id_map: HashMap<u32, PaneId> = HashMap::new();
        let mut restored = 0usize;

        let max_panes = limits.max_panes.max(1);
        let max_workspaces = limits.max_workspaces.max(1);
        let mut seen_workspaces = std::collections::HashSet::new();
        let mut placed_panes = std::collections::HashSet::new();
        for ws_snap in &snap.workspaces {
            if seen_workspaces.contains(&ws_snap.workspace) {
                continue;
            }
            if seen_workspaces.len() >= max_workspaces {
                break;
            }
            seen_workspaces.insert(ws_snap.workspace);
            if let Some(tree) = &ws_snap.tree {
                for pid in tree.pane_ids() {
                    if id_map.contains_key(&pid.0) {
                        continue;
                    }
                    if self.panes.len() >= max_panes {
                        break;
                    }
                    match self.spawn_pane(cols, rows, scrollback, shell, None) {
                        Ok(new_id) => {
                            id_map.insert(pid.0, new_id);
                            restored += 1;
                        }
                        Err(e) => {
                            tracing::warn!("restore: failed to spawn pane: {e}");
                        }
                    }
                }
                if let Some(rebuilt) = remap_node(tree, &id_map, &mut placed_panes) {
                    self.workspaces.insert(ws_snap.workspace, rebuilt);
                }
            }
            for fs in &ws_snap.floating {
                if placed_panes.contains(&fs.id) {
                    continue;
                }
                let new_id = match id_map.get(&fs.id) {
                    Some(id) => Some(*id),
                    None if self.panes.len() >= max_panes => None,
                    None => match self.spawn_pane(cols, rows, scrollback, shell, None) {
                        Ok(id) => {
                            id_map.insert(fs.id, id);
                            restored += 1;
                            Some(id)
                        }
                        Err(e) => {
                            tracing::warn!("restore: failed to spawn floating pane: {e}");
                            None
                        }
                    },
                };
                if let Some(id) = new_id {
                    placed_panes.insert(fs.id);
                    self.floating_panes
                        .entry(ws_snap.workspace)
                        .or_default()
                        .push(FloatingPane::new(id, fs.rect.to_rect(), fs.z_index));
                }
            }
            if let Some(name) = &ws_snap.name {
                self.workspace_names.insert(ws_snap.workspace, name.clone());
            }
        }

        self.active_workspace = snap.active_workspace;
        self.focused_pane = snap.focused.and_then(|old| id_map.get(&old).copied());
        self.zoomed = snap.zoomed.and_then(|old| id_map.get(&old).copied());
        if let Some(f) = self.focused_pane {
            self.focus_clock.set_focus(f);
        }
        restored
    }

    /// toggles floating mode for the focused pane.
    pub fn toggle_float(&mut self, container: Rect) -> bool {
        let Some(focused) = self.focused_pane else {
            return false;
        };
        let ws = self.active_workspace;

        // floating state owns its geometry; drop any zoom on the pane.
        if self.zoomed == Some(focused) {
            self.zoomed = None;
        }

        // if it's already floating, unfloat it back into tiling tree
        if let Some(list) = self.floating_panes.get_mut(&ws)
            && let Some(pos) = list.iter().position(|f| f.id == focused)
        {
            let mut floating = list.remove(pos);
            if list.is_empty() {
                self.floating_panes.remove(&ws);
            }

            // if nothing tiled changed while it floated, restore the exact
            // pre-float tree so every pane returns to its original split.
            let unchanged = match floating.restore_after.as_ref() {
                Some(expected) => self.workspaces.get(&ws) == Some(expected),
                None => !self.workspaces.contains_key(&ws),
            };
            if unchanged && let Some(tree) = floating.restore_tree.take() {
                self.workspaces.insert(ws, tree);
            } else if let Some(order) = floating.restore_order {
                self.insert_pane_at_order(ws, focused, order, container, floating.restore_axis);
            } else {
                let target_pane = self
                    .workspaces
                    .get(&ws)
                    .and_then(|tree| tree.pane_ids().first().copied());
                self.insert_pane_into_tree(ws, focused, target_pane, None);
            }

            self.focus_clock.set_focus(focused);
            return true;
        }

        // otherwise, remove from tiling tree and make it floating
        let mut restore_tree = None;
        let mut restore_after = None;
        let mut restore_order = None;
        let mut restore_axis = None;
        if let Some(tree) = self.workspaces.remove(&ws) {
            restore_order = tree.pane_ids().iter().position(|&id| id == focused);
            restore_axis = tree.parent_split_axis(focused);
            restore_tree = Some(tree.clone());
            restore_after = tree.remove_pane(focused).ok().flatten();
            if let Some(new_tree) = restore_after.clone() {
                self.workspaces.insert(ws, new_tree);
            }
        }

        let width = ((u32::from(container.width) * 3 / 4) as u16)
            .max(20)
            .min(container.width.max(1));
        let height = ((u32::from(container.height) * 3 / 4) as u16)
            .max(8)
            .min(container.height.max(1));
        let x = container.x + (container.width.saturating_sub(width)) / 2;
        let y = container.y + (container.height.saturating_sub(height)) / 2;
        let rect = Rect::new(x, y, width, height);

        let mut float_pane = FloatingPane::new(focused, rect, 1);
        float_pane.restore_tree = restore_tree;
        float_pane.restore_after = restore_after;
        float_pane.restore_order = restore_order;
        float_pane.restore_axis = restore_axis;
        float_pane.clamp(container);

        self.floating_panes.entry(ws).or_default().push(float_pane);
        true
    }

    /// centers the currently focused floating pane.
    pub fn float_center(&mut self, container: Rect) -> bool {
        let Some(focused) = self.focused_pane else {
            return false;
        };
        let ws = self.active_workspace;
        if let Some(list) = self.floating_panes.get_mut(&ws)
            && let Some(f) = list.iter_mut().find(|f| f.id == focused)
        {
            f.center(container);
            return true;
        }
        false
    }

    /// finds and removes a floating pane entry in any workspace.
    fn take_float_everywhere(&mut self, id: PaneId) -> Option<FloatingPane> {
        for list in self.floating_panes.values_mut() {
            if let Some(pos) = list.iter().position(|f| f.id == id) {
                return Some(list.remove(pos));
            }
        }
        None
    }

    /// toggles the shared scratchpad pane: creates it on first call,
    /// then hides/shows it on subsequent calls.
    pub fn toggle_scratch(
        &mut self,
        container: Rect,
        scrollback: usize,
        shell: &str,
    ) -> Result<ScratchAction, String> {
        // drop stale state if the scratch pane already exited.
        if let Some(id) = self.scratch_pane
            && !self.panes.contains_key(&id)
        {
            self.scratch_pane = None;
            self.scratch_hidden = None;
        }

        if let Some(id) = self.scratch_pane {
            // show if hidden.
            if let Some(mut float) = self.scratch_hidden.take() {
                float.clamp(container);
                let ws = self.active_workspace;
                self.floating_panes.entry(ws).or_default().push(float);
                self.focused_pane = Some(id);
                self.focus_clock.set_focus(id);
                return Ok(ScratchAction::Shown);
            }
            // hide otherwise.
            let float = self.take_float_everywhere(id);
            self.floating_panes.retain(|_, l| !l.is_empty());
            if self.zoomed == Some(id) {
                self.zoomed = None;
            }
            self.scratch_hidden = float;
            if self.focused_pane == Some(id) {
                let ws = self.active_workspace;
                let ids = self.workspace_pane_ids(ws);
                self.focused_pane = self
                    .focus_clock
                    .most_recent(&ids)
                    .or_else(|| ids.first().copied());
                if let Some(f) = self.focused_pane {
                    self.focus_clock.set_focus(f);
                }
            }
            return Ok(ScratchAction::Hidden);
        }

        // create on first use.
        let width = ((u32::from(container.width) * 3 / 5) as u16)
            .max(40)
            .min(container.width.max(1));
        let height = ((u32::from(container.height) * 3 / 5) as u16)
            .max(10)
            .min(container.height.max(1));
        let cols = width;
        let rows = height;
        let id = self.spawn_pane(cols, rows, scrollback, shell, None)?;

        let rect = Rect::new(
            container.x + (container.width.saturating_sub(width)) / 2,
            container.y + (container.height.saturating_sub(height)) / 2,
            width,
            height,
        );
        let ws = self.active_workspace;
        let list = self.floating_panes.entry(ws).or_default();
        let z = list.len() as u32 + 1;
        list.push(FloatingPane::new(id, rect, z));
        self.scratch_pane = Some(id);
        self.zoomed = None;
        self.focused_pane = Some(id);
        self.focus_clock.set_focus(id);
        Ok(ScratchAction::Created)
    }
}
