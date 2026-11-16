use ratatui::layout::Rect;
use venti::config::theme::Theme;
use venti::render::chrome::{
    layout_workspace_bar, layout_workspace_sidebar, render_workspace_bar, render_workspace_sidebar,
};

fn ws_list(n: usize) -> Vec<(usize, String, bool)> {
    (1..=n).map(|i| (i, format!("ws{i}"), false)).collect()
}

/// regression: a stale tab_scroll_offset (>= workspace count) after rapid
/// closes must not swallow every tab; the layout clamps to the largest
/// offset that keeps the remaining tab visible.
#[test]
fn stale_offset_beyond_count_renders_pinned_arrow_without_tabs() {
    let area = Rect::new(0, 0, 120, 1);
    let workspaces = ws_list(1);
    let layout = layout_workspace_bar(area, &workspaces, 7);

    assert_eq!(layout.tabs.len(), 1);
    assert_eq!(layout.tabs[0].ws, 1);
    assert_eq!(layout.left_arrow_x, None);
    assert!(layout.right_arrow_x.is_none());

    // render must not panic and must draw the clamped first tab
    let theme = Theme::default_theme();
    let mut buf = ratatui::buffer::Buffer::empty(area);
    render_workspace_bar(
        area,
        &mut buf,
        &[(venti::layout::WorkspaceId(1), "ws1".to_string(), false)],
        venti::layout::WorkspaceId(1),
        "session",
        &theme,
        venti::config::schema::WorkspaceBarPosition::Top,
        7,
    );
    let text: String = (0..40).map(|x| buf[(x, 0)].symbol()).collect();
    assert!(text.contains("ws1"), "clamped tab should render: {text}");
}

/// regression: the '>' indicator must be pinned to the tab area's right edge,
/// not floating after the last rendered tab (it used to drift on rapid closes).
#[test]
fn right_arrow_is_pinned_at_tab_area_edge() {
    let area = Rect::new(0, 0, 80, 1);
    let workspaces = ws_list(8);

    let scrolled = layout_workspace_bar(area, &workspaces, 1);
    assert_eq!(scrolled.left_arrow_x, Some(1));
    // tab area: width 80 - 25 (session) - 3 (add) - 2 (left arrow) = 50 → end at 52
    assert_eq!(scrolled.right_arrow_x, Some(50));

    let unscrolled = layout_workspace_bar(area, &workspaces, 0);
    assert_eq!(unscrolled.left_arrow_x, None);
    assert_eq!(unscrolled.right_arrow_x, Some(50), "arrow must not drift");
}

#[test]
fn hit_zones_match_rendered_glyphs() {
    let area = Rect::new(0, 0, 90, 1);
    let workspaces = ws_list(6);
    let theme = Theme::default_theme();

    let vws: Vec<(venti::layout::WorkspaceId, String, bool)> = workspaces
        .iter()
        .map(|(i, n, _)| (venti::layout::WorkspaceId(*i), n.clone(), false))
        .collect();
    let layout = layout_workspace_bar(area, &workspaces, 2);

    let mut buf = ratatui::buffer::Buffer::empty(area);
    render_workspace_bar(
        area,
        &mut buf,
        &vws,
        venti::layout::WorkspaceId(3),
        "session",
        &theme,
        venti::config::schema::WorkspaceBarPosition::Top,
        2,
    );

    assert_eq!(buf[(layout.left_arrow_x.unwrap(), 0)].symbol(), "<");
    if let Some(rx) = layout.right_arrow_x {
        assert_eq!(buf[(rx, 0)].symbol(), ">");
    }
    if let Some(ax) = layout.add_x {
        assert_eq!(buf[(ax, 0)].symbol(), "+");
    }
    for tab in &layout.tabs {
        assert_eq!(buf[(tab.pill_x, 0)].symbol(), " ", "pill starts with pad");
        assert!(
            buf[(tab.pill_x + tab.pill_w - 1, 0)].symbol() == " ",
            "pill ends with pad"
        );
        assert_eq!(buf[(tab.close_x, 0)].symbol(), "×");
        assert_eq!(buf[(tab.close_x + 1, 0)].symbol(), " ");
        // separator follows each tab
        assert_eq!(buf[(tab.close_x + tab.close_w, 0)].symbol(), "│");
        // tabs never cross the right arrow anchor
        assert!(
            tab.pill_x + tab.pill_w + tab.close_w <= layout.right_arrow_x.unwrap_or(area.width),
            "tab overlaps arrow zone"
        );
    }
}

/// after clamping to a valid offset, layout must be identical to a fresh
/// render at that offset, i.e. offset changes never shift the '>' column.
#[test]
fn layout_is_offset_stable_across_scrolls() {
    let area = Rect::new(0, 0, 70, 1);
    let workspaces = ws_list(9);
    let first = layout_workspace_bar(area, &workspaces, 3);
    let second = layout_workspace_bar(area, &workspaces, 5);

    assert_eq!(first.right_arrow_x, second.right_arrow_x);
    assert_eq!(first.left_arrow_x, second.left_arrow_x);
    // both offsets must actually scroll (different first tab)
    assert_ne!(first.tabs[0].ws, second.tabs[0].ws);
}

#[test]
fn sidebar_vertical_tabs_highlight_workspace_and_show_focused_pane_titles() {
    let area = Rect::new(0, 0, 28, 10);
    let workspaces = vec![
        (
            venti::layout::WorkspaceId(1),
            "dev".into(),
            "zsh".into(),
            false,
        ),
        (
            venti::layout::WorkspaceId(2),
            "logs".into(),
            "tail -f app.log".into(),
            true,
        ),
    ];
    let pairs = vec![(1, "dev".into(), false), (2, "logs".into(), true)];
    let theme = Theme::default_theme();
    let layout = layout_workspace_sidebar(
        area,
        &pairs,
        0,
        venti::config::schema::SidebarDensity::Comfortable,
        false,
        true,
    );
    let mut buf = ratatui::buffer::Buffer::empty(area);

    render_workspace_sidebar(
        area,
        &mut buf,
        &workspaces,
        venti::layout::WorkspaceId(1),
        "default",
        &theme,
        false,
        venti::config::schema::SidebarDensity::Comfortable,
        true,
        0,
    );

    assert_eq!(layout.slots.len(), 2);
    assert!(layout.slots.iter().all(|slot| slot.row_h == 2));
    let active = layout.slots[0];
    let active_name: String = (0..area.width)
        .map(|x| buf[(x, active.y)].symbol())
        .collect();
    let active_hint: String = (0..area.width)
        .map(|x| buf[(x, active.y + 1)].symbol())
        .collect();
    assert!(active_name.contains("dev"), "{active_name}");
    assert!(active_hint.contains("zsh"), "{active_hint}");
    assert_eq!(buf[(area.x, active.y)].symbol(), "▌");
    assert_eq!(buf[(area.x, active.y + 1)].symbol(), "▌");
    assert_eq!(buf[(active.close_x, active.y)].symbol(), "×");
    assert_eq!(buf[(area.x + 5, active.y)].bg, theme.surface_color());
    assert_eq!(
        buf[(area.x + 5, active.y + 1)].bg,
        theme.surface_color(),
        "active workspace highlight spans both tab rows"
    );

    let inactive = layout.slots[1];
    let inactive_hint: String = (0..area.width)
        .map(|x| buf[(x, inactive.y + 1)].symbol())
        .collect();
    assert!(inactive_hint.contains("tail -f app.log"), "{inactive_hint}");
}

#[test]
fn sidebar_stays_inside_a_one_column_terminal() {
    let area = Rect::new(0, 0, 1, 6);
    let theme = Theme::default_theme();
    let mut buf = ratatui::buffer::Buffer::empty(area);

    render_workspace_sidebar(
        area,
        &mut buf,
        &[(
            venti::layout::WorkspaceId(1),
            "dev".into(),
            "zsh".into(),
            false,
        )],
        venti::layout::WorkspaceId(1),
        "default",
        &theme,
        false,
        venti::config::schema::SidebarDensity::Comfortable,
        false,
        0,
    );

    assert!(buf[(0, 0)].symbol().chars().count() <= 1);
}

/// regression: scrolling past the last tab must stop at the offset where the
/// final workspace is fully visible, never past it.
#[test]
fn bar_scroll_stops_when_last_tab_visible() {
    let area = Rect::new(0, 0, 120, 1);
    let workspaces = ws_list(20);

    let max = venti::render::chrome::max_tab_scroll_offset(area, &workspaces);
    // at the max offset every remaining tab renders
    let at_max = layout_workspace_bar(area, &workspaces, max);
    assert_eq!(at_max.tabs.len(), workspaces.len() - max);
    assert!(at_max.right_arrow_x.is_none());
    // one past the max would clip the last tab, the layout clamps it instead
    let over = layout_workspace_bar(area, &workspaces, max + 1);
    assert_eq!(over.tabs.len(), at_max.tabs.len());
    assert_eq!(
        over.tabs.first().map(|t| t.ws),
        at_max.tabs.first().map(|t| t.ws)
    );
    // and a huge stale offset still shows the tail
    let stale = layout_workspace_bar(area, &workspaces, usize::MAX);
    assert_eq!(stale.tabs.len(), workspaces.len() - max);
}

#[test]
fn sidebar_scroll_stops_when_last_workspace_visible() {
    let area = Rect::new(0, 0, 28, 20);
    let pairs = ws_list(10);
    let max = venti::render::chrome::max_sidebar_scroll_offset(
        area,
        &pairs,
        venti::config::schema::SidebarDensity::Comfortable,
        false,
        true,
    );
    let at_max = layout_workspace_sidebar(
        area,
        &pairs,
        max,
        venti::config::schema::SidebarDensity::Comfortable,
        false,
        true,
    );
    assert_eq!(at_max.slots.len(), pairs.len() - max);
    assert!(at_max.down_arrow_y.is_none());
    let over = layout_workspace_sidebar(
        area,
        &pairs,
        usize::MAX,
        venti::config::schema::SidebarDensity::Comfortable,
        false,
        true,
    );
    assert_eq!(over.slots.len(), pairs.len() - max);

    // compact density fits more rows, so it must allow a smaller (or equal) max
    let max_compact = venti::render::chrome::max_sidebar_scroll_offset(
        area,
        &pairs,
        venti::config::schema::SidebarDensity::Compact,
        false,
        true,
    );
    assert!(max_compact <= max);
}
