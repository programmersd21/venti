use venti::pane::query::{QueryResponder, ScreenState};

#[test]
fn test_da1_omits_sixel_when_unsupported() {
    let mut responder = QueryResponder::with_graphics_support(false, false);
    let mut out = Vec::new();
    responder.process_queries(b"\x1b[c", &mut out, None);
    assert_eq!(out, b"\x1b[?63;1;2;22c");
}

#[test]
fn test_kitty_query_answered_with_error_when_unsupported() {
    let mut responder = QueryResponder::with_graphics_support(false, false);
    let mut out = Vec::new();
    responder.process_queries(b"\x1b_Gi=1,a=q;AAAA\x1b\\", &mut out, None);
    assert!(out.starts_with(b"\x1b_Gi=1;ENOENT"), "out: {out:?}");
}

#[test]
fn test_query_responder_device_attributes() {
    let mut responder = QueryResponder::with_graphics_support(true, true);
    let mut out = Vec::new();

    // Split across chunk boundary; reports sixel + kitty when supported
    responder.process_queries(b"\x1b[", &mut out, None);
    assert!(out.is_empty());
    responder.process_queries(b"c", &mut out, None);
    assert_eq!(out, b"\x1b[?63;1;2;4;52;22c");
}

#[test]
fn da1_advertises_kitty_graphics_bit_when_supported() {
    let mut responder = QueryResponder::with_graphics_support(false, true);
    let mut out = Vec::new();
    responder.process_queries(b"\x1b[c", &mut out, None);
    assert_eq!(out, b"\x1b[?63;1;2;52;22c");
}

#[test]
fn xtversion_uses_attached_foot_terminal_identity() {
    let mut responder =
        QueryResponder::for_outer_terminal("foot", venti::util::outer_term::OuterGraphics::Sixel);
    let mut out = Vec::new();

    responder.process_queries(b"\x1b[>q", &mut out, None);

    assert!(out.starts_with(b"\x1bP>|foot("), "out: {out:?}");
}

#[test]
fn test_query_responder_status_and_cursor() {
    let mut responder = QueryResponder::new();
    let mut out = Vec::new();

    responder.process_queries(b"\x1b[5n", &mut out, None);
    assert_eq!(out, b"\x1b[0n");

    out.clear();
    responder.process_queries(b"\x1b[6n", &mut out, None);
    assert_eq!(out, b"\x1b[1;1R");
}

#[test]
fn test_query_responder_reports_actual_cursor_and_size() {
    let mut responder = QueryResponder::new();
    let screen = Some(ScreenState {
        cursor_row: 4,
        cursor_col: 9,
        rows: 30,
        cols: 100,
    });
    let mut out = Vec::new();

    responder.process_queries(b"\x1b[6n", &mut out, screen);
    assert_eq!(out, b"\x1b[5;10R");

    out.clear();
    responder.process_queries(b"\x1b[18t", &mut out, screen);
    assert_eq!(out, b"\x1b[8;30;100t");
}

#[test]
fn test_kitty_query_only_answers_explicit_queries() {
    let mut responder = QueryResponder::with_graphics_support(false, true);
    let mut out = Vec::new();

    // a=t (transmit) must not be answered
    responder.process_queries(b"\x1b_Ga=t,i=42;\x1b\\", &mut out, None);
    assert!(out.is_empty());

    // a=q (query) is answered with the image id
    responder.process_queries(b"\x1b_Ga=q,i=42;\x1b\\", &mut out, None);
    assert_eq!(out, b"\x1b_Gi=42;OK\x1b\\");

    out.clear();
    // an id containing escape bytes must not be echoed
    responder.process_queries(b"\x1b_Ga=q,i=42\x1b[31m;\x1b\\", &mut out, None);
    assert!(out.is_empty());
}

#[test]
fn kitty_query_only_reports_features_for_supported_feature_actions() {
    let mut responder = QueryResponder::with_graphics_support(false, true);
    let mut out = Vec::new();

    responder.process_queries(b"\x1b_Ga=q,i=42,C=1,t=d;\x1b\\", &mut out, None);
    assert_eq!(out, b"\x1b_Gi=42;OK\x1b\\");

    out.clear();
    responder.process_queries(b"\x1b_Ga=q,i=43,e=f,C=1,t=d;\x1b\\", &mut out, None);
    assert_eq!(out, b"\x1b_Gi=43;OK\x1b\\");

    out.clear();
    responder.process_queries(b"\x1b_Ga=q,i=44,e=T,U=1,C=1,t=d;\x1b\\", &mut out, None);
    assert_eq!(out, b"\x1b_Gi=44,e=T,U=1,C=1;OK\x1b\\");
}

#[test]
fn kitty_feature_query_requires_a_valid_nonzero_image_id() {
    let mut responder = QueryResponder::with_graphics_support(false, true);
    let mut out = Vec::new();

    responder.process_queries(b"\x1b_Ga=q,e=T,U=1;\x1b\\", &mut out, None);
    responder.process_queries(b"\x1b_Ga=q,i=0,e=T,U=1;\x1b\\", &mut out, None);
    responder.process_queries(b"\x1b_Ga=q,i=oops,e=T,U=1;\x1b\\", &mut out, None);

    assert!(out.is_empty(), "unexpected response: {out:?}");
}

#[test]
fn query_responder_ignores_csi_bytes_inside_kitty_apc_payload() {
    let mut responder = QueryResponder::with_graphics_support(false, true);
    let mut out = Vec::new();

    responder.process_queries(b"\x1b_Ga=t,i=42;image\x1b[6nbytes\x1b\\", &mut out, None);

    assert!(out.is_empty(), "unexpected response: {out:?}");
}

#[test]
fn query_responder_ignores_empty_and_non_kitty_apc_payloads() {
    let mut responder = QueryResponder::with_graphics_support(false, true);
    let mut out = Vec::new();

    responder.process_queries(b"\x1b_\x1b\\", &mut out, None);
    responder.process_queries(b"\x1b_Xpayload\x1b\\", &mut out, None);

    assert!(out.is_empty(), "unexpected response: {out:?}");
}

#[test]
fn kitty_feature_query_survives_apc_split_across_reads() {
    let mut responder = QueryResponder::with_graphics_support(false, true);
    let mut out = Vec::new();

    responder.process_queries(b"\x1b_", &mut out, None);
    responder.process_queries(b"Ga=q,i=42;\x1b", &mut out, None);
    assert!(out.is_empty());
    responder.process_queries(b"\\", &mut out, None);

    assert_eq!(out, b"\x1b_Gi=42;OK\x1b\\");
}

#[test]
fn oversized_apc_payload_is_bounded_and_query_parser_recovers() {
    let mut responder = QueryResponder::with_graphics_support(false, true);
    let mut out = Vec::new();
    let mut oversized = b"\x1b_Ga=t,i=1;".to_vec();
    oversized.resize(8192, b'x');
    oversized.extend_from_slice(b"\x1b\\");
    responder.process_queries(&oversized, &mut out, None);
    responder.process_queries(b"\x1b_Ga=q,i=42;\x1b\\", &mut out, None);

    assert_eq!(out, b"\x1b_Gi=42;OK\x1b\\");
}

#[test]
fn query_responder_ignores_query_like_bytes_inside_osc() {
    let mut responder = QueryResponder::with_graphics_support(false, false);
    let mut out = Vec::new();

    // An escape sequence embedded inside OSC data is not a terminal query.
    responder.process_queries(b"\x1b]0;title\x1b[6n\x07", &mut out, None);
    assert!(out.is_empty(), "unexpected response: {out:?}");

    // The string state must survive reader chunk boundaries and end at ST.
    responder.process_queries(b"\x1bPdata\x1b[6n", &mut out, None);
    responder.process_queries(b"\x1b\\\x1b[6n", &mut out, None);
    assert_eq!(out, b"\x1b[1;1R");
}
