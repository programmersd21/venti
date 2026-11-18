use proptest::prelude::*;
use ratatui::layout::Rect;
use venti::layout::{Direction, FocusManager, LayoutNode, PaneId, SplitAxis, resolve};

#[test]
fn test_tree_split_and_remove() {
    let root = LayoutNode::leaf(PaneId(1));
    let root = root
        .split_pane(PaneId(1), PaneId(2), SplitAxis::Horizontal, 0.5)
        .expect("split ok");
    assert_eq!(root.pane_ids(), vec![PaneId(1), PaneId(2)]);

    let root = root
        .split_pane(PaneId(2), PaneId(3), SplitAxis::Vertical, 0.5)
        .expect("split ok");
    assert_eq!(root.pane_ids(), vec![PaneId(1), PaneId(2), PaneId(3)]);

    let after_remove = root
        .remove_pane(PaneId(2))
        .expect("remove ok")
        .expect("remains");
    assert_eq!(after_remove.pane_ids(), vec![PaneId(1), PaneId(3)]);
}

#[test]
fn test_spatial_focus() {
    let root = LayoutNode::split(
        SplitAxis::Horizontal,
        0.5,
        LayoutNode::leaf(PaneId(1)),
        LayoutNode::leaf(PaneId(2)),
    );
    let area = Rect::new(0, 0, 100, 40);
    let rects = resolve(&root, area, 0, 0);

    let mut focus = FocusManager::new();
    focus.set_focus(PaneId(1));

    let right = focus.spatial_neighbor(PaneId(1), Direction::Right, &rects);
    assert_eq!(right, Some(PaneId(2)));

    let left = focus.spatial_neighbor(PaneId(2), Direction::Left, &rects);
    assert_eq!(left, Some(PaneId(1)));
}

#[test]
fn oversized_outer_gap_keeps_layout_inside_parent() {
    let node = LayoutNode::split(
        SplitAxis::Horizontal,
        0.5,
        LayoutNode::leaf(PaneId(1)),
        LayoutNode::leaf(PaneId(2)),
    );
    let area = Rect::new(100, 50, 4, 3);
    let rects = resolve(&node, area, 0, u16::MAX);

    for (_, rect) in rects {
        assert!(u32::from(rect.x) >= u32::from(area.x));
        assert!(u32::from(rect.y) >= u32::from(area.y));
        assert!(
            u32::from(rect.x) + u32::from(rect.width) <= u32::from(area.x) + u32::from(area.width)
        );
        assert!(
            u32::from(rect.y) + u32::from(rect.height)
                <= u32::from(area.y) + u32::from(area.height)
        );
    }
}

proptest! {
    #[test]
    fn test_layout_tiling_invariants(
        w in 40u16..300,
        h in 20u16..100,
        splits in proptest::collection::vec(
            (0..2u8, 100u16..900u16)
                .prop_map(|(axis, ratio)| (axis, f32::from(ratio) / 1000.0)),
            1..8
        )
    ) {
        let area = Rect::new(0, 0, w, h);
        let mut tree = LayoutNode::leaf(PaneId(1));
        let mut next_id = 2u32;

        for (axis_choice, ratio) in splits {
            let axis = if axis_choice == 0 { SplitAxis::Horizontal } else { SplitAxis::Vertical };
            let current_ids = tree.pane_ids();
            let target = current_ids[0];
            match tree.split_pane(target, PaneId(next_id), axis, ratio) {
                Ok(new_tree) => {
                    tree = new_tree;
                    next_id += 1;
                }
                Err(old_tree) => {
                    tree = old_tree;
                }
            }
        }

        let rects = resolve(&tree, area, 0, 0);
        let all_ids = tree.pane_ids();
        prop_assert_eq!(rects.len(), all_ids.len());

        // Check non-overlapping and coverage
        for (i, &(id1, r1)) in rects.iter().enumerate() {
            prop_assert!(all_ids.contains(&id1));
            prop_assert!(r1.x >= area.x);
            prop_assert!(r1.y >= area.y);
            prop_assert!(r1.x + r1.width <= area.x + area.width);
            prop_assert!(r1.y + r1.height <= area.y + area.height);

            for &(id2, r2) in rects.iter().skip(i + 1) {
                let x_overlap = r1.x < r2.x + r2.width && r1.x + r1.width > r2.x;
                let y_overlap = r1.y < r2.y + r2.height && r1.y + r1.height > r2.y;
                prop_assert!(!(x_overlap && y_overlap), "panes {id1} and {id2} overlap");
            }
        }
    }
}

#[test]
fn pane_view_paints_every_cell_in_its_area() {
    // stale cells surviving a frame would keep old background colors from
    // apps that paint their own (e.g. cmatrix). every cell in the pane area
    // must be rewritten on every render.
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};
    use ratatui::widgets::Widget;

    let mut parser = vt100::Parser::new(3, 4, 0);
    parser.process(b"hi");
    let screen = parser.screen();

    let theme = venti::config::theme::Theme::builtin("mocha").expect("mocha");
    let area = Rect::new(0, 0, 6, 5);
    let mut buf = Buffer::empty(area);
    // poison out-of-grid cells with a loud, unmistakable style; the widget
    // must reset all of them.
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &mut buf[(x, y)];
            cell.set_char('X');
            cell.set_style(Style::default().fg(Color::Red).bg(Color::Green));
        }
    }

    let view = venti::render::PaneView {
        screen,
        theme: &theme,
        is_focused: true,
        dim_inactive: false,
        dim_strength: 0.0,
        selection: None,
        search: None,
        scroll_offset: 0,
        scrollback_len: 0,
    };
    view.render(area, &mut buf);

    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(x, y)];
            let ch = cell.symbol();
            assert_ne!(
                (cell.fg, cell.bg, ch),
                (Color::Red, Color::Green, "X"),
                "cell ({x}, {y}) kept its stale poisoned style"
            );
        }
    }
}

#[test]
fn pane_view_clips_wide_glyph_at_area_edge() {
    use ratatui::buffer::Buffer;
    use ratatui::widgets::Widget;

    let mut parser = vt100::Parser::new(1, 4, 0);
    parser.process("ab界".as_bytes());
    let theme = venti::config::theme::Theme::builtin("mocha").expect("theme");
    let area = Rect::new(0, 0, 3, 1);
    let mut buffer = Buffer::empty(area);
    venti::render::PaneView {
        screen: parser.screen(),
        theme: &theme,
        is_focused: true,
        dim_inactive: false,
        dim_strength: 0.0,
        selection: None,
        search: None,
        scroll_offset: 0,
        scrollback_len: 0,
    }
    .render(area, &mut buffer);

    assert_eq!(buffer[(2, 0)].symbol(), " ");
}

#[test]
fn regression_vertical_split_puts_new_pane_at_bottom_not_top() {
    // Regression test for issue where F3 (new pane) was splitting
    // north (top) instead of down (bottom) when split_axis was set to Down.
    // The new pane should be at y-offset = first_pane_height, not at y=0.
    let root = LayoutNode::leaf(PaneId(1));
    let root = root
        .split_pane_directed(
            PaneId(1),
            PaneId(2),
            SplitAxis::Vertical,
            0.5,
            venti::layout::SplitDirection::Second,
        )
        .expect("split ok");

    let area = Rect::new(0, 0, 80, 40);
    let rects = resolve(&root, area, 0, 0);

    // Find the rects for pane 1 and pane 2
    let rect1 = rects
        .iter()
        .find(|(id, _)| *id == PaneId(1))
        .map(|(_, r)| *r)
        .expect("pane 1");
    let rect2 = rects
        .iter()
        .find(|(id, _)| *id == PaneId(2))
        .map(|(_, r)| *r)
        .expect("pane 2");

    // Pane 1 should be at the top (y=0)
    assert_eq!(rect1.y, 0, "pane 1 should be at top (y=0)");

    // Pane 2 should be below pane 1 (y > 0)
    assert!(
        rect2.y > rect1.y,
        "pane 2 (new) should be below pane 1, not above. rect2.y={}, rect1.y={}",
        rect2.y,
        rect1.y
    );
    assert_eq!(
        rect2.y,
        rect1.y + rect1.height,
        "pane 2 should be directly below pane 1"
    );
}

#[test]
fn test_float_toggle_restores_original_tiled_position() {
    use venti::server::session::Session;

    let mut session = Session::new("float-test");
    let area = Rect::new(0, 0, 120, 40);
    let shell = "/bin/sh";
    let first = session
        .new_pane(40, 20, 100, shell, None, Some(SplitAxis::Horizontal))
        .expect("spawn first pane");
    let second = session
        .new_pane(40, 20, 100, shell, None, Some(SplitAxis::Horizontal))
        .expect("spawn second pane");
    let third = session
        .new_pane(40, 20, 100, shell, None, Some(SplitAxis::Horizontal))
        .expect("spawn third pane");

    let original = session.active_tree().expect("tree").pane_ids();
    assert_eq!(original, vec![first, second, third]);

    session.focus_pane(second);
    assert!(session.toggle_float(area));
    assert!(session.is_pane_floating(second));
    assert_eq!(
        session.active_tree().expect("tree after float").pane_ids(),
        vec![first, third]
    );

    assert!(session.toggle_float(area));
    assert!(!session.is_pane_floating(second));
    assert_eq!(
        session.active_tree().expect("restored tree").pane_ids(),
        original,
        "unfloating should restore the pane's original position"
    );
    assert_eq!(session.focused_pane, Some(second));

    for pane in session.panes.values() {
        pane.kill_tree();
    }
}
