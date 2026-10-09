use std::time::{Duration, Instant};

use proptest::collection::vec;
use proptest::prelude::*;

use super::*;

#[derive(Debug, Clone)]
enum Op {
    /// Adds an entry with this many continuation lines.
    Push(usize),
    /// Toggles the entry at this index modulo the number of entries.
    Toggle(usize),
    FoldAll,
    UnfoldAll,
    /// Keeps this many entries.
    Truncate(usize),
}

fn op() -> impl Strategy<Value = Op> {
    // Mostly toggles among a few entries, so that several entries are toggled at once
    // between the rarer folds and unfolds of all.
    prop_oneof![
        3 => (0..4usize).prop_map(Op::Push),
        6 => (0..8usize).prop_map(Op::Toggle),
        1 => Just(Op::FoldAll),
        1 => Just(Op::UnfoldAll),
        1 => (0..8usize).prop_map(Op::Truncate),
    ]
}

/// The fold state spelled out entry by entry: each entry's continuation lines and
/// whether it is unfolded.
#[derive(Default)]
struct Model {
    entries: Vec<(usize, bool)>,
    unfold_new_entries: bool,
}

impl Model {
    /// Applies `op` to the model and to `folds`.
    fn apply(&mut self, op: &Op, folds: &mut Folds) {
        match *op {
            Op::Push(lines) => {
                self.entries.push((lines, self.unfold_new_entries));
                folds.push(lines);
            }
            Op::Toggle(_) if self.entries.is_empty() => {}
            Op::Toggle(i) => {
                let entry = i % self.entries.len();
                let (lines, unfolded) = &mut self.entries[entry];
                // Without continuation lines there is nothing to unfold.
                *unfolded = !*unfolded && *lines > 0;
                folds.toggle(entry);
            }
            Op::FoldAll | Op::UnfoldAll => {
                let unfold = matches!(op, Op::UnfoldAll);
                self.unfold_new_entries = unfold;
                for (_, unfolded) in &mut self.entries {
                    *unfolded = unfold;
                }
                if unfold {
                    folds.unfold_all();
                } else {
                    folds.fold_all();
                }
            }
            Op::Truncate(entries) => {
                self.entries.truncate(entries);
                folds.truncate(entries);
            }
        }
    }

    /// `(entry, line)` of every row, top to bottom.
    fn rows(&self) -> Vec<(usize, usize)> {
        let mut rows = Vec::new();
        for (entry, &(lines, unfolded)) in self.entries.iter().enumerate() {
            let shown = if unfolded { lines } else { 0 };
            rows.extend((0..=shown).map(|line| (entry, line)));
        }
        rows
    }

    fn hidden_lines(&self) -> Vec<usize> {
        let hidden = |&(lines, unfolded): &(usize, bool)| if unfolded { 0 } else { lines };
        self.entries.iter().map(hidden).collect()
    }
}

proptest! {
    #[test]
    fn rows_map_to_shown_lines_and_back_in_any_fold_state(ops in vec(op(), 0..40)) {
        let mut model = Model::default();
        let mut folds = Folds::default();
        for op in &ops {
            model.apply(op, &mut folds);
        }
        let rows = model.rows();
        prop_assert_eq!(folds.row_count(), rows.len());
        for (row, &(entry, line)) in rows.iter().enumerate() {
            prop_assert_eq!(folds.line_at(row), (entry, line), "row {}", row);
            prop_assert_eq!(folds.row_of(entry, line), row, "entry {} line {}", entry, line);
        }
        let hidden: Vec<usize> = (0..model.entries.len()).map(|e| folds.hidden_lines(e)).collect();
        prop_assert_eq!(hidden, model.hidden_lines());
    }
}

/// Folding or unfolding all doesn't visit the entries: done 100 000 times over a
/// million entries, it is over in well under a second.
#[test]
fn folding_and_unfolding_all_takes_constant_time() {
    let mut folds = Folds::default();
    for _ in 0..1_000_000 {
        folds.push(1);
    }
    folds.toggle(0);
    let start = Instant::now();
    for _ in 0..50_000 {
        folds.unfold_all();
        folds.fold_all();
    }
    folds.unfold_all();
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(folds.row_count(), 2_000_000);
}
