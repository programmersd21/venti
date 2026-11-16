use ratatui::layout::Rect;
use venti::server::session::Session;

fn open_pane(session: &mut Session) -> venti::layout::PaneId {
    session
        .new_pane(40, 10, 100, "/bin/sh", None, None)
        .expect("spawn pane")
}

#[test]
fn monocle_resolves_to_focused_pane_only() {
    let mut session = Session::new("monocle-test");
    let a = open_pane(&mut session);
    let b = open_pane(&mut session);
    let c = open_pane(&mut session);
    let area = Rect::new(0, 0, 120, 40);

    session.monocle = true;
    session.focused_pane = Some(b);

    let layout = session.layout(area, 0, 0);
    assert_eq!(layout.len(), 1);
    assert_eq!(layout[0].0, b);
    assert_eq!(layout[0].1, area);

    // tiled tree is untouched behind the monocle
    assert_eq!(session.workspace_pane_ids(1).len(), 3);
    assert!(session.active_tree().expect("tree").contains(a));
    assert!(session.active_tree().expect("tree").contains(c));

    // focus follows in monocle mode
    session.focus_pane(a);
    let layout = session.layout(area, 0, 0);
    assert_eq!(layout[0].0, a);
}

#[test]
fn monocle_shows_float_on_top_and_fullscreens_focused_float() {
    let mut session = Session::new("monocle-float-test");
    let a = open_pane(&mut session);
    let b = open_pane(&mut session);
    let area = Rect::new(0, 0, 120, 40);

    session.monocle = true;

    // float a; focus b: monocle shows b fullscreen, a renders as float on top
    session.focused_pane = Some(a);
    assert!(session.toggle_float(area));
    session.focused_pane = Some(b);
    let layout = session.layout(area, 0, 0);
    let ids: Vec<_> = layout.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&b));
    assert!(ids.contains(&a));
    assert_eq!(
        layout.iter().find(|(id, _)| *id == b).map(|(_, r)| *r),
        Some(area)
    );

    // focus the float itself: it fullscreens (zoom-like), not rendered twice
    session.focus_pane(a);
    let layout = session.layout(area, 0, 0);
    let matches_a = layout.iter().filter(|(id, _)| *id == a).count();
    assert_eq!(matches_a, 1);
    assert_eq!(layout.len(), 1);
}
