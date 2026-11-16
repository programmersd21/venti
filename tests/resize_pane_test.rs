use venti::layout::{LayoutNode, PaneId, SplitAxis};

fn tree() -> LayoutNode {
    // Split(H, Leaf(1), Split(V, Leaf(2), Leaf(3)))
    LayoutNode::split(
        SplitAxis::Horizontal,
        0.5,
        LayoutNode::leaf(PaneId(1)),
        LayoutNode::split(
            SplitAxis::Vertical,
            0.5,
            LayoutNode::leaf(PaneId(2)),
            LayoutNode::leaf(PaneId(3)),
        ),
    )
}

#[test]
fn resize_prefers_deeper_split_on_same_axis() {
    let mut t = LayoutNode::split(
        SplitAxis::Horizontal,
        0.5,
        LayoutNode::leaf(PaneId(1)),
        LayoutNode::split(
            SplitAxis::Horizontal,
            0.5,
            LayoutNode::leaf(PaneId(2)),
            LayoutNode::leaf(PaneId(3)),
        ),
    );

    assert!(t.resize_pane(PaneId(3), SplitAxis::Horizontal, 0.1));
    let ids = t.pane_ids();
    assert_eq!(ids, vec![PaneId(1), PaneId(2), PaneId(3)]);
    // the deeper split was adjusted, not the root
    if let LayoutNode::Split {
        ratio: root_ratio,
        second,
        ..
    } = &t
    {
        assert!((*root_ratio - 0.5).abs() < 1e-6);
        if let LayoutNode::Split {
            ratio: inner_ratio, ..
        } = second.as_ref()
        {
            assert!((*inner_ratio - 0.6).abs() < 1e-6);
        } else {
            panic!("expected inner split");
        }
    } else {
        panic!("expected root split");
    }
}

#[test]
fn resize_falls_back_to_nearest_ancestor_on_axis() {
    let mut t = tree();
    // target is in the second subtree of the root (vertical split of 2 and 3)
    assert!(t.resize_pane(PaneId(3), SplitAxis::Horizontal, 0.1));
    if let LayoutNode::Split {
        ratio: root_ratio, ..
    } = &t
    {
        // ratio moves by the delta directly (directional boundary movement)
        assert!((*root_ratio - 0.6).abs() < 1e-6);
    } else {
        panic!("expected root split");
    }
}

#[test]
fn resize_no_axis_split_returns_false() {
    let mut t = tree();
    // target 1 is under the root H split; resize along Vertical with no V split
    // containing pane 1 anywhere -> false
    assert!(!t.resize_pane(PaneId(1), SplitAxis::Vertical, 0.1));
}

#[test]
fn remove_pane_preserves_sibling_subtrees() {
    let t = tree();
    let after = t.remove_pane(PaneId(2)).expect("ok").expect("survives");
    assert_eq!(after.pane_ids(), vec![PaneId(1), PaneId(3)]);
}

#[test]
fn split_pane_preserves_sibling_subtrees() {
    let t = tree();
    let after = t
        .split_pane(PaneId(1), PaneId(9), SplitAxis::Vertical, 0.5)
        .expect("ok");
    assert_eq!(
        after.pane_ids(),
        vec![PaneId(1), PaneId(9), PaneId(2), PaneId(3)]
    );
}
