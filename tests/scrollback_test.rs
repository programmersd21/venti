// regression coverage for the vt100 fork's scrollback semantics.
//
// term-wm-vt100 exposes the *current scrollback view offset* via
// `Screen::scrollback()` and the accumulated line count is only reachable by
// clamping the view to the maximum offset. venti's scrollback navigation used
// to clamp against the offset instead of the count, which made wheel
// scrolling a permanent no-op.
#[test]
fn scrollback_count_is_available_via_offset_clamp() {
    let mut parser = vt100::Parser::new(28, 100, 10000);
    for i in 1..=60 {
        parser.process(format!("line {i}\n").as_bytes());
    }
    assert_eq!(parser.screen().scrollback(), 0);
    let max = venti::pane::search::scrollback_len(&mut parser);
    assert!(max > 0, "expected scrollback to accumulate");
    assert_eq!(
        parser.screen().scrollback(),
        0,
        "helper must restore the view offset"
    );
}

#[test]
fn full_buffer_text_preserves_scrolled_view_offset() {
    let mut parser = vt100::Parser::new(3, 20, 100);
    for i in 1..=12 {
        parser.process(format!("line {i}\r\n").as_bytes());
    }
    parser.screen_mut().set_scrollback(2);
    let offset = parser.screen().scrollback();
    assert!(offset > 0);

    let _ = venti::pane::search::full_buffer_text(&mut parser);

    assert_eq!(parser.screen().scrollback(), offset);
}
