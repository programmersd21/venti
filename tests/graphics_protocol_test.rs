/// Kitty graphics protocol implementation tests
use venti::pane::graphics::{GraphicFormat, GraphicsParser, PaneGraphicsStore, Segment};

#[test]
fn test_single_chunk_kitty_transmission() {
    let mut parser = GraphicsParser::new();
    let seq = b"\x1b_Gf=100,i=1;AAAA\x1b\\";
    let segments = parser.split(seq);

    assert_eq!(segments.len(), 1);
    match &segments[0] {
        Segment::Graphic(cmd) => {
            assert_eq!(cmd.format, GraphicFormat::Kitty);
            assert!(cmd.kitty_metadata.is_some());
            let meta = cmd.kitty_metadata.as_ref().unwrap();
            assert_eq!(meta.f, Some(100)); // PNG format
            assert_eq!(meta.i, Some(1)); // Image ID = 1
        }
        _ => panic!("Expected Graphic segment"),
    }
}

#[test]
fn test_chunked_accumulation() {
    let mut parser = GraphicsParser::new();

    let chunk1 = b"\x1b_Gf=32,s=100,v=100,i=2,m=1;AAAA\x1b\\";
    let segments1 = parser.split(chunk1);
    assert!(segments1.is_empty(), "First chunk (m=1) should not emit");

    let chunk2 = b"\x1b_Gm=1;BBBB\x1b\\";
    let segments2 = parser.split(chunk2);
    assert!(segments2.is_empty(), "Middle chunk (m=1) should not emit");

    let chunk3 = b"\x1b_Gm=0;CCCC\x1b\\";
    let segments3 = parser.split(chunk3);

    assert_eq!(segments3.len(), 1, "Final chunk (m=0) should emit");
    match &segments3[0] {
        Segment::Graphic(cmd) => {
            let meta = cmd.kitty_metadata.as_ref().unwrap();
            assert_eq!(meta.f, Some(32));
            assert_eq!(meta.i, Some(2));
            // Check payload accumulation
            assert!(
                cmd.data.len() > 20,
                "Payload should be accumulated across chunks"
            );
        }
        _ => panic!("Expected Graphic segment"),
    }

    // the re-emitted single-shot sequence must not carry a chunked-transfer marker
    let raw = match &segments3[0] {
        Segment::Graphic(cmd) => String::from_utf8(cmd.data.clone()).unwrap(),
        Segment::Plain(_) => unreachable!(),
    };
    assert!(!raw.contains("m=1"), "no dangling m=1 in {raw}");
    assert!(!raw.contains("m="), "no m key at all in {raw}");
}

#[test]
fn chunked_kitty_payload_is_bounded_and_parser_recovers() {
    let mut parser = GraphicsParser::new();
    let first = [
        b"\x1b_Gf=32,i=9,m=1;".as_slice(),
        &vec![b'A'; 5 * 1024 * 1024],
        b"\x1b\\",
    ]
    .concat();
    assert!(parser.split(&first).is_empty());

    let overflow = [
        b"\x1b_Gm=1;".as_slice(),
        &vec![b'B'; 4 * 1024 * 1024],
        b"\x1b\\",
    ]
    .concat();
    assert!(parser.split(&overflow).is_empty());

    assert!(parser.split(b"\x1b_Gm=0;CCCC\x1b\\").is_empty());

    let next = parser.split(b"\x1b_Gf=24,i=10;AQ==\x1b\\");
    assert_eq!(next.len(), 1);
    assert!(matches!(next[0], Segment::Graphic(_)));
}

#[test]
fn test_metadata_parsing() {
    let mut parser = GraphicsParser::new();
    let seq = b"\x1b_Ga=T,f=100,i=5,c=10,r=20,U=1,q=2;DATA\x1b\\";
    let segments = parser.split(seq);

    assert_eq!(segments.len(), 1);
    match &segments[0] {
        Segment::Graphic(cmd) => {
            let meta = cmd.kitty_metadata.as_ref().unwrap();
            assert_eq!(meta.a.as_deref(), Some("T"));
            assert_eq!(meta.f, Some(100));
            assert_eq!(meta.i, Some(5));
            assert_eq!(meta.c, Some(10));
            assert_eq!(meta.r, Some(20));
            assert_eq!(meta.U, Some(1));
            assert_eq!(meta.q, Some(2));
        }
        _ => panic!("Expected Graphic segment"),
    }
}

#[test]
fn test_deletion_command() {
    let mut parser = GraphicsParser::new();
    let seq = b"\x1b_Ga=d,d=i,i=42,q=2\x1b\\";
    let segments = parser.split(seq);

    assert_eq!(segments.len(), 1);
    match &segments[0] {
        Segment::Graphic(cmd) => {
            let meta = cmd.kitty_metadata.as_ref().unwrap();
            assert_eq!(meta.a.as_deref(), Some("d"));
            assert_eq!(meta.d.as_deref(), Some("i"));
            assert_eq!(meta.i, Some(42));
            assert_eq!(meta.q, Some(2));
        }
        _ => panic!("Expected Graphic segment"),
    }
}

#[test]
fn test_plain_text_preservation() {
    let mut parser = GraphicsParser::new();
    let data = b"Hello \x1b_Gi=1,f=100;ABC\x1b\\ World";
    let segments = parser.split(data);

    assert_eq!(segments.len(), 3);

    match &segments[0] {
        Segment::Plain(p) => assert_eq!(p, b"Hello "),
        _ => panic!("Expected plain text"),
    }

    match &segments[1] {
        Segment::Graphic(cmd) => {
            assert_eq!(cmd.format, GraphicFormat::Kitty);
            assert_eq!(cmd.kitty_metadata.as_ref().unwrap().i, Some(1));
        }
        _ => panic!("Expected graphic"),
    }

    match &segments[2] {
        Segment::Plain(p) => assert_eq!(p, b" World"),
        _ => panic!("Expected plain text"),
    }
}

#[test]
fn test_multiple_sequences() {
    let mut parser = GraphicsParser::new();

    let seq1 = b"\x1b_Gi=1,f=24,s=1,v=1;AAA\x1b\\";
    let seq2 = b"\x1b_Gi=2,f=100;BBBB\x1b\\";
    let combined = [seq1.to_vec(), seq2.to_vec()].concat();

    let segments = parser.split(&combined);

    assert_eq!(segments.len(), 2);
    for (i, seg) in segments.iter().enumerate() {
        match seg {
            Segment::Graphic(cmd) => {
                assert_eq!(cmd.format, GraphicFormat::Kitty);
                let meta = cmd.kitty_metadata.as_ref().unwrap();
                assert_eq!(meta.i, Some((i + 1) as u32));
            }
            _ => panic!("Expected Graphic segment"),
        }
    }
}

#[test]
fn test_control_data_serialization() {
    use venti::pane::graphics::KittyControlData;

    let data = KittyControlData {
        a: Some("T".to_string()),
        f: Some(100),
        i: Some(42),
        q: Some(2),
        ..Default::default()
    };

    let serialized = data.to_string();
    assert!(serialized.contains("a=T"));
    assert!(serialized.contains("f=100"));
    assert!(serialized.contains("i=42"));
    assert!(serialized.contains("q=2"));
}

#[test]
fn test_control_data_parsing() {
    use venti::pane::graphics::KittyControlData;

    let ctl_str = "a=T,f=100,i=5,s=200,v=150,c=10,r=20";
    let data = KittyControlData::parse(ctl_str);

    assert_eq!(data.a.as_deref(), Some("T"));
    assert_eq!(data.f, Some(100));
    assert_eq!(data.i, Some(5));
    assert_eq!(data.s, Some(200));
    assert_eq!(data.v, Some(150));
    assert_eq!(data.c, Some(10));
    assert_eq!(data.r, Some(20));
}

#[test]
fn oversized_sixel_is_discarded_without_leaking_payload_into_pane_text() {
    let mut parser = GraphicsParser::new();
    let mut input = b"\x1bPq".to_vec();
    input.extend(std::iter::repeat_n(b'~', 32 * 1024 * 1024 + 1));
    input.extend_from_slice(b"\x1b\\visible");

    let segments = parser.split(&input);
    assert_eq!(segments, vec![Segment::Plain(b"visible".to_vec())]);
}

#[test]
fn sixel_payloads_larger_than_eight_mib_are_preserved() {
    let mut parser = GraphicsParser::new();
    let mut input = b"\x1bPq".to_vec();
    input.extend(std::iter::repeat_n(b'~', 9 * 1024 * 1024));
    input.extend_from_slice(b"\x1b\\");

    let mut segments = parser.split(&input);
    assert_eq!(segments.len(), 1);
    let Segment::Graphic(command) = segments.pop().unwrap() else {
        panic!("expected one sixel graphic command");
    };
    assert_eq!(command.format, GraphicFormat::Sixel);

    let mut store = PaneGraphicsStore::new();
    store.push((*command).clone());
    assert_eq!(store.active_commands().len(), 1);
    assert_eq!(store.active_commands()[0].format, GraphicFormat::Sixel);
}

#[test]
fn kitty_placement_fits_the_remaining_pane_bounds() {
    let mut parser = GraphicsParser::new();
    let mut segments = parser.split(b"\x1b_Ga=T,f=100,i=7,c=80,r=40;AAAA\x1b\\");
    let Segment::Graphic(command) = &mut segments[0] else {
        panic!("expected kitty graphic");
    };

    command.fit_kitty_to_bounds(30, 10);

    let metadata = command.kitty_metadata.as_ref().unwrap();
    assert_eq!((metadata.c, metadata.r), (Some(20), Some(10)));
    assert!(command.data.windows(4).any(|window| window == b"c=20"));
    assert!(command.data.windows(4).any(|window| window == b"r=10"));
    let reemit = command.reemit.as_ref().unwrap();
    assert!(reemit.windows(4).any(|window| window == b"c=20"));
    assert!(reemit.windows(4).any(|window| window == b"r=10"));
}

#[test]
fn kitty_native_size_image_is_not_upscaled_to_fill_its_pane() {
    let mut parser = GraphicsParser::new();
    let mut segments = parser.split(b"\x1b_Ga=T,f=100,i=8;AAAA\x1b\\");
    let Segment::Graphic(command) = &mut segments[0] else {
        panic!("expected kitty graphic");
    };
    let original_data = command.data.clone();

    command.fit_kitty_to_bounds(80, 40);

    let metadata = command.kitty_metadata.as_ref().unwrap();
    assert_eq!((metadata.c, metadata.r), (None, None));
    assert_eq!(command.data, original_data);
}

#[test]
fn kitty_single_axis_size_keeps_terminal_aspect_ratio_calculation() {
    let mut parser = GraphicsParser::new();
    let mut segments = parser.split(b"\x1b_Ga=T,f=100,i=9,c=120;AAAA\x1b\\");
    let Segment::Graphic(command) = &mut segments[0] else {
        panic!("expected kitty graphic");
    };

    command.fit_kitty_to_bounds(40, 20);

    let metadata = command.kitty_metadata.as_ref().unwrap();
    assert_eq!((metadata.c, metadata.r), (Some(40), None));
}

#[test]
fn screen_clear_queues_a_delete_for_the_pane_image_id() {
    let mut parser = GraphicsParser::new();
    let Segment::Graphic(image) = parser.split(b"\x1b_Ga=T,f=100,i=93;AAAA\x1b\\").remove(0) else {
        panic!("expected kitty image");
    };
    let mut store = PaneGraphicsStore::new();
    store.push(*image);
    let assigned_id = store.active_commands()[0]
        .kitty_metadata
        .as_ref()
        .and_then(|metadata| metadata.i)
        .expect("image has an assigned id");

    store.clear_screen_images(false);

    assert_eq!(store.active_commands().len(), 1);
    let deletion = &store.active_commands()[0];
    let metadata = deletion.kitty_metadata.as_ref().unwrap();
    assert_eq!(metadata.a.as_deref(), Some("d"));
    assert_eq!(metadata.d.as_deref(), Some("i"));
    assert_eq!(metadata.i, Some(assigned_id));
}

#[test]
fn separate_panes_receive_distinct_outer_kitty_image_ids() {
    let mut parser = GraphicsParser::new();
    let sequence = b"\x1b_Ga=T,f=100,i=7;AAAA\x1b\\";
    let Segment::Graphic(first) = parser.split(sequence).remove(0) else {
        panic!("expected kitty image");
    };
    let Segment::Graphic(second) = parser.split(sequence).remove(0) else {
        panic!("expected kitty image");
    };
    let mut first_pane = PaneGraphicsStore::new();
    let mut second_pane = PaneGraphicsStore::new();
    first_pane.push(*first);
    second_pane.push(*second);
    let first_id = first_pane.active_commands()[0]
        .kitty_metadata
        .as_ref()
        .and_then(|metadata| metadata.i);
    let second_id = second_pane.active_commands()[0]
        .kitty_metadata
        .as_ref()
        .and_then(|metadata| metadata.i);
    assert_ne!(first_id, second_id);
}

#[test]
fn clamp_graphics_to_bounds_reduces_oversized_dimensions() {
    use venti::pane::graphics::clamp_graphics_to_bounds;

    // Create a Kitty graphics command with c=80,r=40 (image 80x40 cells)
    let data = b"\x1b_Ga=p,i=7,c=80,r=40\x1b\\";

    // Test 1: Image is 80x40 but pane is only 40x30, positioned at (0,0)
    // Should clamp to c=40,r=30
    let result = clamp_graphics_to_bounds(data, 0, 0, 40, 30);
    assert!(
        result.is_some(),
        "Should return Some when clamping is needed"
    );

    let clamped = result.unwrap();
    let clamped_str = std::str::from_utf8(&clamped[..]).unwrap();
    assert!(clamped_str.contains("c=40"), "Should clamp width to 40");
    assert!(clamped_str.contains("r=30"), "Should clamp height to 30");

    // Test 2: Image at position (30, 20) in pane of 80x40
    // Available space: width=50, height=20, so should clamp to c=50,r=20
    let result2 = clamp_graphics_to_bounds(data, 30, 20, 80, 40);
    assert!(result2.is_some());

    let clamped2 = result2.unwrap();
    let clamped2_str = std::str::from_utf8(&clamped2[..]).unwrap();
    assert!(
        clamped2_str.contains("c=50"),
        "Should clamp width to available 50"
    );
    assert!(
        clamped2_str.contains("r=20"),
        "Should clamp height to available 20"
    );

    // Test 3: Image fits perfectly - should return None
    let data_small = b"\x1b_Ga=p,i=7,c=30,r=20\x1b\\";
    let result3 = clamp_graphics_to_bounds(data_small, 10, 10, 80, 40);
    assert!(
        result3.is_none(),
        "Should return None when no clamping needed"
    );
}

#[test]
fn kitty_bounds_clamp_never_moves_control_fields_into_image_payload() {
    use venti::pane::graphics::clamp_graphics_to_bounds;

    let data = b"\x1b_Ga=T,f=100,i=7,c=80,r=40;QUJD\x1b\\";
    let clamped = clamp_graphics_to_bounds(data, 0, 0, 40, 30)
        .expect("oversized explicit placement should be constrained");

    assert!(clamped.starts_with(b"\x1b_G"));
    assert!(clamped.windows(4).any(|window| window == b"c=40"));
    assert!(clamped.windows(4).any(|window| window == b"r=30"));
    assert!(
        clamped.ends_with(b";QUJD\x1b\\"),
        "payload must remain after the control separator: {:?}",
        String::from_utf8_lossy(&clamped)
    );
}

#[test]
fn kitty_native_size_clamp_does_not_append_dimensions_to_payload() {
    use venti::pane::graphics::clamp_graphics_to_bounds;

    let data = b"\x1b_Ga=T,f=100,i=8;QUJD\x1b\\";
    assert_eq!(clamp_graphics_to_bounds(data, 0, 0, 80, 40), None);
}

#[test]
fn kitty_native_png_is_scaled_down_only_when_it_exceeds_the_pane() {
    use venti::pane::graphics::clamp_graphics_to_bounds;

    let data = b"\x1b_Ga=T,f=24,s=1600,v=900,i=9;RAWDATA\x1b\\";
    let clamped = clamp_graphics_to_bounds(data, 0, 0, 80, 40)
        .expect("native image larger than pane should be fitted");
    assert!(clamped.windows(4).any(|window| window == b"c=80"));
    assert!(clamped.windows(4).any(|window| window == b"r=23"));
    assert!(clamped.ends_with(b";RAWDATA\x1b\\"));
}

#[test]
fn kitty_clamp_skips_delete_and_fits_placement_in_combined_reemit() {
    use venti::pane::graphics::clamp_graphics_to_bounds;

    let data = b"\x1b_Ga=d,d=i,i=9,p=4,q=2\x1b\\\x1b_Ga=p,i=9,p=4,c=120,r=60,q=2\x1b\\";
    let clamped = clamp_graphics_to_bounds(data, 0, 0, 40, 30)
        .expect("second placement command should be fitted");
    assert!(clamped.starts_with(b"\x1b_Ga=d,d=i,i=9,p=4,q=2\x1b\\"));
    assert!(clamped.ends_with(b"a=p,i=9,p=4,c=40,r=30,q=2\x1b\\"));
}
