use venti::pane::selection::SelectionMode;

#[test]
fn test_selection_normalization_and_contains() {
    use venti::pane::selection::{GridPoint, Selection};

    let p1 = GridPoint { line: 2, col: 5 };
    let p2 = GridPoint { line: 4, col: 10 };
    let sel = Selection::new(p2, p1, SelectionMode::Character);
    let (min_pt, max_pt) = sel.normalized();
    assert_eq!(min_pt, p1);
    assert_eq!(max_pt, p2);

    assert!(sel.contains(2, 5));
    assert!(sel.contains(2, 20));
    assert!(sel.contains(3, 0));
    assert!(sel.contains(4, 10));
    assert!(!sel.contains(4, 11));
    assert!(!sel.contains(1, 10));
}
