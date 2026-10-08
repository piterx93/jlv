//! Reading a file's last entries from its end, long before the whole file is indexed.

use super::{Chunk, NO_TIMESTAMP, Record, Settings};

/// The records of the last `count` entries of `bytes`, the whole file, or of all its
/// entries if it has fewer: the same records, timestamps included, that end the
/// index of the whole file ([`Chunk::finish`](super::Chunk::finish)).
///
/// It reads the file backwards from its end, `window` bytes at first (at least 1),
/// and further back only as far as it has to: to the first line of the earliest of
/// those entries, and, when that entry has no timestamp of its own, to the entry it
/// takes one from. Any window gives the same records; a small one only takes more
/// steps.
///
/// `None` once it would have to read more than the last `limit` bytes: a file whose
/// lines don't start entries of the preset, one huge last line, or a long run of
/// entries without a timestamp would otherwise be read to its start.
pub fn tail(
    bytes: &[u8],
    count: usize,
    window: usize,
    limit: usize,
    settings: &Settings,
) -> Option<Vec<Record>> {
    let window = window.max(1);
    let floor = bytes.len().saturating_sub(limit);
    let mut tail = Chunk::empty();
    let mut start = bytes.len();
    while start > 0 && !holds_last(&tail, count, settings) {
        if start == floor {
            return None;
        }
        let end = start;
        start = end.saturating_sub(window).max(floor);
        let mut chunk = Chunk::index(bytes, 0, start..end, settings);
        chunk.join(tail);
        tail = chunk;
    }
    let mut records = tail.finish(settings);
    Some(records.split_off(records.len().saturating_sub(count)))
}

/// Whether `tail`, a chunk that runs to the end of the file, holds the file's last
/// `count` entries whole, and what the first of them takes its timestamp from.
fn holds_last(tail: &Chunk, count: usize, settings: &Settings) -> bool {
    if count == 0 {
        return true;
    }
    // A leading record continues an entry that starts further back.
    if tail.records.len() - usize::from(tail.leads_in) < count {
        return false;
    }
    let first = tail.records.len() - count;
    match settings.preset.entry_parser() {
        None => true,
        // Entries without a timestamp already carry the one before them in the chunk.
        Some(parser) if parser.has_date() => tail.records[first].timestamp != NO_TIMESTAMP,
        // Times of day are dated back from the end of the file, which is all here, so
        // a time at or before the first entry is enough.
        Some(_) => tail.times[..=first].iter().any(Option::is_some),
    }
}
