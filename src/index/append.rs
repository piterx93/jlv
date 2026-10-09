//! Keeping the index up to date while the file grows.

use jiff::Timestamp;
use jiff::civil::Time;

use super::{Chunk, NO_TIMESTAMP, Record, Settings, attach, set_timestamps};
use crate::format::DatesForward;

/// Brings `records` up to date with `bytes`, the whole file after more was appended
/// to it. `records` is the index of the file as it was, which ends where its last
/// record ends (an empty index for an empty file), as [`Chunk::finish`] or an earlier
/// call gave it; `bytes` starts with that file.
///
/// The entries before the last stay as they are. The last one is read again from its
/// start, as it may have gained continuation lines or its last line may have been
/// incomplete, and the entries after it are added. A last line without a line ending
/// is indexed as it is and read again once more is appended.
///
/// An entry without a timestamp takes the one of the entry before it. Under a
/// time-only preset the times of day are dated forward from the last dated entry, by
/// [`midnights_between`]; dates already given never shift. Entries with no dated
/// entry before them are dated as indexing the whole file dates them, back from the
/// anchor. So unless the appended times of day cross midnight, `records` ends up the
/// index of `bytes` that [`Chunk::finish`] gives.
///
/// [`Chunk::finish`]: super::Chunk::finish
/// [`midnights_between`]: crate::format::midnights_between
pub fn append(records: &mut Vec<Record>, bytes: &[u8], settings: &Settings) {
    let reread = records.pop();
    // The record lies within the file, which is mapped, so its offset fits a usize.
    let start = reread.map_or(0, |last| last.offset as usize);
    let chunk = Chunk::index(bytes, 0, start..bytes.len(), settings);
    let leads_in = chunk.leads_in;
    // The entry read again keeps its date, as it may be the only dated one so far.
    let appended = match reread.and_then(|last| last.timestamp()) {
        Some(from) => {
            let timestamps = dated_forward(&chunk.times, from, settings);
            let mut appended = chunk.records;
            set_timestamps(&mut appended, &timestamps);
            appended
        }
        None => chunk.finish(settings),
    };
    attach(records, &appended, leads_in);
}

/// The timestamps of the entries whose times of day are `times`, which follow an
/// entry at `from`: dated forward from it, as local time in the zone, and carried
/// forward to the entries without one. Those before the first time of day are left
/// to take the timestamp of the entry before them.
fn dated_forward(times: &[Option<Time>], from: Timestamp, settings: &Settings) -> Vec<i64> {
    let mut dates = DatesForward::new(settings.zone.to_datetime(from));
    let mut previous = NO_TIMESTAMP;
    times
        .iter()
        .map(|time| {
            if let Some(instant) = time.and_then(|time| dates.timestamp(time, &settings.zone)) {
                previous = instant.as_microsecond();
            }
            previous
        })
        .collect()
}
