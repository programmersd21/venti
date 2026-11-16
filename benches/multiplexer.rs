use criterion::{Criterion, black_box, criterion_group, criterion_main};
use ratatui::layout::Rect;
use venti::input::KeyParser;
use venti::layout::{LayoutNode, PaneId, SplitAxis, resolve};
use venti::proto::{CellDiff, CursorShape, ScreenUpdate, ServerMsg, decode_from_slice, encode};
use venti::util::fuzzy::fuzzy_match;

fn bench_layout_resolve(c: &mut Criterion) {
    let mut tree = LayoutNode::leaf(PaneId(1));
    for id in 2..=16 {
        tree = LayoutNode::split(
            if id % 2 == 0 {
                SplitAxis::Horizontal
            } else {
                SplitAxis::Vertical
            },
            0.5,
            tree,
            LayoutNode::leaf(PaneId(id)),
        );
    }
    let area = Rect::new(0, 0, 1920, 1080);

    c.bench_function("layout_tree_resolve_16_panes", |b| {
        b.iter(|| {
            let res = resolve(black_box(&tree), black_box(area), 1, 1);
            black_box(res);
        });
    });
}

fn bench_codec_roundtrip(c: &mut Criterion) {
    let msg = ServerMsg::Update(ScreenUpdate {
        cols: 120,
        rows: 40,
        cursor_pos: Some((10, 5)),
        cursor_visible: true,
        cursor_shape: CursorShape::Block,
        cells: vec![
            CellDiff {
                x: 10,
                y: 5,
                symbol: "a".to_string(),
                fg: venti::proto::ProtoColor::Default,
                bg: venti::proto::ProtoColor::Default,
                bold: false,
                dim: false,
                italic: false,
                underline: false,
                reverse: false,
                strike: false,
            };
            200
        ],
        full_redraw: false,
        gfx: Vec::new(),
        reset: false,
    });

    c.bench_function("codec_encode_screen_update", |b| {
        b.iter(|| {
            let encoded = encode(black_box(&msg)).unwrap();
            black_box(encoded);
        });
    });

    let encoded = encode(&msg).unwrap();
    c.bench_function("codec_decode_screen_update", |b| {
        b.iter(|| {
            let (decoded, _): (ServerMsg, usize) =
                decode_from_slice(black_box(&encoded)).unwrap().unwrap();
            black_box(decoded);
        });
    });
}

fn bench_key_parser(c: &mut Criterion) {
    let bytes = b"\x1b[1;5A\x1b[M @@@\x1b[200~pasted content here\x1b[201~";

    c.bench_function("key_parser_mixed_stream", |b| {
        b.iter_batched(
            KeyParser::new,
            |mut parser| black_box(parser.parse_bytes(black_box(bytes))),
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_fuzzy_match(c: &mut Criterion) {
    let haystack = "workspace switch: default scratchpad layout";
    let needle = "ws sw def";

    c.bench_function("fuzzy_match_search", |b| {
        b.iter(|| {
            let score = fuzzy_match(black_box(needle), black_box(haystack));
            black_box(score);
        });
    });
}

criterion_group!(
    benches,
    bench_layout_resolve,
    bench_codec_roundtrip,
    bench_key_parser,
    bench_fuzzy_match
);
criterion_main!(benches);
