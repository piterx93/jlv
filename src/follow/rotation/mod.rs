//! What happened to the followed file between two looks at its path, `tail -F` style.
//! Pure logic: the caller stats the path, reads the first bytes and says how far it
//! has read; [`Tracker::observe`] tells it what to do next.

#[cfg(test)]
mod tests;

/// How many bytes from the start of the file an observation carries.
pub const PREFIX_LEN: usize = 256;

/// What a look at the path found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    Present(Snapshot),
    Missing,
}

/// The file at the path when it was looked at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub inode: u64,
    pub size: u64,
    /// The first `min(size, PREFIX_LEN)` bytes of the file.
    pub prefix: Vec<u8>,
}

/// What the caller should do after an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Nothing new to read.
    Unchanged,
    /// The file has bytes past the read offset; read on from there.
    Grew,
    /// The file was cut short, and maybe written again since: read it from the start.
    Truncated,
    /// Another file now lives at the path: finish reading the open one, then open the
    /// path and read it from the start.
    Replaced,
    /// Nothing at the path: finish reading the open file, then look again later.
    Gone,
    /// A file is at the path again after [`Gone`](Change::Gone): finish the open file
    /// if that wasn't done on `Gone`, then open the path and read it from the start.
    Back,
}

/// The state machine: which file the caller follows and what its first bytes were.
#[derive(Debug, Clone)]
pub struct Tracker {
    inode: u64,
    prefix: Vec<u8>,
    gone: bool,
}

impl Tracker {
    /// Follows the file the caller has just opened, described by `opened`.
    pub fn new(opened: Snapshot) -> Self {
        Self {
            inode: opened.inode,
            prefix: opened.prefix,
            gone: false,
        }
    }

    /// Takes in a new look at the path. `read_offset` is how many bytes of the
    /// followed file the caller has read so far. The first rule that applies wins:
    ///
    /// - nothing at the path: [`Gone`](Change::Gone), as long as it stays missing;
    /// - a file at the path after `Gone`: [`Back`](Change::Back);
    /// - another inode than the followed file's: [`Replaced`](Change::Replaced);
    /// - smaller than `read_offset`, or the first bytes seen last time are no longer
    ///   the start of the file: [`Truncated`](Change::Truncated);
    /// - larger than `read_offset`: [`Grew`](Change::Grew);
    /// - otherwise [`Unchanged`](Change::Unchanged).
    ///
    /// After [`Truncated`](Change::Truncated) the caller reads from offset 0. After
    /// [`Replaced`](Change::Replaced) and [`Back`](Change::Back) the tracker follows
    /// the file in `observation`, and the offsets passed from then on are offsets
    /// into that file.
    pub fn observe(&mut self, observation: &Observation, read_offset: u64) -> Change {
        let Observation::Present(seen) = observation else {
            self.gone = true;
            return Change::Gone;
        };
        let change = if self.gone {
            Change::Back
        } else if seen.inode != self.inode {
            Change::Replaced
        } else if seen.size < read_offset || !seen.prefix.starts_with(&self.prefix) {
            Change::Truncated
        } else if seen.size > read_offset {
            Change::Grew
        } else {
            Change::Unchanged
        };
        // Whatever changed, the file just seen is the one followed from now on, and its
        // first bytes are what the next look is checked against.
        self.gone = false;
        self.inode = seen.inode;
        self.prefix.clone_from(&seen.prefix);
        change
    }
}
