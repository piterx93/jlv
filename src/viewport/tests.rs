use proptest::prelude::*;

use super::*;

/// `(top, selected)`: the first row on screen and the selected row.
fn pos(v: &Viewport) -> (usize, Option<usize>) {
    (v.top(), v.selected())
}

/// A viewport over `total` rows, `height > 0` lines, scrolled to `top` with `selected`
/// selected. Built through the public moves: down to the last line of the wanted
/// screen, then back up to the selection.
fn viewport(total: usize, height: usize, top: usize, selected: usize) -> Viewport {
    let mut v = Viewport::new(total, height);
    let last_on_screen = (top + height - 1).min(total - 1);
    v.down(last_on_screen);
    v.up(last_on_screen - selected);
    assert_eq!(pos(&v), (top, Some(selected)), "setting up the viewport");
    v
}

/// 100 rows on a 10-line screen, the shape most tests use.
fn hundred(top: usize, selected: usize) -> Viewport {
    viewport(100, 10, top, selected)
}

// --- new ---

#[test]
fn new_viewport_is_at_the_top_with_the_first_row_selected() {
    let v = Viewport::new(100, 10);
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn new_viewport_reports_its_total_and_height() {
    let v = Viewport::new(100, 10);
    assert_eq!((v.total(), v.height()), (100, 10));
}

#[test]
fn new_viewport_over_no_rows_has_no_selection() {
    let v = Viewport::new(0, 10);
    assert_eq!(pos(&v), (0, None));
}

// --- down ---

#[test]
fn down_within_the_screen_moves_only_the_selection() {
    let mut v = hundred(20, 22);
    v.down(3);
    assert_eq!(pos(&v), (20, Some(25)));
}

#[test]
fn down_onto_the_last_screen_line_does_not_scroll() {
    let mut v = hundred(20, 22);
    v.down(7);
    assert_eq!(pos(&v), (20, Some(29)));
}

#[test]
fn down_past_the_last_screen_line_scrolls_just_enough_to_show_the_selection() {
    let mut v = hundred(20, 27);
    v.down(5);
    assert_eq!(pos(&v), (23, Some(32)));
}

#[test]
fn down_by_more_than_the_remaining_rows_stops_at_the_last_row() {
    let mut v = hundred(85, 93);
    v.down(50);
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn down_on_the_last_row_changes_nothing() {
    let mut v = hundred(90, 99);
    v.down(1);
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn down_by_zero_changes_nothing() {
    let mut v = hundred(20, 27);
    v.down(0);
    assert_eq!(pos(&v), (20, Some(27)));
}

#[test]
fn down_by_usize_max_stops_at_the_last_row() {
    let mut v = hundred(20, 22);
    v.down(usize::MAX);
    assert_eq!(pos(&v), (90, Some(99)));
}

// --- up ---

#[test]
fn up_within_the_screen_moves_only_the_selection() {
    let mut v = hundred(20, 25);
    v.up(3);
    assert_eq!(pos(&v), (20, Some(22)));
}

#[test]
fn up_onto_the_first_screen_line_does_not_scroll() {
    let mut v = hundred(20, 25);
    v.up(5);
    assert_eq!(pos(&v), (20, Some(20)));
}

#[test]
fn up_past_the_first_screen_line_scrolls_just_enough_to_show_the_selection() {
    let mut v = hundred(20, 22);
    v.up(5);
    assert_eq!(pos(&v), (17, Some(17)));
}

#[test]
fn up_by_more_than_the_remaining_rows_stops_at_the_first_row() {
    let mut v = hundred(5, 7);
    v.up(50);
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn up_on_the_first_row_changes_nothing() {
    let mut v = Viewport::new(100, 10);
    v.up(1);
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn up_by_zero_changes_nothing() {
    let mut v = hundred(20, 22);
    v.up(0);
    assert_eq!(pos(&v), (20, Some(22)));
}

#[test]
fn up_by_usize_max_stops_at_the_first_row() {
    let mut v = hundred(20, 22);
    v.up(usize::MAX);
    assert_eq!(pos(&v), (0, Some(0)));
}

// --- page down / up ---

#[test]
fn page_down_scrolls_a_screen_and_keeps_the_selection_on_its_screen_line() {
    let mut v = hundred(20, 23);
    v.page_down();
    assert_eq!(pos(&v), (30, Some(33)));
}

#[test]
fn page_down_from_the_first_row_shows_the_next_screen() {
    let mut v = Viewport::new(100, 10);
    v.page_down();
    assert_eq!(pos(&v), (10, Some(10)));
}

#[test]
fn page_down_near_the_end_stops_the_screen_but_moves_the_selection_a_full_page() {
    let mut v = hundred(85, 87);
    v.page_down();
    assert_eq!(pos(&v), (90, Some(97)));
}

#[test]
fn page_down_with_less_than_a_page_below_selects_the_last_row() {
    let mut v = hundred(90, 93);
    v.page_down();
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn page_down_on_the_last_row_changes_nothing() {
    let mut v = hundred(90, 99);
    v.page_down();
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn page_up_scrolls_a_screen_and_keeps_the_selection_on_its_screen_line() {
    let mut v = hundred(30, 33);
    v.page_up();
    assert_eq!(pos(&v), (20, Some(23)));
}

#[test]
fn page_up_from_the_last_screen_shows_the_previous_screen() {
    let mut v = hundred(90, 97);
    v.page_up();
    assert_eq!(pos(&v), (80, Some(87)));
}

#[test]
fn page_up_near_the_start_stops_the_screen_but_moves_the_selection_a_full_page() {
    let mut v = hundred(5, 12);
    v.page_up();
    assert_eq!(pos(&v), (0, Some(2)));
}

#[test]
fn page_up_with_less_than_a_page_above_selects_the_first_row() {
    let mut v = hundred(0, 6);
    v.page_up();
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn page_up_on_the_first_row_changes_nothing() {
    let mut v = Viewport::new(100, 10);
    v.page_up();
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn page_down_when_all_rows_fit_selects_the_last_row_without_scrolling() {
    let mut v = Viewport::new(5, 10);
    v.page_down();
    assert_eq!(pos(&v), (0, Some(4)));
}

#[test]
fn page_up_when_all_rows_fit_selects_the_first_row_without_scrolling() {
    let mut v = viewport(5, 10, 0, 4);
    v.page_up();
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn page_moves_on_a_one_line_screen_move_one_row() {
    let mut v = viewport(100, 1, 50, 50);
    v.page_down();
    assert_eq!(pos(&v), (51, Some(51)));
    v.page_up();
    assert_eq!(pos(&v), (50, Some(50)));
}

// --- half page down / up ---

#[test]
fn half_page_down_scrolls_half_a_screen_and_keeps_the_selection_on_its_screen_line() {
    let mut v = hundred(20, 23);
    v.half_page_down();
    assert_eq!(pos(&v), (25, Some(28)));
}

#[test]
fn half_page_up_scrolls_half_a_screen_and_keeps_the_selection_on_its_screen_line() {
    let mut v = hundred(25, 28);
    v.half_page_up();
    assert_eq!(pos(&v), (20, Some(23)));
}

#[test]
fn half_page_down_near_the_end_stops_the_screen_but_moves_the_selection_half_a_page() {
    let mut v = hundred(88, 90);
    v.half_page_down();
    assert_eq!(pos(&v), (90, Some(95)));
}

#[test]
fn half_page_down_with_less_than_half_a_page_below_selects_the_last_row() {
    let mut v = hundred(90, 97);
    v.half_page_down();
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn half_page_down_on_the_last_row_changes_nothing() {
    let mut v = hundred(90, 99);
    v.half_page_down();
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn half_page_up_near_the_start_stops_the_screen_but_moves_the_selection_half_a_page() {
    let mut v = hundred(3, 9);
    v.half_page_up();
    assert_eq!(pos(&v), (0, Some(4)));
}

#[test]
fn half_page_up_with_less_than_half_a_page_above_selects_the_first_row() {
    let mut v = hundred(0, 3);
    v.half_page_up();
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn half_page_up_on_the_first_row_changes_nothing() {
    let mut v = Viewport::new(100, 10);
    v.half_page_up();
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn half_page_on_an_odd_height_rounds_down() {
    let mut v = Viewport::new(100, 7);
    v.half_page_down();
    assert_eq!(pos(&v), (3, Some(3)));
}

#[test]
fn half_page_moves_on_a_one_line_screen_move_one_row() {
    let mut v = viewport(100, 1, 50, 50);
    v.half_page_down();
    assert_eq!(pos(&v), (51, Some(51)));
    v.half_page_up();
    assert_eq!(pos(&v), (50, Some(50)));
}

// --- top / bottom ---

#[test]
fn go_bottom_selects_the_last_row_and_shows_the_last_full_screen() {
    let mut v = hundred(20, 23);
    v.go_bottom();
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn go_bottom_on_the_last_row_changes_nothing() {
    let mut v = hundred(90, 99);
    v.go_bottom();
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn go_bottom_when_all_rows_fit_does_not_scroll() {
    let mut v = Viewport::new(5, 10);
    v.go_bottom();
    assert_eq!(pos(&v), (0, Some(4)));
}

#[test]
fn go_top_selects_the_first_row_and_shows_the_first_screen() {
    let mut v = hundred(90, 99);
    v.go_top();
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn go_top_on_the_first_row_changes_nothing() {
    let mut v = Viewport::new(100, 10);
    v.go_top();
    assert_eq!(pos(&v), (0, Some(0)));
}

// --- goto ---

#[test]
fn goto_a_visible_row_moves_only_the_selection() {
    let mut v = hundred(20, 22);
    v.goto(27);
    assert_eq!(pos(&v), (20, Some(27)));
}

#[test]
fn goto_the_first_screen_line_does_not_scroll() {
    let mut v = hundred(20, 25);
    v.goto(20);
    assert_eq!(pos(&v), (20, Some(20)));
}

#[test]
fn goto_the_last_screen_line_does_not_scroll() {
    let mut v = hundred(20, 25);
    v.goto(29);
    assert_eq!(pos(&v), (20, Some(29)));
}

#[test]
fn goto_the_row_just_below_the_screen_centers_it() {
    let mut v = hundred(20, 25);
    v.goto(30);
    assert_eq!(pos(&v), (26, Some(30)));
}

#[test]
fn goto_the_row_just_above_the_screen_centers_it() {
    let mut v = hundred(20, 25);
    v.goto(19);
    assert_eq!(pos(&v), (15, Some(19)));
}

#[test]
fn goto_a_far_row_puts_it_on_the_upper_middle_line_of_an_even_height() {
    let mut v = Viewport::new(100, 10);
    v.goto(50);
    assert_eq!(pos(&v), (46, Some(50)));
}

#[test]
fn goto_a_far_row_puts_it_on_the_middle_line_of_an_odd_height() {
    let mut v = Viewport::new(100, 5);
    v.goto(50);
    assert_eq!(pos(&v), (48, Some(50)));
}

#[test]
fn goto_a_row_near_the_start_shows_the_first_screen() {
    let mut v = hundred(90, 95);
    v.goto(3);
    assert_eq!(pos(&v), (0, Some(3)));
}

#[test]
fn goto_a_row_near_the_end_shows_the_last_full_screen() {
    let mut v = Viewport::new(100, 10);
    v.goto(97);
    assert_eq!(pos(&v), (90, Some(97)));
}

#[test]
fn goto_past_the_end_selects_the_last_row() {
    let mut v = Viewport::new(100, 10);
    v.goto(500);
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn goto_usize_max_selects_the_last_row() {
    let mut v = hundred(20, 25);
    v.goto(usize::MAX);
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn goto_when_all_rows_fit_does_not_scroll() {
    let mut v = Viewport::new(5, 10);
    v.goto(4);
    assert_eq!(pos(&v), (0, Some(4)));
}

// --- resize ---

#[test]
fn resize_reports_the_new_height() {
    let mut v = hundred(20, 23);
    v.resize(7);
    assert_eq!(v.height(), 7);
}

#[test]
fn shrinking_keeps_the_screen_when_the_selection_stays_on_it() {
    let mut v = hundred(20, 23);
    v.resize(5);
    assert_eq!(pos(&v), (20, Some(23)));
}

#[test]
fn shrinking_past_the_selection_scrolls_it_onto_the_last_line() {
    let mut v = hundred(20, 28);
    v.resize(5);
    assert_eq!(pos(&v), (24, Some(28)));
}

#[test]
fn growing_keeps_the_screen_where_there_are_rows_to_fill_it() {
    let mut v = hundred(20, 23);
    v.resize(20);
    assert_eq!(pos(&v), (20, Some(23)));
}

#[test]
fn growing_near_the_end_pulls_the_screen_up_to_fill_it() {
    let mut v = hundred(90, 95);
    v.resize(20);
    assert_eq!(pos(&v), (80, Some(95)));
}

#[test]
fn growing_past_the_total_shows_all_rows_from_the_top() {
    let mut v = hundred(90, 95);
    v.resize(200);
    assert_eq!(pos(&v), (0, Some(95)));
}

#[test]
fn resizing_to_one_line_shows_only_the_selected_row() {
    let mut v = hundred(20, 23);
    v.resize(1);
    assert_eq!(pos(&v), (23, Some(23)));
}

#[test]
fn resizing_to_zero_lines_keeps_the_selection_and_moves_top_to_it() {
    let mut v = hundred(20, 23);
    v.resize(0);
    assert_eq!(pos(&v), (23, Some(23)));
}

#[test]
fn growing_from_zero_lines_shows_the_selection_on_the_first_line() {
    let mut v = Viewport::new(100, 0);
    v.down(50);
    v.resize(10);
    assert_eq!(pos(&v), (50, Some(50)));
}

#[test]
fn growing_from_zero_lines_near_the_end_shows_the_last_full_screen() {
    let mut v = Viewport::new(100, 0);
    v.down(95);
    v.resize(10);
    assert_eq!(pos(&v), (90, Some(95)));
}

// --- set_total ---

#[test]
fn set_total_reports_the_new_total() {
    let mut v = hundred(20, 23);
    v.set_total(150);
    assert_eq!(v.total(), 150);
}

#[test]
fn growing_keeps_the_screen_and_the_selection() {
    let mut v = hundred(90, 99);
    v.set_total(150);
    assert_eq!(pos(&v), (90, Some(99)));
}

#[test]
fn growing_while_all_rows_fit_keeps_the_screen_and_the_selection() {
    let mut v = viewport(5, 10, 0, 4);
    v.set_total(8);
    assert_eq!(pos(&v), (0, Some(4)));
}

#[test]
fn growing_from_no_rows_selects_the_first_row() {
    let mut v = Viewport::new(0, 10);
    v.set_total(5);
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn setting_the_same_total_changes_nothing() {
    let mut v = hundred(50, 55);
    v.set_total(100);
    assert_eq!(pos(&v), (50, Some(55)));
}

#[test]
fn shrinking_below_the_selection_selects_the_new_last_row_on_the_last_full_screen() {
    let mut v = hundred(50, 55);
    v.set_total(30);
    assert_eq!(pos(&v), (20, Some(29)));
}

#[test]
fn shrinking_into_the_screen_pulls_the_screen_up_and_keeps_the_selection() {
    let mut v = hundred(50, 55);
    v.set_total(58);
    assert_eq!(pos(&v), (48, Some(55)));
}

#[test]
fn shrinking_below_the_screen_without_reaching_the_selection_keeps_everything() {
    let mut v = hundred(50, 55);
    v.set_total(60);
    assert_eq!(pos(&v), (50, Some(55)));
}

#[test]
fn shrinking_below_the_height_shows_all_rows_from_the_top() {
    let mut v = hundred(50, 55);
    v.set_total(4);
    assert_eq!(pos(&v), (0, Some(3)));
}

#[test]
fn shrinking_to_one_row_selects_it() {
    let mut v = hundred(50, 55);
    v.set_total(1);
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn shrinking_to_no_rows_clears_the_selection() {
    let mut v = hundred(50, 55);
    v.set_total(0);
    assert_eq!(pos(&v), (0, None));
}

// --- edge shapes ---

type Labelled = (&'static str, fn(&mut Viewport));

/// Every operation that takes no new size, labelled for failure messages.
fn every_operation() -> Vec<Labelled> {
    vec![
        ("down", |v| v.down(1)),
        ("down many", |v| v.down(1000)),
        ("up", |v| v.up(1)),
        ("up many", |v| v.up(1000)),
        ("page_down", Viewport::page_down),
        ("page_up", Viewport::page_up),
        ("half_page_down", Viewport::half_page_down),
        ("half_page_up", Viewport::half_page_up),
        ("go_bottom", Viewport::go_bottom),
        ("go_top", Viewport::go_top),
        ("goto", |v| v.goto(3)),
        ("goto past the end", |v| v.goto(1000)),
    ]
}

#[test]
fn no_operation_selects_anything_when_there_are_no_rows() {
    for (name, op) in every_operation() {
        let mut v = Viewport::new(0, 10);
        op(&mut v);
        assert_eq!(pos(&v), (0, None), "after {name}");
    }
}

#[test]
fn resizing_with_no_rows_keeps_no_selection() {
    let mut v = Viewport::new(0, 10);
    v.resize(3);
    assert_eq!(pos(&v), (0, None));
    v.resize(0);
    assert_eq!(pos(&v), (0, None));
}

#[test]
fn every_operation_on_a_single_row_keeps_it_selected() {
    for (name, op) in every_operation() {
        let mut v = Viewport::new(1, 10);
        op(&mut v);
        assert_eq!(pos(&v), (0, Some(0)), "after {name}");
    }
}

#[test]
fn a_single_row_on_a_single_line_stays_selected() {
    for (name, op) in every_operation() {
        let mut v = Viewport::new(1, 1);
        op(&mut v);
        assert_eq!(pos(&v), (0, Some(0)), "after {name}");
    }
}

#[test]
fn a_one_line_screen_always_shows_the_selected_row() {
    let mut v = Viewport::new(100, 1);
    v.down(1);
    assert_eq!(pos(&v), (1, Some(1)));
    v.goto(50);
    assert_eq!(pos(&v), (50, Some(50)));
    v.up(3);
    assert_eq!(pos(&v), (47, Some(47)));
    v.go_bottom();
    assert_eq!(pos(&v), (99, Some(99)));
}

#[test]
fn a_zero_line_screen_starts_at_the_first_row() {
    let v = Viewport::new(100, 0);
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn on_a_zero_line_screen_top_follows_the_selection() {
    let mut v = Viewport::new(100, 0);
    v.down(5);
    assert_eq!(pos(&v), (5, Some(5)));
    v.goto(40);
    assert_eq!(pos(&v), (40, Some(40)));
    v.up(10);
    assert_eq!(pos(&v), (30, Some(30)));
    v.go_bottom();
    assert_eq!(pos(&v), (99, Some(99)));
    v.go_top();
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn page_moves_on_a_zero_line_screen_move_one_row() {
    let mut v = Viewport::new(100, 0);
    v.page_down();
    assert_eq!(pos(&v), (1, Some(1)));
    v.half_page_down();
    assert_eq!(pos(&v), (2, Some(2)));
    v.half_page_up();
    assert_eq!(pos(&v), (1, Some(1)));
    v.page_up();
    assert_eq!(pos(&v), (0, Some(0)));
}

#[test]
fn page_down_on_the_last_row_of_a_zero_line_screen_changes_nothing() {
    let mut v = Viewport::new(100, 0);
    v.go_bottom();
    v.page_down();
    assert_eq!(pos(&v), (99, Some(99)));
}

#[test]
fn shrinking_the_total_on_a_zero_line_screen_keeps_top_on_the_selection() {
    let mut v = Viewport::new(100, 0);
    v.goto(50);
    v.set_total(20);
    assert_eq!(pos(&v), (19, Some(19)));
}

// --- properties ---

#[derive(Debug, Clone)]
enum Op {
    Up(usize),
    Down(usize),
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    Top,
    Bottom,
    Goto(usize),
    Resize(usize),
    SetTotal(usize),
}

impl Op {
    fn apply(&self, v: &mut Viewport) {
        match *self {
            Op::Up(n) => v.up(n),
            Op::Down(n) => v.down(n),
            Op::PageUp => v.page_up(),
            Op::PageDown => v.page_down(),
            Op::HalfPageUp => v.half_page_up(),
            Op::HalfPageDown => v.half_page_down(),
            Op::Top => v.go_top(),
            Op::Bottom => v.go_bottom(),
            Op::Goto(row) => v.goto(row),
            Op::Resize(height) => v.resize(height),
            Op::SetTotal(total) => v.set_total(total),
        }
    }
}

/// Totals and heights: mostly small, to hit the edge shapes often.
fn size() -> impl Strategy<Value = usize> {
    prop_oneof![4 => 0..25usize, 1 => 0..5000usize]
}

/// Move distances and goto targets, including ones that would overflow.
fn distance() -> impl Strategy<Value = usize> {
    prop_oneof![4 => 0..30usize, 1 => 0..5000usize, 1 => Just(usize::MAX)]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        distance().prop_map(Op::Up),
        distance().prop_map(Op::Down),
        Just(Op::PageUp),
        Just(Op::PageDown),
        Just(Op::HalfPageUp),
        Just(Op::HalfPageDown),
        Just(Op::Top),
        Just(Op::Bottom),
        distance().prop_map(Op::Goto),
        size().prop_map(Op::Resize),
        size().prop_map(Op::SetTotal),
    ]
}

/// Any viewport reachable through the public operations.
fn any_viewport() -> impl Strategy<Value = Viewport> {
    (size(), size(), prop::collection::vec(op(), 0..20)).prop_map(|(total, height, ops)| {
        let mut v = Viewport::new(total, height);
        for op in &ops {
            op.apply(&mut v);
        }
        v
    })
}

fn check_invariants(v: &Viewport) -> Result<(), TestCaseError> {
    let (total, height, top) = (v.total(), v.height(), v.top());
    match v.selected() {
        None => {
            prop_assert_eq!(total, 0, "no selection although there are rows");
            prop_assert_eq!(top, 0);
        }
        Some(selected) => {
            prop_assert!(selected < total, "selected {} of {} rows", selected, total);
            if height == 0 {
                prop_assert_eq!(top, selected, "top follows the selection on zero lines");
            } else {
                prop_assert!(top <= selected, "selected {} above top {}", selected, top);
                prop_assert!(
                    selected - top < height,
                    "selected {} below the screen {}..{}+{}",
                    selected,
                    top,
                    top,
                    height
                );
                prop_assert!(
                    top <= total.saturating_sub(height),
                    "screen {}+{} runs past {} rows",
                    top,
                    height,
                    total
                );
            }
        }
    }
    Ok(())
}

/// Checks that `v` scrolled as little as possible from `old_top`: it holds the
/// invariants, and moving `top` one row back towards `old_top` would break them.
fn check_scrolled_least(old_top: usize, v: &Viewport) -> Result<(), TestCaseError> {
    check_invariants(v)?;
    if v.top != old_top {
        let mut closer = v.clone();
        closer.top = if old_top < v.top {
            v.top - 1
        } else {
            v.top + 1
        };
        prop_assert!(
            check_invariants(&closer).is_err(),
            "scrolled from {} to {} where {} would do",
            old_top,
            v.top,
            closer.top
        );
    }
    Ok(())
}

proptest! {
    #[test]
    fn invariants_hold_after_any_sequence_of_operations(
        total in size(),
        height in size(),
        ops in prop::collection::vec(op(), 0..60),
    ) {
        let mut v = Viewport::new(total, height);
        check_invariants(&v)?;
        for op in &ops {
            op.apply(&mut v);
            check_invariants(&v).map_err(|e| TestCaseError::fail(format!("after {op:?}: {e}")))?;
        }
    }

    #[test]
    fn down_moves_the_selection_by_n_or_to_the_last_row(mut v in any_viewport(), n in distance()) {
        let before = v.selected();
        v.down(n);
        let expected = before.map(|s| s.saturating_add(n).min(v.total() - 1));
        prop_assert_eq!(v.selected(), expected);
    }

    #[test]
    fn up_moves_the_selection_by_n_or_to_the_first_row(mut v in any_viewport(), n in distance()) {
        let before = v.selected();
        v.up(n);
        prop_assert_eq!(v.selected(), before.map(|s| s.saturating_sub(n)));
    }

    #[test]
    fn line_moves_scroll_as_little_as_possible(
        mut v in any_viewport(),
        n in distance(),
        down in any::<bool>(),
    ) {
        let old_top = v.top();
        if down { v.down(n) } else { v.up(n) }
        check_scrolled_least(old_top, &v)?;
    }

    #[test]
    fn page_down_moves_selection_and_screen_by_a_page_unless_they_hit_the_end(
        mut v in any_viewport(),
        half in any::<bool>(),
    ) {
        prop_assume!(v.total() > 0 && v.height() > 0);
        let (total, height) = (v.total(), v.height());
        let (top, selected) = (v.top(), v.selected().unwrap());
        let page = if half { (height / 2).max(1) } else { height };
        if half { v.half_page_down() } else { v.page_down() }
        let last_top = total.saturating_sub(height);
        prop_assert_eq!(pos(&v), ((top + page).min(last_top), Some((selected + page).min(total - 1))));
    }

    #[test]
    fn page_up_moves_selection_and_screen_by_a_page_unless_they_hit_the_start(
        mut v in any_viewport(),
        half in any::<bool>(),
    ) {
        prop_assume!(v.total() > 0 && v.height() > 0);
        let height = v.height();
        let (top, selected) = (v.top(), v.selected().unwrap());
        let page = if half { (height / 2).max(1) } else { height };
        if half { v.half_page_up() } else { v.page_up() }
        prop_assert_eq!(pos(&v), (top.saturating_sub(page), Some(selected.saturating_sub(page))));
    }

    #[test]
    fn goto_selects_the_row_or_the_last_one(mut v in any_viewport(), row in distance()) {
        v.goto(row);
        prop_assert_eq!(v.selected(), v.total().checked_sub(1).map(|last| row.min(last)));
    }

    #[test]
    fn goto_a_visible_row_never_scrolls(mut v in any_viewport(), offset in 0..5000usize) {
        prop_assume!(v.total() > 0 && v.height() > 0);
        let (total, height, top) = (v.total(), v.height(), v.top());
        let visible = height.min(total - top);
        let row = top + offset % visible;
        v.goto(row);
        prop_assert_eq!(pos(&v), (top, Some(row)));
    }

    #[test]
    fn goto_an_off_screen_row_centers_it_as_far_as_the_ends_allow(
        mut v in any_viewport(),
        pick in 0..5000usize,
    ) {
        prop_assume!(v.total() > 0 && v.height() > 0);
        // Rejecting all-rows-fit shapes would reject about half the cases; growing the
        // total instead keeps the screen and the selection where they are.
        if v.total() <= v.height() {
            v.set_total(v.height() + 1 + pick % 5);
        }
        let (total, height, top) = (v.total(), v.height(), v.top());
        // The `total - height` rows off screen: those above it, then those below it.
        let k = pick % (total - height);
        let row = if k < top { k } else { k + height };
        v.goto(row);
        let centered = row.saturating_sub((height - 1) / 2).min(total - height);
        prop_assert_eq!(pos(&v), (centered, Some(row)));
    }

    #[test]
    fn resize_never_changes_the_selection(mut v in any_viewport(), height in size()) {
        let before = v.selected();
        v.resize(height);
        prop_assert_eq!(v.selected(), before);
    }

    #[test]
    fn resize_scrolls_as_little_as_possible(mut v in any_viewport(), height in size()) {
        let old_top = v.top();
        v.resize(height);
        check_scrolled_least(old_top, &v)?;
    }

    #[test]
    fn growing_the_total_keeps_the_screen_and_the_selection(
        mut v in any_viewport(),
        more in 0..5000usize,
    ) {
        prop_assume!(v.total() > 0);
        let before = pos(&v);
        v.set_total(v.total() + more);
        prop_assert_eq!(pos(&v), before);
    }

    #[test]
    fn set_total_keeps_the_selection_unless_it_was_cut_off(
        mut v in any_viewport(),
        total in size(),
    ) {
        let before = v.selected();
        v.set_total(total);
        let expected = before
            .or(Some(0))
            .and_then(|s| total.checked_sub(1).map(|last| s.min(last)));
        prop_assert_eq!(v.selected(), expected);
    }

    #[test]
    fn set_total_scrolls_as_little_as_possible(mut v in any_viewport(), total in size()) {
        let old_top = v.top();
        v.set_total(total);
        check_scrolled_least(old_top, &v)?;
    }
}
