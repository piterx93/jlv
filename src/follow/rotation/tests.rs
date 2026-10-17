use proptest::prelude::*;

use super::*;
use Change::*;

/// The file `inode` holding `content`, as a look at the path sees it.
fn file(inode: u64, content: &str) -> Observation {
    Observation::Present(snapshot(inode, content.as_bytes()))
}

fn snapshot(inode: u64, content: &[u8]) -> Snapshot {
    Snapshot {
        inode,
        size: content.len() as u64,
        prefix: content[..content.len().min(PREFIX_LEN)].to_vec(),
    }
}

/// A run of looks at the path: the tracker starts on inode 1 holding `start`, then
/// takes each observation with its read offset and should answer the change.
struct Case {
    name: &'static str,
    start: String,
    steps: Vec<(Observation, u64, Change)>,
}

fn case(name: &'static str, start: &str, steps: Vec<(Observation, u64, Change)>) -> Case {
    Case {
        name,
        start: start.to_string(),
        steps,
    }
}

#[test]
fn answers_every_transition() {
    // Longer than the first bytes an observation carries, and the same with the last
    // of those bytes changed.
    let long = "x".repeat(PREFIX_LEN + 10);
    let mut long_edited = long.clone();
    long_edited.replace_range(PREFIX_LEN - 1..PREFIX_LEN, "y");
    let cases = [
        case(
            "same size, read to the end",
            "abc\n",
            vec![(file(1, "abc\n"), 4, Unchanged)],
        ),
        case(
            "bytes past the read offset",
            "abc\n",
            vec![(file(1, "abc\ndef\n"), 4, Grew)],
        ),
        case(
            "read offset behind the size",
            "abc\n",
            vec![(file(1, "abc\n"), 2, Grew)],
        ),
        case(
            "smaller than the read offset",
            "abc\n",
            vec![(file(1, "ab"), 4, Truncated)],
        ),
        case("emptied", "abc\n", vec![(file(1, ""), 4, Truncated)]),
        case(
            "copytruncate and fast writes: grew, first bytes changed",
            "abc\n",
            vec![(file(1, "xyz\nmore\n"), 4, Truncated)],
        ),
        case(
            "rewritten to the same size",
            "abc\n",
            vec![(file(1, "xyz\n"), 4, Truncated)],
        ),
        case(
            "shrank below the bytes seen, read offset behind",
            "abcdef",
            vec![(file(1, "abc"), 2, Truncated)],
        ),
        case(
            "the last of the first bytes changed",
            &long,
            vec![(file(1, &long_edited), PREFIX_LEN as u64, Truncated)],
        ),
        case(
            "another inode",
            "abc\n",
            vec![(file(2, "new\n"), 4, Replaced)],
        ),
        case(
            "another inode with the same first bytes",
            "abc\n",
            vec![(file(2, "abc\ndef\n"), 4, Replaced)],
        ),
        case(
            "path missing",
            "abc\n",
            vec![(Observation::Missing, 4, Gone)],
        ),
        case(
            "path stays missing",
            "abc\n",
            vec![
                (Observation::Missing, 4, Gone),
                (Observation::Missing, 4, Gone),
            ],
        ),
        case(
            "path back with another inode",
            "abc\n",
            vec![(Observation::Missing, 4, Gone), (file(2, "new\n"), 4, Back)],
        ),
        case(
            "path back with the same inode and content",
            "abc\n",
            vec![(Observation::Missing, 4, Gone), (file(1, "abc\n"), 4, Back)],
        ),
        case(
            "the file that came back is followed",
            "abc\n",
            vec![
                (Observation::Missing, 4, Gone),
                (file(2, "n\n"), 4, Back),
                (file(2, "n\nm\n"), 2, Grew),
                (file(2, "n\nm\n"), 4, Unchanged),
            ],
        ),
        case(
            "several rotations between two looks are one replacement",
            "abc\n",
            vec![
                (file(3, "newest\n"), 4, Replaced),
                (file(3, "newest\n"), 0, Grew),
                (file(3, "newest\n"), 7, Unchanged),
            ],
        ),
        case(
            "the replaced inode coming back is another replacement",
            "abc\n",
            vec![
                (file(2, "new\n"), 4, Replaced),
                (file(1, "abc\n"), 4, Replaced),
            ],
        ),
        case(
            "inode reused after delete and create, caught by the first bytes",
            "abc\n",
            vec![(file(1, "new file\n"), 4, Truncated)],
        ),
        case(
            "shrinks to 0, then grows",
            "abc\n",
            vec![
                (file(1, ""), 4, Truncated),
                (file(1, ""), 0, Unchanged),
                (file(1, "x\n"), 0, Grew),
                (file(1, "x\ny\n"), 2, Grew),
            ],
        ),
        case(
            "after a truncation the new first bytes count",
            "abc\n",
            vec![
                (file(1, "xyz\nmore\n"), 4, Truncated),
                (file(1, "xyz\nmore\nmore\n"), 9, Grew),
            ],
        ),
        case(
            "first bytes are learnt as the file grows",
            "",
            vec![
                (file(1, "abc\n"), 0, Grew),
                (file(1, "xbc\nmore\n"), 4, Truncated),
            ],
        ),
    ];

    let mut wrong = Vec::new();
    for Case { name, start, steps } in cases {
        let mut tracker = Tracker::new(snapshot(1, start.as_bytes()));
        for (step, (observation, read_offset, expected)) in steps.into_iter().enumerate() {
            let got = tracker.observe(&observation, read_offset);
            if got != expected {
                wrong.push(format!(
                    "{name}, step {step}: {got:?}, expected {expected:?}"
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Any snapshot, consistent with itself or not; a two-letter alphabet makes first
/// bytes that match each other common.
fn any_snapshot() -> impl Strategy<Value = Snapshot> {
    (
        0..3u64,
        0..600u64,
        prop::collection::vec(prop::sample::select(b"ab".to_vec()), 0..300),
    )
        .prop_map(|(inode, size, prefix)| Snapshot {
            inode,
            size,
            prefix,
        })
}

fn any_observation() -> impl Strategy<Value = Observation> {
    prop_oneof![
        1 => Just(Observation::Missing),
        4 => any_snapshot().prop_map(Observation::Present),
    ]
}

proptest! {
    /// Whatever the sizes, offsets and first bytes, the path alone decides `Gone`,
    /// `Back` and `Replaced`.
    #[test]
    fn missing_paths_and_inodes_decide_gone_back_and_replaced(
        start in any_snapshot(),
        steps in prop::collection::vec((any_observation(), 0..700u64), 0..40),
    ) {
        let mut tracker = Tracker::new(start.clone());
        let mut followed = start.inode;
        let mut was_missing = false;
        for (observation, read_offset) in &steps {
            let change = tracker.observe(observation, *read_offset);
            match observation {
                Observation::Missing => prop_assert_eq!(change, Gone),
                Observation::Present(_) if was_missing => prop_assert_eq!(change, Back),
                Observation::Present(seen) if seen.inode != followed => {
                    prop_assert_eq!(change, Replaced)
                }
                Observation::Present(_) => {
                    prop_assert!(matches!(change, Unchanged | Grew | Truncated), "{:?}", change)
                }
            }
            was_missing = matches!(observation, Observation::Missing);
            if let Observation::Present(seen) = observation {
                followed = seen.inode;
            }
        }
    }

    /// Looking twice at the same file, read to its end the second time, finds
    /// nothing new, whatever came before.
    #[test]
    fn the_same_file_read_to_its_end_is_unchanged(
        start in any_snapshot(),
        steps in prop::collection::vec((any_observation(), 0..700u64), 0..20),
        last in any_snapshot(),
        read_offset in 0..700u64,
    ) {
        let mut tracker = Tracker::new(start);
        for (observation, offset) in &steps {
            tracker.observe(observation, *offset);
        }
        let size = last.size;
        let last = Observation::Present(last);
        tracker.observe(&last, read_offset);
        prop_assert_eq!(tracker.observe(&last, size), Unchanged);
    }
}
