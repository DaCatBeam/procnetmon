//! Helpers shared by the unit tests.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ratatui::buffer::Buffer;
use tracing_subscriber::{EnvFilter, fmt::MakeWriter};

/// A real `/proc/net/dev`, captured on a Linux 6.18 VM: `lo`, two `ifb` devices, and `eth0`.
pub const CAPTURE: &str = include_str!("../tests/fixtures/linux-6.18.txt");

/// A file in the system's temp folder, removed again when dropped.
pub struct TempFile(PathBuf);

impl TempFile {
    /// Writes `contents` to a new temp file. `name` must differ between tests, which run in
    /// parallel.
    pub fn new(name: &str, contents: impl AsRef<[u8]>) -> Self {
        let file = format!("procnetmon-{}-{name}", std::process::id());
        let path = std::env::temp_dir().join(file);
        fs::write(&path, contents).expect("the temp folder is writable");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Best effort: a file left in the temp folder is harmless.
        let _ = fs::remove_file(&self.0);
    }
}

/// A buffer's rows as text, one string per row.
pub fn lines(buffer: &Buffer) -> Vec<String> {
    buffer
        .content
        .chunks(usize::from(buffer.area.width))
        .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
        .collect()
}

/// Collects log output, for assertions about what gets logged.
#[derive(Clone, Default)]
pub struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
    /// Runs `f` with logs at `filter` (like `RUST_LOG`) coming here, and returns what was written.
    pub fn capture(filter: &str, f: impl FnOnce()) -> String {
        let logs = Self::default();
        let subscriber = crate::logging::subscriber(logs.clone(), EnvFilter::new(filter));
        tracing::subscriber::with_default(subscriber, f);
        let bytes = logs.0.lock().expect("no test panicked mid-write").clone();
        String::from_utf8(bytes).expect("logs are UTF-8")
    }
}

impl io::Write for Logs {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("no test panicked mid-write")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl MakeWriter<'_> for Logs {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}
