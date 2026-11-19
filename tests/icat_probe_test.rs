//! kitty icat's support probe goes through the pane cleanly: the pane's
//! local query responder must answer the probe without hanging.
use venti::pane::graphics::GraphicsParser;
use venti::pane::query::{QueryResponder, ScreenState};
use venti::util::outer_term::OuterGraphics;

/// feeds `bytes` into the split path and returns whatever the local query
/// responder wrote back to the pane (i.e. what the app would see).
fn answer_for(responder: &mut QueryResponder, bytes: &[u8]) -> Vec<u8> {
    let mut parser = GraphicsParser::new();
    let segs = parser.split(bytes);
    let mut plain = Vec::new();
    for seg in segs {
        if let venti::pane::graphics::Segment::Plain(p) = seg {
            plain.extend_from_slice(&p);
        }
    }
    let mut out = Vec::new();
    responder
        .process_queries(
            &plain,
            &mut out,
            Some(ScreenState {
                cursor_row: 0,
                cursor_col: 0,
                rows: 24,
                cols: 80,
            }),
        )
        .expect("query response write");
    out
}

#[test]
fn icat_direct_probe_is_answered_on_kitty_outer() {
    let mut responder = QueryResponder::for_outer_terminal("xterm-kitty", OuterGraphics::Kitty);
    let ok = answer_for(
        &mut responder,
        b"\x1b_Gi=77,s=1,v=1,a=q,t=d,f=24;MTIz\x1b\\",
    );
    assert!(
        ok.starts_with(b"\x1b_Gi=77,t=d;OK\x1b\\") || ok.starts_with(b"\x1b_Gi=77;OK\x1b\\"),
        "kitty probe must get OK, got: {ok:?}"
    );
}

#[test]
fn icat_direct_probe_gets_enoent_on_non_kitty_outer() {
    let mut responder = QueryResponder::for_outer_terminal("foot", OuterGraphics::Sixel);
    let out = answer_for(
        &mut responder,
        b"\x1b_Gi=77,s=1,v=1,a=q,t=d,f=24;MTIz\x1b\\",
    );
    assert!(
        out.starts_with(b"\x1b_Gi=77;ENOENT"),
        "non-kitty probe must get ENOENT, got: {out:?}"
    );
}

#[test]
fn icat_tempfile_probe_is_answered() {
    let mut responder = QueryResponder::for_outer_terminal("xterm-kitty", OuterGraphics::Kitty);
    // icat sends the temp-file probe with t=t and the path base64-encoded
    let ok = answer_for(
        &mut responder,
        b"\x1b_Gi=78,s=1,v=1,a=q,t=t,f=24;dGVtcGZvby50eDc=\x1b\\",
    );
    assert!(
        ok.starts_with(b"\x1b_Gi=78,t=t;OK\x1b\\") || ok.starts_with(b"\x1b_Gi=78;OK\x1b\\"),
        "tempfile probe must get OK, got: {ok:?}"
    );
}

#[test]
fn icat_sharedmem_probe_is_answered() {
    let mut responder = QueryResponder::for_outer_terminal("xterm-kitty", OuterGraphics::Kitty);
    // icat sends the shm probe with t=s
    let ok = answer_for(
        &mut responder,
        b"\x1b_Gi=79,s=1,v=1,a=q,t=s,f=24;c2htXzE2MA==\x1b\\",
    );
    assert!(
        ok.starts_with(b"\x1b_Gi=79,t=s;OK\x1b\\") || ok.starts_with(b"\x1b_Gi=79;OK\x1b\\"),
        "shared memory probe must get OK, got: {ok:?}"
    );
}

#[test]
fn icat_da1_query_sees_both_graphics_bits() {
    let mut responder = QueryResponder::for_outer_terminal("xterm-kitty", OuterGraphics::Kitty);
    let mut out = Vec::new();
    responder
        .process_queries(b"\x1b[c", &mut out, None)
        .expect("query response write");
    let da1 = String::from_utf8_lossy(&out);
    assert!(
        da1.contains("52") && da1.contains("22"),
        "DA1 must advertise Kitty graphics (52) and base (22), got: {da1}"
    );
}

#[test]
fn kitty_animation_feature_probes_are_answered() {
    let mut responder = QueryResponder::for_outer_terminal("xterm-kitty", OuterGraphics::Kitty);
    for (id, action) in [(80, "f"), (81, "a"), (82, "c")] {
        let query = format!("\x1b_Ga=q,i={id},e={action}\x1b\\");
        let out = answer_for(&mut responder, query.as_bytes());
        assert_eq!(
            out,
            format!("\x1b_Gi={id},e={action};OK\x1b\\").as_bytes(),
            "Kitty should report its forwarded {action} action"
        );
    }
}

#[test]
fn real_pane_feed_route_probes_as_plain() {
    // the exact bytes yazi sends for the file-based probe must pass through
    // the split pipeline as plain so the responder can see them
    let mut parser = GraphicsParser::new();
    let segs = parser.split(b"\x1b_Gi=99,a=q,t=f,i=99;dGVtcA==\x1b\\");
    for seg in &segs {
        assert!(
            matches!(seg, venti::pane::graphics::Segment::Plain(_)),
            "a=q must not become a graphic command: {seg:?}"
        );
    }
}
