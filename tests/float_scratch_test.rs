use ratatui::layout::Rect;
use venti::layout::PaneId;
use venti::server::session::{ScratchAction, Session};

fn make_session() -> Session {
    Session::new("test")
}

fn open_pane(session: &mut Session) -> PaneId {
    // spawn and attach a real shell pane
    session
        .new_pane(40, 10, 100, "/bin/sh", None, None)
        .expect("new pane")
}

#[test]
fn layout_includes_floating_panes() {
    let mut session = make_session();
    let id = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    assert!(session.toggle_float(container));
    let layout = session.layout(container, 0, 0);
    assert_eq!(layout.len(), 1);
    assert_eq!(layout[0].0, id);
    assert!(session.floating_panes.contains_key(&1));
}

#[test]
fn layout_keeps_floating_panes_when_zoomed() {
    let mut session = make_session();
    let tiled_a = open_pane(&mut session);
    let tiled_b = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    // float one of them, zoom the other
    session.focused_pane = Some(tiled_a);
    assert!(session.toggle_float(container));
    session.zoomed = Some(tiled_b);

    let layout = session.layout(container, 0, 0);
    let ids: Vec<PaneId> = layout.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&tiled_b));
    assert!(ids.contains(&tiled_a));
}

#[test]
fn close_focused_float_falls_back_to_tiled_focus() {
    let mut session = make_session();
    let tiled = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    let float = open_pane(&mut session);
    session.focused_pane = Some(float);
    assert!(session.toggle_float(container));

    // close the floating pane; head should fall back to the tiled pane
    session.close_pane(float);
    assert_eq!(session.focused_pane, Some(tiled));
}

#[test]
fn toggle_float_twice_returns_to_tiling() {
    let mut session = make_session();
    let pane = open_pane(&mut session);
    let keep = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    session.focused_pane = Some(pane);
    assert!(session.toggle_float(container));
    assert!(session.floating_panes.contains_key(&1));

    session.focused_pane = Some(pane);
    assert!(session.toggle_float(container));
    assert!(!session.floating_panes.contains_key(&1));
    // the pane is back in the tree
    let tree = session.active_tree().expect("tree present");
    assert!(tree.contains(pane));
    assert!(tree.contains(keep));
}

#[test]
fn move_float_to_workspace_carries_float() {
    let mut session = make_session();
    let pane = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    session.focused_pane = Some(pane);
    assert!(session.toggle_float(container));

    session.focused_pane = Some(pane);
    assert!(session.move_focused_to_workspace(2));
    // float entry moved, not duplicated into the tree
    assert!(
        session
            .floating_panes
            .get(&2)
            .is_some_and(|l| l.iter().any(|f| f.id == pane))
    );
    assert!(!session.workspaces.get(&2).is_some_and(|t| t.contains(pane)));
    // source workspace has no stale float entry
    assert!(
        !session
            .floating_panes
            .get(&1)
            .is_some_and(|l| l.iter().any(|f| f.id == pane))
    );
}

#[test]
fn new_workspace_skips_float_only_ids() {
    let mut session = make_session();
    let pane = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    // float-only workspace 2 (pane floated, no tiled tree)
    session.focused_pane = Some(pane);
    assert!(session.toggle_float(container));
    session.active_workspace = 2;
    session.workspaces.remove(&1); // pane moved: ws 2 holds only the float
    // actually the float stayed in ws 1; simulate ws being unknown to names/trees
    session.floating_panes.clear();
    session.floating_panes.insert(
        2,
        vec![venti::layout::FloatingPane::new(pane, container, 1)],
    );

    let next = session.new_workspace();
    assert_ne!(next, 2);
}

#[test]
fn floating_panes_are_clamped_inside_content() {
    let mut session = make_session();
    let pane = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    session.focused_pane = Some(pane);
    assert!(session.toggle_float(container));

    // shrink the content area; the float rect must clamp on layout
    let small = Rect::new(0, 0, 40, 12);
    let layout = session.layout(small, 0, 0);
    let (_, rect) = layout
        .iter()
        .find(|(id, _)| *id == pane)
        .expect("float present");
    assert!(rect.x + rect.width <= small.x + small.width);
    assert!(rect.y + rect.height <= small.y + small.height);
}

#[test]
fn toggle_scratch_lifecycle() {
    let mut session = make_session();
    let container = Rect::new(0, 0, 120, 40);

    let first = session
        .toggle_scratch(container, 100, "/bin/sh")
        .expect("scratch created");
    assert_eq!(first, ScratchAction::Created);
    let scratch_id = session.scratch_pane.expect("scratch pane set");

    let hidden = session
        .toggle_scratch(container, 100, "/bin/sh")
        .expect("scratch hidden");
    assert_eq!(hidden, ScratchAction::Hidden);
    // hidden scratch is not in any layout
    let layout = session.layout(container, 0, 0);
    assert!(layout.iter().all(|(id, _)| *id != scratch_id));

    let shown = session
        .toggle_scratch(container, 100, "/bin/sh")
        .expect("scratch shown");
    assert_eq!(shown, ScratchAction::Shown);
    let layout = session.layout(container, 0, 0);
    assert!(layout.iter().any(|(id, _)| *id == scratch_id));
}

#[test]
fn focus_pane_clears_zoom_for_other_panes() {
    let mut session = make_session();
    let a = open_pane(&mut session);
    let b = open_pane(&mut session);

    session.focused_pane = Some(a);
    session.toggle_zoom();
    assert_eq!(session.zoomed, Some(a));

    assert!(session.focus_pane(b));
    assert_eq!(session.zoomed, None);

    assert!(session.focus_pane(a));
    session.toggle_zoom();
    assert_eq!(session.zoomed, Some(a));
    // focusing the zoomed pane keeps zoom
    assert!(session.focus_pane(a));
    assert_eq!(session.zoomed, Some(a));
}

#[test]
fn zoomed_float_renders_once_fullscreen() {
    let mut session = make_session();
    let pane = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    session.focused_pane = Some(pane);
    assert!(session.toggle_float(container));

    // zoom the floating pane itself
    session.zoomed = Some(pane);
    let layout = session.layout(container, 0, 0);
    let matches = layout.iter().filter(|(id, _)| *id == pane).count();
    assert_eq!(matches, 1);
    assert_eq!(layout[0].1, container);
}
