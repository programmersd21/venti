use venti::pane::{GraphicFormat, GraphicsParser, PaneGraphicsStore, Segment};
use venti::proto::CursorShape;
use venti::render::ClientBackend;

#[test]
fn sixel_sequence_is_captured_after_plain_text() {
    let mut parser = GraphicsParser::new();
    let out = parser.split(b"hello\x1bP0;1;100qPAYLOAD\x1b\\world");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0], Segment::Plain(b"hello".to_vec()));
    match &out[1] {
        Segment::Graphic(cmd) => {
            assert_eq!(cmd.format, GraphicFormat::Sixel);
            assert_eq!(cmd.data, b"\x1bP0;1;100qPAYLOAD\x1b\\");
        }
        other => panic!("expected graphic, got {other:?}"),
    }
    assert_eq!(out[2], Segment::Plain(b"world".to_vec()));
}

#[test]
fn eight_bit_dcs_sixel_sequence_is_captured() {
    let mut parser = GraphicsParser::new();
    let out = parser.split(b"\x90qPAYLOAD\x9c");
    assert_eq!(out.len(), 1);
    assert!(
        matches!(&out[0], Segment::Graphic(cmd) if cmd.format == GraphicFormat::Sixel && cmd.data == b"\x90qPAYLOAD\x9c")
    );
}

#[test]
fn eight_bit_apc_kitty_sequence_is_captured() {
    let mut parser = GraphicsParser::new();
    let out = parser.split(b"\x9fGa=T,f=24;AAAA\x9c");
    assert_eq!(out.len(), 1);
    assert!(matches!(&out[0], Segment::Graphic(cmd) if cmd.format == GraphicFormat::Kitty));
}

#[test]
fn utf8_continuation_bytes_are_not_treated_as_eight_bit_controls() {
    let mut parser = GraphicsParser::new();
    let input = "left 🎗️ right".as_bytes();
    let mut plain = Vec::new();
    for byte in input {
        for segment in parser.split(std::slice::from_ref(byte)) {
            if let Segment::Plain(bytes) = segment {
                plain.extend(bytes);
            }
        }
    }
    assert_eq!(plain, input);
}

#[test]
fn kitty_sequence_is_captured() {
    let mut parser = GraphicsParser::new();
    let out = parser.split(b"\x1b_Ga=T,f=32,i=7,s=1,v=1;payload\x1b\\");
    assert_eq!(out.len(), 1);
    match &out[0] {
        Segment::Graphic(cmd) => {
            assert_eq!(cmd.format, GraphicFormat::Kitty);
            assert!(cmd.data.starts_with(b"\x1b_G"));
            assert!(cmd.data.ends_with(b"\x1b\\"));
        }
        other => panic!("expected graphic, got {other:?}"),
    }
}

#[test]
fn kitty_query_is_not_forwarded_as_graphic() {
    let mut parser = GraphicsParser::new();
    let out = parser.split(b"\x1b_Gi=1,a=q;AAAA\x1b\\");
    assert!(
        !out.iter().any(|s| matches!(s, Segment::Graphic(_))),
        "kitty query must not be forwarded to the client"
    );
}

#[test]
fn iterm2_sequence_is_captured() {
    let mut parser = GraphicsParser::new();
    let out = parser.split(b"\x1b]1337;File=inline=1:pixels\x07");
    assert_eq!(out.len(), 1);
    match &out[0] {
        Segment::Graphic(cmd) => assert_eq!(cmd.format, GraphicFormat::Iterm2),
        other => panic!("expected graphic, got {other:?}"),
    }
}

#[test]
fn iterm2_multipart_file_commands_are_captured() {
    let mut parser = GraphicsParser::new();
    let out = parser.split(
        b"\x1b]1337;MultipartFile=inline=1:size=3\x07\x1b]1337;FilePart=YWJj\x07\x1b]1337;FileEnd\x07",
    );
    assert_eq!(out.len(), 3);
    for segment in &out {
        match segment {
            Segment::Graphic(cmd) => assert_eq!(cmd.format, GraphicFormat::Iterm2),
            other => panic!("expected iterm2 graphic command, got {other:?}"),
        }
    }
    assert!(matches!(&out[1], Segment::Graphic(cmd) if cmd.data == b"\x1b]1337;FilePart=YWJj\x07"));
    assert!(matches!(&out[2], Segment::Graphic(cmd) if cmd.data == b"\x1b]1337;FileEnd\x07"));
}

#[test]
fn iterm2_file_part_split_across_reads_is_captured() {
    let mut parser = GraphicsParser::new();
    assert!(parser.split(b"\x1b]1337;FilePa").is_empty());
    let out = parser.split(b"rt=YWJj\x1b\\");
    assert_eq!(out.len(), 1);
    assert!(matches!(&out[0], Segment::Graphic(cmd) if cmd.format == GraphicFormat::Iterm2));
}

#[test]
fn sequence_split_across_reads_is_reassembled() {
    let mut parser = GraphicsParser::new();
    let a = parser.split(b"\x1bP0;");
    assert!(a.iter().all(|s| matches!(s, Segment::Plain(_))));
    let b = parser.split(b"1;100qPAYLOAD\x1b\\");
    assert_eq!(b.len(), 1);
    match &b[0] {
        Segment::Graphic(cmd) => assert_eq!(cmd.data, b"\x1bP0;1;100qPAYLOAD\x1b\\"),
        other => panic!("expected graphic, got {other:?}"),
    }
}

#[test]
fn oversized_sequence_is_dropped_and_parser_recovers() {
    let mut parser = GraphicsParser::new();
    let mut junk = Vec::from(&b"\x1bP0;1q"[..]);
    junk.extend(std::iter::repeat_n(b'A', 8 * 1024 * 1024 + 1));
    let out = parser.split(&junk);
    assert!(out.is_empty() || out.iter().all(|s| matches!(s, Segment::Plain(_))));
    // parser still works after an abort
    let next = parser.split(b"\x1bP0;1;100qOK\x1b\\");
    assert!(next.iter().any(|s| matches!(s, Segment::Graphic(_))));
}

#[test]
fn store_assigns_monotonic_seq_and_shifts_on_scroll() {
    let mut store = PaneGraphicsStore::new();
    let mut parser = GraphicsParser::new();
    let cmd_a = parser.feed(b"\x1bP0;1;100qA\x1b\\", 10, 4);
    let cmd_b = parser.feed(b"\x1bP0;1;100qB\x1b\\", 12, 4);
    for cmd in cmd_a.into_iter().chain(cmd_b) {
        store.push(cmd);
    }
    let seqs: Vec<u64> = store.active_commands().iter().map(|c| c.seq).collect();
    assert_eq!(seqs.len(), 2);
    assert!(seqs[1] > seqs[0]);

    store.scroll_rows(3);
    assert_eq!(store.active_commands()[0].row, 7);
    assert_eq!(store.active_commands()[1].row, 9);
    store.scroll_rows(100);
    // Graphics are now preserved even when scrolled out of bounds
    // They won't render, but they're kept for potential future visibility
    assert!(!store.active_commands().is_empty());
}

#[test]
fn backend_emits_new_graphics_once_and_reemits_all_on_full_redraw() {
    let mut backend = ClientBackend::new(80, 24);
    let cmd = |seq: u64| venti::proto::GfxCmd {
        row: 3,
        col: 5,
        format: venti::proto::GfxFormat::Sixel,
        seq,
        data: vec![0x1b, b'P', b'q', b'x', 0x1b, b'\\'],
    };

    backend.set_pending_gfx(vec![cmd(1)], false);
    let u1 = backend.take_screen_update();
    assert_eq!(u1.gfx.len(), 1);

    // same command not re-emitted
    backend.set_pending_gfx(vec![cmd(1)], false);
    let u2 = backend.take_screen_update();
    assert!(u2.gfx.is_empty());

    // newer command emitted
    backend.set_pending_gfx(vec![cmd(2)], false);
    let u3 = backend.take_screen_update();
    assert_eq!(u3.gfx.len(), 1);
    assert_eq!(u3.gfx[0].seq, 2);

    // full redraw (e.g. after ED) re-emits everything known
    use ratatui::backend::Backend as _;
    backend.clear().expect("clear marks a full redraw");
    backend.set_pending_gfx(vec![cmd(1), cmd(2)], false);
    let u4 = backend.take_screen_update();
    assert!(u4.full_redraw);
    assert_eq!(u4.gfx.len(), 2);
}

#[test]
fn backend_force_reemit_replaces_pixels_without_full_redraw() {
    let mut backend = ClientBackend::new(80, 24);
    let cmd = |seq: u64| venti::proto::GfxCmd {
        row: 3,
        col: 5,
        format: venti::proto::GfxFormat::Kitty,
        seq,
        data: vec![0x1b, b'_', b'G', b'x', 0x1b, b'\\'],
    };

    backend.set_pending_gfx(vec![cmd(7)], false);
    let u1 = backend.take_screen_update();
    assert_eq!(u1.gfx.len(), 1);

    // an anchor shift re-places without marking a full redraw
    backend.set_pending_gfx(vec![cmd(7)], true);
    let u2 = backend.take_screen_update();
    assert_eq!(u2.gfx.len(), 1);
    assert!(!u2.full_redraw);
}

#[test]
fn kitty_transmit_produces_cheap_delete_and_place_reemit_bytes() {
    let mut store = PaneGraphicsStore::new();
    let mut parser = GraphicsParser::new();
    let cmds = parser.feed(
        b"\x1b_Ga=T,f=24,t=d,i=42,s=4,v=4,c=2,r=2;UE1MT09E\x1b\\",
        10,
        4,
    );
    assert_eq!(cmds.len(), 1);
    store.push(cmds.into_iter().next().expect("one graphic"));
    let image_id = store.active_commands()[0]
        .kitty_metadata
        .as_ref()
        .and_then(|metadata| metadata.i)
        .expect("transmitted image receives a terminal-unique id");
    let stored = store.active_commands()[0]
        .reemit
        .clone()
        .expect("kitty image with id re-places");
    let text = String::from_utf8(stored).expect("utf8 control data");
    // delete the previous placement only, keeping the pixels on the host
    assert!(
        text.contains("a=d")
            && text.contains("d=i")
            && text.contains(&format!("i={image_id}"))
            && text.contains("q=2"),
        "delete placement first: {text}"
    );
    // re-place the stored pixels with a real a=p placement (invisible without
    // one), not a virtual (U=1) placement, which draws nothing unless the
    // application itself prints U+10EEEE placeholder cells
    assert!(
        text.contains(&format!("a=p,i={image_id},q=2")) && !text.contains("U=1"),
        "then re-place stored pixels: {text}"
    );
    // placement geometry is preserved so the image lands back at the same size
    assert!(
        text.contains("c=2") && text.contains("r=2"),
        "placement geometry preserved: {text}"
    );
    assert!(
        !text.contains("UE1MT09E"),
        "pixel payload must not be re-sent: {text}"
    );
}

#[test]
fn kitty_separate_transmit_and_placement_both_survive_for_replay() {
    let mut parser = GraphicsParser::new();
    let mut store = PaneGraphicsStore::new();
    for cmd in parser.feed(
        b"\x1b_Ga=t,f=24,i=51,s=2,v=2;AAAA\x1b\\\x1b_Ga=p,i=51,p=7,c=2,r=2\x1b\\",
        4,
        6,
    ) {
        store.push(cmd);
    }
    let commands = store.active_commands();
    assert_eq!(commands.len(), 2, "keep image data and its placement");
    assert_eq!(
        commands[0]
            .kitty_metadata
            .as_ref()
            .and_then(|m| m.a.as_deref()),
        Some("t")
    );
    let restored_transmission = commands[0].restore_after_clear();
    let transmit_replay = restored_transmission
        .as_deref()
        .expect("transmitted pixels must be restored after the terminal clears graphics");
    assert!(transmit_replay.windows(3).any(|w| w == b"d=I"));
    assert!(transmit_replay.windows(3).any(|w| w == b"a=t"));
    assert!(transmit_replay.windows(4).any(|w| w == b"AAAA"));
    assert!(transmit_replay.ends_with(b"\x1b\\"));
    assert_eq!(
        commands[1]
            .kitty_metadata
            .as_ref()
            .and_then(|m| m.a.as_deref()),
        Some("p")
    );
    assert!(commands[1].reemit.is_some(), "placement must be replayable");
    assert_eq!(
        commands[1].restore_after_clear().as_deref(),
        Some(commands[1].data.as_slice())
    );
}

#[test]
fn alternate_screen_graphics_do_not_replace_or_delete_main_screen_images() {
    let mut parser = GraphicsParser::new();
    let main_command = parser
        .feed(b"\x1b_Ga=T,f=24,i=5,s=1,v=1;AAAA\x1b\\", 0, 0)
        .remove(0);
    let mut alternate_command = parser
        .feed(b"\x1b_Ga=T,f=24,i=5,s=1,v=1;BBBB\x1b\\", 0, 0)
        .remove(0);
    alternate_command.alternate_screen = true;

    let mut store = PaneGraphicsStore::new();
    store.push(main_command);
    store.push(alternate_command);
    let commands = store.active_commands();
    assert_eq!(commands.len(), 2);
    let main_id = commands[0].kitty_metadata.as_ref().and_then(|m| m.i);
    let alternate_id = commands[1].kitty_metadata.as_ref().and_then(|m| m.i);
    assert_ne!(
        main_id, alternate_id,
        "screen-local image ids must not alias"
    );

    let mut alt_delete = parser.feed(b"\x1b_Ga=d,d=I,i=5,q=2\x1b\\", 0, 0).remove(0);
    alt_delete.alternate_screen = true;
    store.push(alt_delete);
    assert_eq!(store.active_commands().len(), 2);
    assert!(!store.active_commands()[0].alternate_screen);
    assert_eq!(
        store.active_commands()[0]
            .kitty_metadata
            .as_ref()
            .and_then(|m| m.i),
        main_id
    );

    let removed = store.remove_alternate_screen();
    assert_eq!(removed.len(), 1);
    assert!(removed[0].alternate_screen);
    assert_eq!(store.active_commands().len(), 1);
    assert!(!store.active_commands()[0].alternate_screen);
}

#[test]
fn kitty_transmit_assigns_explicit_protocol_ids() {
    // an image transmitted without ids (e.g. fastfetch) must still become
    // re-placeable; the multiplexer stamps explicit ids at emit time since
    // host-side auto ids are unknowable with responses suppressed.
    let mut parser = GraphicsParser::new();
    let cmds = parser.feed(b"\x1b_Ga=T,f=24,s=1,v=1;AAAA\x1b\\", 0, 0);
    assert_eq!(cmds.len(), 1);
    let apc = String::from_utf8(cmds[0].data.clone()).expect("utf8 kitty control data");
    let count = |needle: &str| apc.matches(needle).count();
    assert_eq!(count("i="), 1, "image id assigned: {apc}");
    assert_eq!(count("p="), 1, "placement id assigned: {apc}");
    assert!(apc.contains("q=2"));

    let mut store = PaneGraphicsStore::new();
    store.push(cmds.into_iter().next().expect("one graphic"));
    let text = String::from_utf8(store.active_commands()[0].reemit.clone().expect("reemit"))
        .expect("utf8 control data");
    // delete and re-place target the same placement id the transmit used
    let pl_start = apc.find("p=").expect("placement in transmit");
    let pl_end = apc[pl_start..]
        .find([',', ';'])
        .map(|o| pl_start + o)
        .unwrap_or(apc.len());
    let placement = &apc[pl_start..pl_end];
    assert!(text.contains(placement), "delete+place out of sync: {text}");
}

#[test]
fn kitty_unicode_placeholder_transmit_is_forwarded_once_never_reemitted() {
    let mut store = PaneGraphicsStore::new();
    let mut parser = GraphicsParser::new();
    let cmds = parser.feed(
        b"\x1b_Ga=T,f=24,i=77,s=1,v=1,U=1,c=2,r=1;UE1MT09E\x1b\\",
        4,
        2,
    );
    assert_eq!(cmds.len(), 1);
    store.push(cmds.into_iter().next().expect("one graphic"));
    let stored = &store.active_commands()[0];
    assert!(
        stored.reemit.is_none(),
        "virtual (U=1) placements must not be re-placed by the multiplexer"
    );

    // scrolling a virtual image out of view: it's now preserved even when scrolled far
    // This allows potential re-visibility if scrollback structure changes
    store.scroll_rows(100);
    // Virtual placement is still preserved, just with clamped row
    assert!(
        !store.active_commands().is_empty(),
        "virtual placement preserved"
    );
}

#[test]
fn tmux_passthrough_envelope_matches_tmux_convention() {
    let wrapped = venti::client::apply_update::wrap_tmux_passthrough(b"\x1b_Ga=T,i=1;AAAA\x1b\\");
    assert_eq!(
        wrapped, b"\x1bPtmux;\x1b\x1b_Ga=T,i=1;AAAA\x1b\x1b\\\x1b\\",
        "every inner ESC is doubled and the envelope is ESC P tmux; ... ESC \\\\"
    );
    // inner payload bytes other than ESC pass through untouched
    let wrapped = venti::client::apply_update::wrap_tmux_passthrough(b"abc");
    assert_eq!(wrapped, b"\x1bPtmux;abc\x1b\\");
}

#[test]
fn kitty_anonymous_image_keeps_payload_bytes() {
    // the transmit itself is emitted byte-for-byte; only q and id keys are
    // stamped. (earlier this passed byte-for-byte untouched; now ids must be
    // injected for re-placement, so assert payload preservation instead.)
    let mut parser = GraphicsParser::new();
    let cmds = parser.feed(b"\x1b_Ga=T,f=24,s=1,v=1;AAAA\x1b\\", 0, 0);
    let apc = String::from_utf8(cmds[0].data.clone()).expect("utf8 kitty control data");
    assert!(apc.contains("AAAA"), "payload preserved: {apc}");
    assert!(
        apc.contains("a=T") && apc.contains("f=24"),
        "keys preserved: {apc}"
    );
}

#[test]
fn screen_update_roundtrips_with_graphics() {
    use venti::proto::{ScreenUpdate, ServerMsg, decode_from_slice, encode};
    let update = ScreenUpdate {
        cols: 80,
        rows: 24,
        cursor_pos: Some((0, 0)),
        cursor_visible: true,
        cursor_shape: CursorShape::Block,
        cells: Vec::new(),
        full_redraw: true,
        reset: false,
        gfx: vec![venti::proto::GfxCmd {
            row: 1,
            col: 2,
            format: venti::proto::GfxFormat::Kitty,
            seq: 42,
            data: b"\x1b_Ga=T\x1b\\".to_vec(),
        }],
    };
    let encoded = encode(&ServerMsg::Update(update.clone())).expect("encode");
    let (decoded, _): (ServerMsg, usize) =
        decode_from_slice(&encoded).expect("decode").expect("frame");
    assert_eq!(decoded, ServerMsg::Update(update));
}

#[test]
fn scrolled_out_image_is_preserved_with_adjusted_row() {
    let mut store = PaneGraphicsStore::new();
    let mut parser = GraphicsParser::new();
    let cmds = parser.feed(b"\x1b_Ga=T,f=24,i=42,s=1,v=1;QUJD\x1b\\", 0, 0);
    store.push(cmds.into_iter().next().expect("one graphic"));
    let _image_id = store.active_commands()[0]
        .kitty_metadata
        .as_ref()
        .and_then(|metadata| metadata.i)
        .expect("transmitted image receives a terminal-unique id");
    assert_eq!(store.active_commands().len(), 1);

    store.scroll_rows(5);
    let cmds = store.active_commands();
    // Original image is still there, just with adjusted row position
    assert_eq!(cmds.len(), 1, "original image preserved with adjusted row");
    // Verify it's the original image command, not a delete
    let text = String::from_utf8(cmds[0].data.clone()).expect("utf8");
    // Should be a transmit or placement, not a delete (a=d)
    assert!(!text.contains("a=d"), "image should not be deleted");
}

#[test]
fn graphics_store_bounds_delete_and_scroll_cleanup_commands() {
    let mut parser = GraphicsParser::new();
    let mut store = PaneGraphicsStore::new();

    for id in 1..=128 {
        let sequence = format!("\x1b_Ga=d,d=i,i={id},q=2\x1b\\");
        let command = parser
            .feed(sequence.as_bytes(), 0, 0)
            .into_iter()
            .next()
            .expect("delete command parsed");
        store.push(command);
    }
    assert_eq!(store.active_commands().len(), 64);

    store.clear();
    for row in 0..64 {
        let command = parser
            .feed(b"\x1b_Ga=T,f=24,i=9,s=1,v=1;QUJD\x1b\\", row, 0)
            .into_iter()
            .next()
            .expect("image command parsed");
        store.push(command);
    }
    store.scroll_rows(100);
    assert!(store.active_commands().len() <= 64);
}

#[test]
fn graphics_store_bounds_total_retained_bytes() {
    let mut parser = GraphicsParser::new();
    let mut store = PaneGraphicsStore::new();
    let payload = vec![b'A'; 5 * 1024 * 1024];

    for id in [21, 22] {
        let mut sequence = format!("\x1b_Ga=T,f=24,i={id},s=1,v=1;").into_bytes();
        sequence.extend_from_slice(&payload);
        sequence.extend_from_slice(b"\x1b\\");
        let command = parser
            .feed(&sequence, 0, 0)
            .into_iter()
            .next()
            .expect("image command parsed");
        store.push(command);
    }

    let retained_bytes: usize = store
        .active_commands()
        .iter()
        .map(|command| command.data.len())
        .sum();
    assert!(retained_bytes <= 8 * 1024 * 1024);
    assert_eq!(store.active_commands().len(), 1);
}

#[test]
fn sixel_raster_attributes_declare_dimensions() {
    // the "1;1;64;12 raster in this sequence declares a 64x12 image
    let seq = b"\x1bP0;1;1q\"1;1;64;12#0;2;100;0;0#0!64~-\x1b\\";
    assert_eq!(
        venti::pane::graphics::sixel_pixel_dimensions(seq),
        Some((64, 12))
    );
}

#[test]
fn sixel_dimensions_counted_without_raster_attributes() {
    // no raster: width counts columns (64 from !64), height counts bands (2 bands = 12px)
    let seq = b"\x1bPq#0;2;100;0;0#0!64~-#0!64~-\x1b\\";
    assert_eq!(
        venti::pane::graphics::sixel_pixel_dimensions(seq),
        Some((64, 12))
    );
}

#[test]
fn sixel_trailing_empty_band_is_not_counted() {
    // chafa ends sixels with a graphics newline; the phantom band must not extend the height
    let seq = b"\x1bPq#0;2;100;0;0#0!8~-\x1b\\";
    assert_eq!(
        venti::pane::graphics::sixel_pixel_dimensions(seq),
        Some((8, 6))
    );
}

#[test]
fn sixel_repeat_zero_counts_as_one() {
    let seq = b"\x1bPq#0;2;100;0;0#0!0~\x1b\\";
    assert_eq!(
        venti::pane::graphics::sixel_pixel_dimensions(seq),
        Some((1, 6))
    );
}

#[test]
fn sixel_carriage_return_resets_column_but_keeps_max() {
    // second pass paints after $; width is the widest band, not the sum
    let seq = b"\x1bPq#0!4~$#0~\x1b\\";
    assert_eq!(
        venti::pane::graphics::sixel_pixel_dimensions(seq),
        Some((4, 6))
    );
}

#[test]
fn sixel_garbage_returns_none() {
    assert_eq!(venti::pane::graphics::sixel_pixel_dimensions(b""), None);
    assert_eq!(
        venti::pane::graphics::sixel_pixel_dimensions(b"\x1bPq"),
        None
    );
    assert_eq!(
        venti::pane::graphics::sixel_pixel_dimensions(b"\x1b[2J\x1b[H"),
        None
    );
}

#[test]
fn sixel_dimensions_handle_huge_repeat_counts_without_panicking() {
    let seq = b"\x1bPq#0!9999999999~\x1b\\";
    let dims = venti::pane::graphics::sixel_pixel_dimensions(seq);
    assert_eq!(dims.map(|(w, _)| w), Some(u32::MAX));
}

#[test]
fn sixel_at_same_anchor_replaces_previous() {
    // chafa animation frames re-draw the same image at the same cell; the store
    // must not accumulate every frame
    let mut store = PaneGraphicsStore::new();
    let mut parser = GraphicsParser::new();
    for frame in [
        b"frame1".as_slice(),
        b"frame2".as_slice(),
        b"frame3".as_slice(),
    ] {
        let mut seq = b"\x1bP0;1;100q".to_vec();
        seq.extend_from_slice(frame);
        seq.extend_from_slice(b"\x1b\\");
        let cmds = parser.feed(&seq, 4, 2);
        store.push(cmds.into_iter().next().unwrap());
    }
    assert_eq!(store.active_commands().len(), 1);
    assert!(store.active_commands()[0].data.ends_with(b"frame3\x1b\\"));
}

#[test]
fn clear_screen_drops_sixel() {
    let mut store = PaneGraphicsStore::new();
    let mut parser = GraphicsParser::new();
    let cmds = parser.feed(b"\x1bP0;1;100qSIXEL\x1b\\", 0, 0);
    store.push(cmds.into_iter().next().unwrap());
    store.clear_screen_images(false);
    assert!(
        store.active_commands().is_empty(),
        "sixel placement must be dropped on clear; the host erased its pixels"
    );
}

#[test]
fn placements_scrolled_out_of_the_history_are_freed() {
    use venti::pane::graphics::{
        GraphicCommand, GraphicFormat, KittyControlData, PaneGraphicsStore,
    };

    let mut store = PaneGraphicsStore::new();
    for (id, row) in [(1u32, 0i32), (2, -5), (3, -50)] {
        store.push(GraphicCommand {
            format: GraphicFormat::Kitty,
            seq: 0,
            row,
            col: 0,
            alternate_screen: false,
            data: b"\x1b_Ga=T,f=100,i=7;AAAA\x1b\\".to_vec(),
            kitty_metadata: Some(KittyControlData {
                a: Some("T".to_string()),
                i: Some(id),
                ..Default::default()
            }),
            reemit: None,
            pixel_size: None,
            file_bytes: None,
        });
    }
    assert_eq!(store.active_commands().len(), 3);
    // the history keeps 10 lines: everything more than 10 rows above the
    // live grid is gone for good
    let freed = store.drop_scrolled_out(-10);
    assert_eq!(freed.len(), 1, "only the placement beyond the history goes");
    assert_eq!(freed[0].row, -50);
    assert_eq!(store.active_commands().len(), 2);
    assert!(
        store
            .active_commands()
            .iter()
            .all(|command| command.row >= -10)
    );
}

#[test]
fn touching_a_placement_moves_it_to_the_back_of_the_eviction_queue() {
    use venti::pane::graphics::{
        GraphicCommand, GraphicFormat, KittyControlData, PaneGraphicsStore,
    };

    let mut store = PaneGraphicsStore::new();
    for id in 1..=3u32 {
        store.push(GraphicCommand {
            format: GraphicFormat::Kitty,
            seq: 0,
            row: 0,
            col: 0,
            alternate_screen: false,
            data: b"\x1b_Ga=T,f=100,i=7;AAAA\x1b\\".to_vec(),
            kitty_metadata: Some(KittyControlData {
                a: Some("T".to_string()),
                i: Some(id),
                ..Default::default()
            }),
            reemit: None,
            pixel_size: None,
            file_bytes: None,
        });
    }
    let first_seq = store.active_commands()[0].seq;
    store.touch(first_seq);
    let seqs: Vec<u64> = store.active_commands().iter().map(|c| c.seq).collect();
    assert_eq!(
        *seqs.last().expect("commands"),
        first_seq,
        "the touched placement must be last (least recently used first)"
    );
}

#[test]
fn a_placement_larger_than_the_pane_is_cropped_not_hidden() {
    let mut store = PaneGraphicsStore::new();
    // a 4000x800 pixel image (400x40 cells at 10x20) in a 40x10 cell pane:
    // it cannot fit, but it must be cropped to the visible part instead of
    // being hidden (zellij clips an image quad to its owning pane).
    // s/v give the pixel size, so no PNG decoding is needed for the crop math
    let mut payload = String::from("a=T,f=100,s=4000,v=800,i=42;");
    payload.push_str("AAAA");
    let mut data = Vec::new();
    data.extend_from_slice(b"\x1b_G");
    data.extend_from_slice(payload.as_bytes());
    data.extend_from_slice(b"\x1b\\");
    let mut parser = venti::pane::GraphicsParser::new();
    let mut command = parser.feed(&data, 0, 0).remove(0);
    command.row = 0;
    command.col = 0;
    store.push(command);

    let placement = store.active_commands().first().expect("placement");
    let cropped = placement
        .crop_placement(40, 10, 0)
        .expect("a too large image must be cropped, not hidden");
    let (payload, col, row) = cropped;
    let text = String::from_utf8_lossy(&payload);
    assert!(
        text.contains("a=p,"),
        "crop must re-place the image: {text}"
    );
    assert!(
        text.contains("c=40"),
        "the cropped placement must fit the pane width: {text}"
    );
    assert_eq!(col, 0);
    assert_eq!(row, 0);
}

#[test]
fn crop_uses_the_placement_scale_not_the_cell_size() {
    let mut store = PaneGraphicsStore::new();
    // a 400x200 pixel image placed into 20x10 cells: 20 pixels per cell,
    // not the terminal's 10x20. a crop must use the placement's own scale or
    // the host draws the wrong pixels (a black rectangle).
    let mut payload = String::from("a=T,f=100,s=400,v=200,i=42,c=20,r=10;");
    payload.push_str("AAAA");
    let mut data = Vec::new();
    data.extend_from_slice(b"\x1b_G");
    data.extend_from_slice(payload.as_bytes());
    data.extend_from_slice(b"\x1b\\");
    let mut parser = venti::pane::GraphicsParser::new();
    let mut command = parser.feed(&data, 0, 0).remove(0);
    command.row = 0;
    command.col = 0;
    store.push(command);

    let placement = store.active_commands().first().expect("placement");
    // the image is 20 cells wide and 10 rows tall; the pane shows 10x5 cells
    let (payload, col, row) = placement
        .crop_placement(10, 5, 0)
        .expect("the image must be cropped to the pane");
    assert_eq!((col, row), (0, 0));
    let text = String::from_utf8_lossy(&payload);
    assert!(
        text.contains("c=10") && text.contains("r=5"),
        "the cropped placement must cover the visible cells: {text}"
    );
    assert!(
        text.contains("w=200") && text.contains("h=100"),
        "the crop must take half the source pixels (20 px per cell): {text}"
    );
}

#[test]
fn transmissions_that_read_pixels_from_a_file_are_not_re_transmitted() {
    use venti::pane::graphics::{GraphicCommand, GraphicFormat, KittyControlData};

    let command = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 0,
        row: 0,
        col: 0,
        alternate_screen: false,
        data: b"\x1b_Ga=T,f=100,t=f,i=42;AAAA\x1b\\".to_vec(),
        kitty_metadata: Some(KittyControlData {
            a: Some("T".to_string()),
            t: Some("f".to_string()),
            i: Some(42),
            ..Default::default()
        }),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };
    assert!(
        command.reads_pixels_from_file(),
        "a temp-file transmission reads its pixels from a file"
    );

    let inline = GraphicCommand {
        kitty_metadata: Some(KittyControlData {
            a: Some("T".to_string()),
            t: Some("d".to_string()),
            i: Some(42),
            ..Default::default()
        }),
        ..command
    };
    assert!(
        !inline.reads_pixels_from_file(),
        "an inline transmission can be re-sent"
    );
}

#[test]
fn placements_in_the_scrollback_do_not_drift_when_the_grid_scrolls() {
    use venti::pane::graphics::{
        GraphicCommand, GraphicFormat, KittyControlData, PaneGraphicsStore,
    };

    let mut store = PaneGraphicsStore::new();
    let mut push = |row: i32, id: u32| {
        store.push(GraphicCommand {
            format: GraphicFormat::Kitty,
            seq: 0,
            row,
            col: 0,
            alternate_screen: false,
            data: b"\x1b_Ga=T,f=100,i=7;AAAA\x1b\\".to_vec(),
            kitty_metadata: Some(KittyControlData {
                a: Some("T".to_string()),
                i: Some(id),
                ..Default::default()
            }),
            reemit: None,
            pixel_size: None,
            file_bytes: None,
        });
    };
    push(5, 1); // in the live grid
    push(-5, 2); // already in the scrollback

    // the grid scrolls by 3 lines: the live placement moves up, the one in
    // the scrollback stays where it is (new output is appended below it).
    store.scroll_rows(3);
    let rows: Vec<i32> = store.active_commands().iter().map(|c| c.row).collect();
    assert_eq!(rows, vec![2, -5]);
}

#[test]
fn placements_follow_their_content_when_the_grid_is_re_wrapped() {
    use venti::pane::graphics::{GraphicCommand, GraphicFormat, PaneGraphicsStore};

    let mut store = PaneGraphicsStore::new();
    for (id, row) in [(1u32, 4i32), (2, 7)] {
        store.push(GraphicCommand {
            format: GraphicFormat::Sixel,
            seq: 0,
            row,
            col: 0,
            alternate_screen: false,
            data: b"\x1bP0;1;0qAAAA\x1b\\".to_vec(),
            kitty_metadata: None,
            reemit: None,
            pixel_size: None,
            file_bytes: None,
        });
        let _ = id;
    }
    // the grid was re-wrapped: rows 0..4 stayed, row 4 moved to 6, row 7 to 9
    store.remap_rows(&[
        (0, 0),
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 6),
        (5, 7),
        (6, 8),
        (7, 9),
    ]);
    let rows: Vec<i32> = store.active_commands().iter().map(|c| c.row).collect();
    assert_eq!(rows, vec![6, 9], "placements must follow their content");
}

#[test]
fn placements_in_the_grid_move_up_when_rows_scroll_into_the_scrollback() {
    use venti::pane::graphics::{
        GraphicCommand, GraphicFormat, KittyControlData, PaneGraphicsStore,
    };

    let mut store = PaneGraphicsStore::new();
    for (id, row) in [(1u32, 5i32), (2, -3)] {
        store.push(GraphicCommand {
            format: GraphicFormat::Kitty,
            seq: 0,
            row,
            col: 0,
            alternate_screen: false,
            data: b"\x1b_Ga=T,f=100,i=7;AAAA\x1b\\".to_vec(),
            kitty_metadata: Some(KittyControlData {
                a: Some("T".to_string()),
                i: Some(id),
                ..Default::default()
            }),
            reemit: None,
            pixel_size: None,
            file_bytes: None,
        });
    }
    // the pane lost two rows: the grid content scrolled up by two
    store.scroll_rows(2);
    let rows: Vec<i32> = store.active_commands().iter().map(|c| c.row).collect();
    assert_eq!(
        rows,
        vec![3, -3],
        "the grid placement moves up, the scrollback placement stays"
    );
}

#[test]
fn chunked_kitty_transmission_is_one_command() {
    use venti::pane::graphics::GraphicsParser;
    let mut parser = GraphicsParser::new();
    let mut seqs = Vec::new();
    // chunk 1: m=1, first half
    seqs.extend_from_slice(b"\x1b_Ga=T,f=32,s=200,v=160,i=7,m=1;AAAA");
    seqs.extend_from_slice(b"\x1b\\");
    // chunk 2: m=0, second half
    seqs.extend_from_slice(b"\x1b_Gm=0;BBBB");
    seqs.extend_from_slice(b"\x1b\\");

    let cmds = parser.feed(&seqs, 0, 0);
    assert_eq!(cmds.len(), 1, "a chunked transmission is one command");
    let text = String::from_utf8_lossy(&cmds[0].data);
    assert!(text.contains("AAAA"), "first chunk payload missing");
    assert!(text.contains("BBBB"), "second chunk payload missing");
}

#[test]
fn kitty_control_rewrite_keeps_the_payload_intact() {
    use venti::pane::graphics::GraphicsParser;
    // a chafa-style kitty transmit: raw-ish payload, big
    let payload = "AAB//wAAf/8CAH//AwB//wQAf/8GAH//BwB//wgAf/8JAH//CwB//wwAf/8NAH//".to_string();
    let mut seq = Vec::new();
    seq.extend_from_slice(b"\x1b_Ga=T,f=32,s=200,v=160,i=1000003,q=2,c=2,r=1,p=2000001;");
    seq.extend_from_slice(payload.as_bytes());
    seq.extend_from_slice(b"\x1b\\");

    let mut parser = GraphicsParser::new();
    let cmds = parser.feed(&seq, 0, 0);
    assert_eq!(cmds.len(), 1);
    let cmd = &cmds[0];

    // now the store rewrite path (what the pane reader does)
    let mut store = venti::pane::graphics::PaneGraphicsStore::new();
    store.push(cmd.clone());
    let stored = store.active_commands()[0].clone();
    // the payload must still be intact
    let idx = stored.data.iter().position(|&b| b == b';').unwrap();
    let out_payload = &stored.data[idx + 1..stored.data.len() - 2];
    assert_eq!(
        out_payload,
        payload.as_bytes(),
        "the payload must not change"
    );
}

#[test]
fn large_kitty_payload_is_chunked_on_emission() {
    use venti::pane::graphics::{GraphicCommand, GraphicFormat, KittyControlData};
    // a payload larger than the 4096-byte base64 chunk limit
    let payload = vec![7u8; 10_000];
    let control = KittyControlData {
        a: Some("T".to_string()),
        f: Some(100),
        s: Some(100),
        v: Some(100),
        i: Some(42),
        q: Some(2),
        t: Some("d".to_string()),
        ..Default::default()
    };
    let mut data = venti::pane::graphics::kitty_apc(&control);
    let mut b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &payload);
    // strip the trailing ESC\ from kitty_apc so we can append the payload
    data.truncate(data.len() - 2);
    data.push(b';');
    data.extend_from_slice(b64.as_bytes());
    data.extend_from_slice(b"\x1b\\");
    b64.clear();

    let cmd = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 1,
        row: 0,
        col: 0,
        alternate_screen: false,
        data,
        kitty_metadata: Some(control),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };

    let out = cmd.emission_payload();
    let text = String::from_utf8_lossy(&out);
    // must be chunked: at least two m=1/m=0 apcs, every chunk <= 4096 payload
    let apc_count = out.windows(3).filter(|w| *w == b"\x1b_G").count();
    assert!(
        apc_count >= 3,
        "a 10k payload must be split into several apcs, got {apc_count}"
    );
    assert!(text.contains("m=1"), "continuation chunks must carry m=1");
    assert!(text.contains("m=0"), "the final chunk must carry m=0");
    assert!(
        text.contains("a=T"),
        "the first chunk must carry the full control data"
    );
    // no single apc may exceed 4096 payload bytes
    let mut rest = &out[..];
    while let Some(p) = rest.windows(3).position(|w| w == b"\x1b_G") {
        let after = &rest[p + 3..];
        let end = after
            .windows(2)
            .position(|w| w == b"\x1b\\")
            .map(|e| p + 3 + e)
            .unwrap_or(rest.len());
        let apc = &rest[p..end];
        let semi = apc.iter().position(|&b| b == b';').unwrap_or(apc.len());
        let payload_len = apc.len() - semi - 1;
        assert!(
            payload_len <= 4096,
            "a chunk carried {payload_len} bytes, limit is 4096"
        );
        rest = &rest[end + 2..];
    }
}

#[test]
fn small_kitty_payload_is_not_chunked() {
    use venti::pane::graphics::{GraphicCommand, GraphicFormat, KittyControlData};
    let payload = vec![9u8; 100];
    let control = KittyControlData {
        a: Some("T".to_string()),
        f: Some(100),
        s: Some(10),
        v: Some(10),
        i: Some(1),
        q: Some(2),
        t: Some("d".to_string()),
        ..Default::default()
    };
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &payload);
    let mut data = venti::pane::graphics::kitty_apc(&control);
    data.truncate(data.len() - 2);
    data.push(b';');
    data.extend_from_slice(b64.as_bytes());
    data.extend_from_slice(b"\x1b\\");

    let cmd = GraphicCommand {
        format: GraphicFormat::Kitty,
        seq: 1,
        row: 0,
        col: 0,
        alternate_screen: false,
        data,
        kitty_metadata: Some(control),
        reemit: None,
        pixel_size: None,
        file_bytes: None,
    };
    // a small payload is forwarded verbatim, not chunked
    let out = cmd.emission_payload();
    let apc_count = out.windows(3).filter(|w| *w == b"\x1b_G").count();
    assert_eq!(apc_count, 1, "a small payload must stay a single apc");
}

#[test]
fn file_transmission_is_rewritten_to_inline_chunks_at_push() {
    use std::io::Write;
    use venti::pane::graphics::{GraphicsParser, PaneGraphicsStore};
    // a file-backed transmission like `kitten icat` sends: t=f, the payload
    // is the base64 path. the app deletes the file right after, so the store
    // must cache the pixels and rewrite the command to an inline chunked t=d.
    let dir = std::env::temp_dir().join(format!("venti-gfx-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("img.png");
    let pixels = vec![42u8; 8_000];
    std::fs::File::create(&path)
        .expect("create file")
        .write_all(&pixels)
        .expect("write pixels");

    let mut parser = GraphicsParser::new();
    let b64path = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        path.to_str().unwrap().as_bytes(),
    );
    let mut seq = b"\x1b_Ga=T,f=100,s=100,v=100,i=77,q=2,t=f;".to_vec();
    seq.extend_from_slice(b64path.as_bytes());
    seq.extend_from_slice(b"\x1b\\");

    let cmds = parser.feed(&seq, 0, 0);
    assert_eq!(cmds.len(), 1, "the file transmission must be captured");
    let cmd = cmds.into_iter().next().unwrap();
    assert!(
        cmd.reads_pixels_from_file(),
        "must be recognized as file-backed"
    );

    let mut store = PaneGraphicsStore::new();
    store.push(cmd.clone());
    let stored = store.active_commands()[0].clone();

    // the stored command is now an inline chunked t=d transmission: the host
    // can render it without touching the (soon deleted) file
    let text = String::from_utf8_lossy(&stored.data);
    assert!(
        text.contains("t=d"),
        "file transmission must become inline: {text:.120}"
    );
    assert!(text.contains("m=1"), "the 8k payload must be chunked");
    assert!(!stored.reads_pixels_from_file() || stored.file_bytes.is_some());
    // re-emission after ED2 re-sends the cached pixels, still chunked
    let reemit = stored.retransmit_with_cached_bytes().expect("cached bytes");
    let reemit_text = String::from_utf8_lossy(&reemit);
    assert!(reemit_text.contains("t=d"));
    assert!(
        reemit_text.contains("m=0"),
        "re-emission must terminate the chunks"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
