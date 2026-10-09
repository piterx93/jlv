use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use flate2::read::MultiGzDecoder;

use super::{OpenError, Origin};

const MAGIC: &[u8] = b"\x1f\x8b";

/// Whether `file` starts with the gzip magic bytes. Leaves the file at its start.
pub(super) fn is_gzip(file: &mut File) -> io::Result<bool> {
    let mut start = Vec::with_capacity(MAGIC.len());
    file.take(MAGIC.len() as u64).read_to_end(&mut start)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(start == MAGIC)
}

/// Decompresses the first `len` bytes of `file` into an anonymous temporary file in
/// `temp_dir`. The file is unlinked from the start, so the system reclaims it as
/// soon as it is closed, whether by drop, unwinding or the process being killed.
pub(super) fn decompress(
    file: File,
    len: u64,
    temp_dir: &Path,
    mut progress: impl FnMut(u64),
) -> Result<(File, Origin), OpenError> {
    let temp_error = |err| OpenError::TempFile {
        dir: temp_dir.to_owned(),
        err,
    };
    let mut temp = tempfile::tempfile_in(temp_dir).map_err(temp_error)?;
    let mut decoder = MultiGzDecoder::new(Counting {
        inner: file.take(len),
        read: 0,
        progress: &mut progress,
    });
    let mut buf = vec![0; 64 * 1024];
    let origin = loop {
        match decoder.read(&mut buf) {
            Ok(0) => break Origin::Gzip,
            Ok(n) => temp.write_all(&buf[..n]).map_err(temp_error)?,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) if is_damage(&err) => break Origin::TruncatedGzip,
            Err(err) => return Err(err.into()),
        }
    };
    progress(len);
    Ok((temp, origin))
}

/// Whether the error is flate2 telling that the compressed data ends early or is
/// corrupt, rather than the file failing to be read.
fn is_damage(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData
    )
}

/// A reader that tells `progress` the total bytes read after every read.
struct Counting<R, F> {
    inner: R,
    read: u64,
    progress: F,
}

impl<R: Read, F: FnMut(u64)> Read for Counting<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        (self.progress)(self.read);
        Ok(n)
    }
}
