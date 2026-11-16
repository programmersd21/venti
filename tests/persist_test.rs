use ratatui::layout::Rect;
use std::path::PathBuf;
use venti::layout::{FloatingPane, PaneId};
use venti::server::persist::{self, SessionSnapshot};
use venti::server::session::Session;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("venti-persist-test-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn open_pane(session: &mut Session) -> PaneId {
    session
        .new_pane(40, 10, 100, "/bin/sh", None, None)
        .expect("spawn pane")
}

#[test]
fn snapshot_roundtrip_serde() {
    let mut session = Session::new("roundtrip-test");
    let pane = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);
    session.focused_pane = Some(pane);
    session.toggle_float(container);
    session.set_workspace_names(&["ws1".to_string()]);

    let snap = session.snapshot();
    let toml = toml::to_string(&snap).expect("serialize snapshot");
    let parsed: SessionSnapshot = toml::from_str(&toml).expect("deserialize snapshot");

    assert_eq!(parsed.version, persist::SNAPSHOT_VERSION);
    assert_eq!(parsed.name, "roundtrip-test");
    assert_eq!(parsed.workspaces.len(), 1);
    let ws = &parsed.workspaces[0];
    assert_eq!(ws.name.as_deref(), Some("ws1"));
    assert_eq!(ws.floating.len(), 1);
    assert_eq!(ws.floating[0].id, pane.0);
    assert!(ws.tree.is_none(), "pane floated, no tiled tree remains");
}

#[test]
fn restore_rebuilds_layout_with_fresh_panes() {
    let mut session = Session::new("restore-test");
    let _a = open_pane(&mut session);
    let b = open_pane(&mut session);
    let container = Rect::new(0, 0, 120, 40);

    // float b above a
    session.focused_pane = Some(b);
    session.toggle_float(container);
    session.set_workspace_names(&["ws1".to_string()]);

    let snap = session.snapshot();
    let toml = toml::to_string(&snap).expect("serialize");
    let parsed: SessionSnapshot = toml::from_str(&toml).expect("deserialize");

    let mut restored = Session::new("restore-test");
    let count = restored.restore_snapshot(&parsed, 80, 24, 100, "/bin/sh");
    assert_eq!(count, 2);

    // one tree with two leaves was rebuilt; b is floating again
    assert_eq!(restored.workspace_pane_ids(1).len(), 2);
    let floats = restored.floating_panes.get(&1).expect("float restored");
    assert_eq!(floats.len(), 1);
    assert!(restored.workspaces.contains_key(&1));
    // names restored
    assert_eq!(restored.workspace_name(1), "ws1");
    // focus restored to the previously focused pane (some valid pane)
    assert!(restored.focused_pane.is_some());
}

#[test]
fn save_and_load_session_file_roundtrip() {
    let dir = tmp_dir("io");
    let mut session = Session::new("file-test");
    open_pane(&mut session);
    let snap = session.snapshot();

    persist::save_session(&dir, &snap).expect("save snapshot");
    assert!(dir.join("file-test.toml").exists());

    let loaded = persist::load_session(&dir, "file-test").expect("load snapshot");
    assert_eq!(loaded.name, "file-test");
    assert_eq!(loaded.version, persist::SNAPSHOT_VERSION);

    // permissions: file should be 0600
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir.join("file-test.toml"))
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_session_rejects_legacy_version() {
    let dir = tmp_dir("legacy");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("legacy.toml"),
        "name = \"legacy\"\nactive_workspace = 1\npanes_count = 3\n",
    )
    .expect("write legacy");

    // legacy snapshot loads (serde defaults) but carries no version/workspaces,
    // so restore_snapshot refuses it and the caller spawns a fresh pane instead
    let loaded = persist::load_session(&dir, "legacy").expect("parses");
    assert_ne!(loaded.version, persist::SNAPSHOT_VERSION);
    assert!(loaded.workspaces.is_empty());

    let mut session = Session::new("legacy");
    let restored = session.restore_snapshot(&loaded, 80, 24, 100, "/bin/sh");
    assert_eq!(restored, 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_session_rejects_oversized_snapshot() {
    let dir = tmp_dir("oversized");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("large.toml");
    let mut data = b"version = 2\nname = \"large\"\n".to_vec();
    data.resize(8 * 1024 * 1024 + 1, b' ');
    std::fs::write(&path, data).expect("write oversized snapshot");

    assert!(persist::load_session(&dir, "large").is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restore_rebuilds_multi_pane_tree() {
    use venti::layout::{LayoutNode, PaneId, SplitAxis};
    let mut session = Session::new("partial");
    let tree = LayoutNode::split(
        SplitAxis::Horizontal,
        0.5,
        LayoutNode::leaf(PaneId(1)),
        LayoutNode::leaf(PaneId(999)), // this pid will map fine on fresh restore
    );
    let snap = persist::SessionSnapshot {
        version: persist::SNAPSHOT_VERSION,
        name: "partial".into(),
        active_workspace: 1,
        focused: Some(1),
        zoomed: None,
        workspaces: vec![persist::WorkspaceSnapshot {
            workspace: 1,
            name: None,
            tree: Some(tree),
            floating: vec![],
        }],
    };
    let restored = session.restore_snapshot(&snap, 80, 24, 100, "/bin/sh");
    assert_eq!(restored, 2);
    let tree = session.active_tree().expect("tree present");
    assert_eq!(tree.pane_ids().len(), 2);
}

#[test]
fn restore_respects_pane_and_workspace_limits() {
    use venti::layout::{LayoutNode, PaneId, SplitAxis};

    let tree = LayoutNode::split(
        SplitAxis::Horizontal,
        0.5,
        LayoutNode::leaf(PaneId(1)),
        LayoutNode::split(
            SplitAxis::Vertical,
            0.5,
            LayoutNode::leaf(PaneId(2)),
            LayoutNode::leaf(PaneId(3)),
        ),
    );
    let snap = SessionSnapshot {
        version: persist::SNAPSHOT_VERSION,
        name: "bounded".into(),
        active_workspace: 1,
        focused: None,
        zoomed: None,
        workspaces: vec![
            persist::WorkspaceSnapshot {
                workspace: 1,
                name: None,
                tree: Some(tree),
                floating: vec![],
            },
            persist::WorkspaceSnapshot {
                workspace: 2,
                name: None,
                tree: Some(LayoutNode::leaf(PaneId(4))),
                floating: vec![],
            },
        ],
    };
    let mut session = Session::new("bounded");
    let restored = session.restore_snapshot_with_limits(
        &snap,
        80,
        24,
        100,
        "/bin/sh",
        venti::server::session::RestoreLimits {
            max_panes: 1,
            max_workspaces: 1,
        },
    );

    assert_eq!(restored, 1);
    assert_eq!(session.panes.len(), 1);
    assert_eq!(session.workspace_pane_ids(1).len(), 1);
    assert!(!session.workspaces.contains_key(&2));
}

#[test]
fn restore_prunes_duplicate_pane_and_workspace_entries() {
    use venti::layout::{LayoutNode, PaneId, SplitAxis};

    let duplicate_tree = LayoutNode::split(
        SplitAxis::Horizontal,
        0.5,
        LayoutNode::leaf(PaneId(7)),
        LayoutNode::leaf(PaneId(7)),
    );
    let workspace = persist::WorkspaceSnapshot {
        workspace: 1,
        name: Some("first".into()),
        tree: Some(duplicate_tree),
        floating: vec![persist::FloatingSnapshot {
            id: 7,
            rect: persist::RectSnapshot {
                x: 0,
                y: 0,
                width: 10,
                height: 5,
            },
            z_index: 1,
        }],
    };
    let snap = SessionSnapshot {
        version: persist::SNAPSHOT_VERSION,
        name: "duplicates".into(),
        active_workspace: 1,
        focused: Some(7),
        zoomed: None,
        workspaces: vec![
            workspace.clone(),
            persist::WorkspaceSnapshot {
                workspace: 1,
                name: Some("second".into()),
                tree: Some(LayoutNode::leaf(PaneId(8))),
                floating: vec![],
            },
            persist::WorkspaceSnapshot {
                workspace: 2,
                name: None,
                tree: Some(LayoutNode::leaf(PaneId(7))),
                floating: vec![],
            },
        ],
    };

    let mut session = Session::new("duplicates");
    let restored = session.restore_snapshot_with_limits(
        &snap,
        80,
        24,
        100,
        "/bin/sh",
        venti::server::session::RestoreLimits {
            max_panes: 10,
            max_workspaces: 10,
        },
    );

    assert_eq!(restored, 1);
    assert_eq!(session.panes.len(), 1);
    assert_eq!(
        session.workspaces[&1].pane_ids(),
        vec![*session.panes.keys().next().unwrap()]
    );
    assert!(!session.workspaces.contains_key(&2));
    assert!(session.floating_panes.is_empty());
    assert_eq!(session.workspace_name(1), "first");
}

#[test]
fn layout_node_serialization_roundtrip() {
    use venti::layout::{LayoutNode, PaneId, SplitAxis};
    let tree = LayoutNode::split(
        SplitAxis::Horizontal,
        0.6,
        LayoutNode::leaf(PaneId(1)),
        LayoutNode::split(
            SplitAxis::Vertical,
            0.4,
            LayoutNode::leaf(PaneId(2)),
            LayoutNode::leaf(PaneId(3)),
        ),
    );
    let toml = toml::to_string(&tree).expect("serialize tree");
    let back: LayoutNode = toml::from_str(&toml).expect("deserialize tree");
    assert_eq!(back.pane_ids(), tree.pane_ids());
}

#[test]
fn floating_pane_survives_snapshot_with_fields() {
    let fp = FloatingPane::new(PaneId(7), Rect::new(3, 4, 50, 12), 3);
    let snap = persist::FloatingSnapshot {
        id: fp.id.0,
        rect: persist::RectSnapshot::from_rect(fp.rect),
        z_index: fp.z_index,
    };
    let toml = toml::to_string(&snap).expect("serialize");
    let back: persist::FloatingSnapshot = toml::from_str(&toml).expect("deserialize");
    assert_eq!(back.id, 7);
    assert_eq!(back.rect.x, 3);
    assert_eq!(back.rect.width, 50);
    assert_eq!(back.z_index, 3);
}
