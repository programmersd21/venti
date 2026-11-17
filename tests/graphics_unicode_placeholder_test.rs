use venti::pane::graphics::{GraphicCommand, GraphicFormat, KittyControlData, PaneGraphicsStore};

#[test]
fn graphic_store_preserves_virtual_placeholders_on_screen_clear() {
    let mut store = PaneGraphicsStore::new();

    // Add a virtual placement
    let control = KittyControlData {
        a: Some("T".to_string()),
        i: Some(1001),
        p: Some(2001),
        U: Some(1),
        ..Default::default()
    };

    let cmd = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 1,
        row: 5,
        col: 10,
        alternate_screen: false,
        data: b"\x1b_G\
                 a=T,i=1001,p=2001,U=1\
                 ;PAYLOAD\x1b\\"
            .to_vec(),
        kitty_metadata: Some(control),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };

    store.push(cmd);
    assert_eq!(store.active_commands().len(), 1);

    // Clear the screen
    store.clear_screen_images(false);

    // The clear generates deletion commands for images
    let commands = store.active_commands();
    // We expect the delete command (1) emitted by clear_screen_images
    assert_eq!(commands.len(), 1);

    // The remaining command should be a delete
    assert_eq!(
        commands[0].kitty_metadata.as_ref().unwrap().a.as_deref(),
        Some("d")
    );
}

#[test]
fn graphic_command_clamp_to_pane_bounds_for_virtual_placeholder() {
    let control = KittyControlData {
        a: Some("T".to_string()),
        U: Some(1), // virtual
        ..Default::default()
    };

    let mut cmd = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 1,
        row: 100, // out of bounds
        col: 200, // out of bounds
        alternate_screen: false,
        data: b"\x1b_GA=T,U=1;PAYLOAD\x1b\\".to_vec(),
        kitty_metadata: Some(control),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };

    cmd.clamp_to_pane_bounds(50, 25);

    // Should clamp row and col to within bounds
    assert!(cmd.row < 25);
    assert!(cmd.col < 50);
}

#[test]
fn graphic_command_clamp_to_pane_bounds_respects_max_values() {
    let control = KittyControlData {
        a: Some("T".to_string()),
        U: Some(1), // virtual
        ..Default::default()
    };

    let mut cmd = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 1,
        row: i32::from(u16::MAX),
        col: u16::MAX,
        alternate_screen: false,
        data: b"\x1b_GA=T,U=1;PAYLOAD\x1b\\".to_vec(),
        kitty_metadata: Some(control),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };

    cmd.clamp_to_pane_bounds(50, 25);

    // Should clamp to just below the max
    assert_eq!(cmd.row, 24); // saturating_sub(1) from 25
    assert_eq!(cmd.col, 49); // saturating_sub(1) from 50
}

#[test]
fn virtual_placeholders_can_be_mixed_with_real_placements() {
    let mut store = PaneGraphicsStore::new();

    // Add a real (non-virtual) placement
    let real_control = KittyControlData {
        a: Some("T".to_string()),
        i: Some(1001),
        p: Some(2001),
        ..Default::default()
    };

    let real_cmd = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 1,
        row: 5,
        col: 10,
        alternate_screen: false,
        data: b"\x1b_G\
                 a=T,i=1001,p=2001\
                 ;PAYLOAD1\x1b\\"
            .to_vec(),
        kitty_metadata: Some(real_control),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };

    store.push(real_cmd);

    // Add a virtual placement
    let virtual_control = KittyControlData {
        a: Some("T".to_string()),
        i: Some(1002),
        p: Some(2002),
        U: Some(1),
        ..Default::default()
    };

    let virtual_cmd = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 2,
        row: 7,
        col: 15,
        alternate_screen: false,
        data: b"\x1b_G\
                 a=T,i=1002,p=2002,U=1\
                 ;PAYLOAD2\x1b\\"
            .to_vec(),
        kitty_metadata: Some(virtual_control),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };

    store.push(virtual_cmd);

    let commands = store.active_commands();
    assert_eq!(commands.len(), 2);

    // Both should be in the store
    let seqs: Vec<u64> = commands.iter().map(|c| c.seq).collect();
    assert!(seqs.contains(&1));
    assert!(seqs.contains(&2));
}

#[test]
fn unicode_placeholder_enabled_graphics_work_in_scrollback() {
    let mut store = PaneGraphicsStore::new();

    // Add a virtual placement at row 5
    let control = KittyControlData {
        a: Some("T".to_string()),
        i: Some(1001),
        p: Some(2001),
        U: Some(1),
        ..Default::default()
    };

    let cmd = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 1,
        row: 5,
        col: 10,
        alternate_screen: false,
        data: b"\x1b_G\
                 a=T,i=1001,p=2001,U=1\
                 ;PAYLOAD\x1b\\"
            .to_vec(),
        kitty_metadata: Some(control),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };

    store.push(cmd);
    assert_eq!(store.active_commands()[0].row, 5);

    // Scroll up by 3 rows
    store.scroll_rows(3);

    // Virtual placement should move up by 3
    assert_eq!(store.active_commands()[0].row, 2);

    // scroll up by 5 more (rows go negative; the image is above the viewport
    // but is preserved, not deleted or pinned to row 0)
    store.scroll_rows(5);
    assert_eq!(store.active_commands()[0].row, -3);

    // A downward grid scroll moves the content and its anchor down together.
    store.scroll_rows(-5);
    assert_eq!(store.active_commands()[0].row, 2);
}
