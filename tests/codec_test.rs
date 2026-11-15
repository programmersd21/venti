use proptest::prelude::*;
use venti::proto::{
    CellDiff, ClientMsg, CursorShape, HelloMsg, ScreenUpdate, ServerMsg, decode_from_slice, encode,
};

#[test]
fn test_roundtrip_codec() {
    let hello = ClientMsg::Hello(HelloMsg {
        protocol: 1,
        term: "xterm-256color".to_string(),
        colorterm: Some("truecolor".to_string()),
        size: (80, 24),
        env: vec![("SHELL".to_string(), "/bin/bash".to_string())],
        cell_pixel: None,
    });

    let encoded = encode(&hello).expect("encode succeeds");
    let (decoded, consumed): (ClientMsg, usize) = decode_from_slice(&encoded)
        .expect("decode succeeds")
        .expect("has frame");
    assert_eq!(consumed, encoded.len());
    assert_eq!(decoded, hello);

    let update = ServerMsg::Update(ScreenUpdate {
        cols: 80,
        rows: 24,
        cursor_pos: Some((10, 5)),
        cursor_visible: true,
        cursor_shape: CursorShape::Block,
        cells: vec![CellDiff {
            x: 0,
            y: 0,
            symbol: "a".to_string(),
            fg: venti::proto::ProtoColor::Rgb(255, 0, 0),
            bg: venti::proto::ProtoColor::Default,
            bold: true,
            dim: false,
            italic: false,
            underline: false,
            reverse: false,
            strike: false,
        }],
        full_redraw: false,
        gfx: Vec::new(),
        reset: false,
    });

    let encoded_u = encode(&update).expect("encode succeeds");
    let (decoded_u, consumed_u): (ServerMsg, usize) = decode_from_slice(&encoded_u)
        .expect("decode succeeds")
        .expect("has frame");
    assert_eq!(consumed_u, encoded_u.len());
    assert_eq!(decoded_u, update);
}

proptest! {
    #[test]
    fn test_decoder_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        // Arbitrary inputs should return Ok(None) or Err(_), but never panic
        let _ = decode_from_slice::<ClientMsg>(&bytes);
        let _ = decode_from_slice::<ServerMsg>(&bytes);
    }
}
