use std::{
    fs::File,
    io::{self, Read, Seek},
    path::PathBuf,
};

/// Keeps one handle open and rewinds it on every read, as `top` does with /proc files.
pub struct ProcNetMon {
    path: PathBuf,
    file: File,
    buf: String,
}

impl ProcNetMon {
    /// Opens the counters file, usually `/proc/net/dev`.
    ///
    /// # Errors
    ///
    /// If the file can't be opened.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, SourceError> {
        let path = path.into();
        let file = File::open(&path).map_err(|source| SourceError::Open {
            path: path.clone(),
            source,
        })?;
        Ok(Self {
            path,
            file,
            buf: String::with_capacity(4096),
        })
    }

    /// Re-reads the file and returns the fresh contents. The buffer's allocation is reused.
    ///
    /// # Errors
    ///
    /// If the file can't be read, or isn't UTF-8.
    pub fn read(&mut self) -> Result<&str, SourceError> {
        self.buf.clear();
        self.file
            .rewind()
            .and_then(|()| self.file.read_to_string(&mut self.buf))
            .map_err(|source| SourceError::Read {
                path: self.path.clone(),
                source,
            })?;
        Ok(&self.buf)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("could not open {}", .path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not read {}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[cfg(test)]
mod tests {
    use std::{error::Error, fs};

    use rstest::rstest;

    use super::*;
    use crate::test_support::{CAPTURE, TempFile};

    #[rstest]
    fn reads_the_whole_file() {
        let counters = TempFile::new("source-whole", CAPTURE);
        let mut source = ProcNetMon::open(counters.path()).expect("it exists");
        assert_eq!(source.read().expect("it's readable"), CAPTURE);
    }

    #[rstest]
    fn each_read_sees_the_files_latest_contents() {
        let counters = TempFile::new("source-reread", "first");
        let mut source = ProcNetMon::open(counters.path()).expect("it exists");
        assert_eq!(source.read().expect("readable"), "first");
        fs::write(counters.path(), "second, and longer").expect("writable");
        assert_eq!(source.read().expect("readable"), "second, and longer");
        // Shorter than before: nothing from the last read may linger in the buffer.
        fs::write(counters.path(), "3rd").expect("writable");
        assert_eq!(source.read().expect("readable"), "3rd");
    }

    #[rstest]
    fn a_missing_file_names_its_path() {
        let err = ProcNetMon::open("/no/such/counters")
            .err()
            .expect("there's nothing to open");
        assert!(matches!(err, SourceError::Open { .. }), "{err:?}");
        assert_eq!(err.to_string(), "could not open /no/such/counters");
        assert!(err.source().is_some(), "the io::Error stays attached");
    }

    #[rstest]
    fn a_file_that_isnt_text_fails_to_read() {
        let counters = TempFile::new("source-binary", b"\xff\xfe\xfd");
        let mut source = ProcNetMon::open(counters.path()).expect("it exists");
        let err = source.read().expect_err("not UTF-8");
        assert!(matches!(err, SourceError::Read { .. }), "{err:?}");
        assert!(err.to_string().starts_with("could not read "), "{err}");
    }
}
