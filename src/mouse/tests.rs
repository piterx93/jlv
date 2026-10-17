use ratatui::layout::Rect;

use super::*;

/// An 80×24 screen: the header on line 0, the histogram row on line 1 with 24 buckets
/// from column 2 on, rows 40 to 49 on lines 2 to 11 with the last one wrapped onto
/// line 12, lines 13 to 17 left empty, the details pane on lines 18 to 22 and the
/// footer on line 23.
fn map() -> HitMap {
    let mut lines: Vec<usize> = (40..50).collect();
    lines.push(49);
    HitMap {
        rows: Rect::new(0, 2, 80, 16),
        lines,
        buckets: Some(Rect::new(2, 1, 24, 1)),
        details: Some(Rect::new(0, 18, 80, 5)),
    }
}

#[test]
fn a_screen_line_hits_the_row_drawn_on_it() {
    assert_eq!(map().target(0, 2), Some(Target::Row(40)));
    assert_eq!(map().target(79, 11), Some(Target::Row(49)));
}

#[test]
fn every_line_of_a_wrapped_row_hits_that_row() {
    assert_eq!(map().target(30, 12), Some(Target::Row(49)));
}

#[test]
fn header_and_footer_hit_nothing() {
    assert_eq!(map().target(10, 0), None);
    assert_eq!(map().target(10, 23), None);
}

#[test]
fn lines_below_the_last_row_hit_nothing() {
    assert_eq!(map().target(10, 13), None);
    assert_eq!(map().target(10, 17), None);
}

#[test]
fn a_histogram_column_hits_its_bucket() {
    assert_eq!(map().target(2, 1), Some(Target::Bucket(0)));
    assert_eq!(map().target(25, 1), Some(Target::Bucket(23)));
}

#[test]
fn histogram_row_after_the_last_bucket_hits_nothing() {
    assert_eq!(map().target(26, 1), None);
    assert_eq!(map().target(79, 1), None);
}

#[test]
fn details_pane_borders_and_inside_hit_the_details_pane() {
    assert_eq!(map().target(0, 18), Some(Target::Details));
    assert_eq!(map().target(40, 20), Some(Target::Details));
    assert_eq!(map().target(79, 22), Some(Target::Details));
}

#[test]
fn cells_outside_the_screen_drawn_hit_nothing() {
    // The terminal grew between drawing and the click.
    assert_eq!(map().target(100, 5), None);
    assert_eq!(map().target(5, 30), None);
}

#[test]
fn nothing_is_hit_before_the_first_screen() {
    assert_eq!(HitMap::default().target(0, 0), None);
}
