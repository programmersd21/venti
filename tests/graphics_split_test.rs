//! regression: a kitty transmission must never fall through to the text
//! stream, not even when a multi-byte character was split across reads.
//!
//! the splitter tracked utf-8 continuation bytes to avoid mistaking them for
//! control bytes, but it also swallowed the ESC that starts the next kitty
//! APC - the whole transmission (and its base64 payload) was then printed as
//! text ("AAAA..." filling the screen).
use venti::pane::graphics::GraphicsParser;

fn kitty_transmit() -> Vec<u8> {
    let mut seq = Vec::new();
    seq.extend_from_slice(b"\x1b_Ga=T,f=100,s=8,v=8,i=7;");
    seq.extend_from_slice(b"iVBORw0KGgo=");
    seq.extend_from_slice(b"\x1b\\");
    seq
}

#[test]
fn a_split_utf8_character_does_not_swallow_the_next_graphics_sequence() {
    let mut parser = GraphicsParser::new();
    // "ä" (0xc3 0xa4) split across two reads, then a kitty transmission
    let mut first = vec![0xc3];
    first.extend_from_slice(&kitty_transmit());
    let segments = parser.split(&first);
    let plain: Vec<u8> = segments
        .iter()
        .filter_map(|segment| match segment {
            venti::pane::graphics::Segment::Plain(bytes) => Some(bytes.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let graphics = segments
        .iter()
        .filter(|segment| matches!(segment, venti::pane::graphics::Segment::Graphic(_)))
        .count();
    assert_eq!(graphics, 1, "the kitty transmission must be recognized");
    assert!(
        !plain.windows(12).any(|w| w == b"iVBORw0KGgo="),
        "the payload must not be printed as text: {plain:?}"
    );
}

#[test]
fn can_and_sub_abort_a_graphics_sequence() {
    let mut parser = GraphicsParser::new();
    // an aborted transmission (CAN) must not leave the parser capturing
    let mut bytes = b"\x1b_Ga=T,f=100,s=8,v=8,i=7;PARTIAL".to_vec();
    bytes.push(0x18);
    bytes.extend_from_slice(&kitty_transmit());
    let segments = parser.split(&bytes);
    let plain: Vec<u8> = segments
        .iter()
        .filter_map(|segment| match segment {
            venti::pane::graphics::Segment::Plain(bytes) => Some(bytes.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let graphics = segments
        .iter()
        .filter(|segment| matches!(segment, venti::pane::graphics::Segment::Graphic(_)))
        .count();
    assert_eq!(
        graphics, 1,
        "the sequence after the abort must be recognized"
    );
    assert!(
        !plain.windows(12).any(|w| w == b"iVBORw0KGgo="),
        "the payload must not be printed as text: {plain:?}"
    );
}
