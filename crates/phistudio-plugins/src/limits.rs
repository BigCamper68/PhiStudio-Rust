use std::io::{self, Write};

struct SizeLimit {
    used: usize,
    limit: usize,
}
impl Write for SizeLimit {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.used) {
            return Err(io::Error::other("Serialized data exceeds size limit"));
        }
        self.used += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Check wire size without allocating an encoded copy of untrusted data.
pub(crate) fn serialized_size(
    value: &(impl serde::Serialize + ?Sized),
    limit: usize,
) -> Option<usize> {
    let mut size = SizeLimit { used: 0, limit };
    serde_json::to_writer(&mut size, value).ok()?;
    Some(size.used)
}
