use venti::pane::query::{QueryResponder, ScreenState};

#[test]
fn test_da1_omits_sixel_when_unsupported() {
    let mut responder = QueryResponder::with_graphics_support(false, false);
    let mut out = Vec::new();
    responder.process_queries(b"\x1b[c", &mut out, None);
    assert_eq!(out, b"\x1b[?63;1;2;22c");
}

#[test]
fn test_query_responder_device_attributes() {
    let mut responder = QueryResponder::with_graphics_support(true, false);
    let mut out = Vec::new();

    responder.process_queries(b"\x1b[", &mut out, None);
    assert!(out.is_empty());
    responder.process_queries(b"c", &mut out, None);
    assert_eq!(out, b"\x1b[?63;1;2;4;22c");
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
fn query_responder_ignores_query_like_bytes_inside_osc() {
    let mut responder = QueryResponder::with_graphics_support(false, false);
    let mut out = Vec::new();

    responder.process_queries(b"\x1b]0;title\x1b[6n\x07", &mut out, None);
    assert!(out.is_empty(), "unexpected response: {out:?}");

    responder.process_queries(b"\x1bPdata\x1b[6n", &mut out, None);
    responder.process_queries(b"\x1b\\\x1b[6n", &mut out, None);
    assert_eq!(out, b"\x1b[1;1R");
}
