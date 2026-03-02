/// Default max output size: 5MB
const DEFAULT_MAX_OUTPUT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug)]
pub struct OutputBuffer {
    raw: Vec<u8>,
    truncated: bool,
    max_bytes: usize,
}

impl OutputBuffer {
    pub fn new() -> Self {
        Self::with_max_bytes(DEFAULT_MAX_OUTPUT_BYTES)
    }

    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self { raw: Vec::new(), truncated: false, max_bytes }
    }

    pub fn append(&mut self, data: &[u8]) {
        let remaining = self.max_bytes.saturating_sub(self.raw.len());
        if remaining == 0 {
            self.truncated = true;
            return;
        }

        let to_append = data.len().min(remaining);
        self.raw.extend_from_slice(&data[..to_append]);
        if to_append < data.len() {
            self.truncated = true;
        }
    }

    pub fn finish(self) -> OutputResult {
        let stripped = strip_ansi_escapes::strip(&self.raw);
        let clean = String::from_utf8_lossy(&stripped).into_owned();

        OutputResult { raw: self.raw, clean, truncated: self.truncated }
    }

    pub fn clear(&mut self) {
        self.raw.clear();
        self.truncated = false;
    }
}

impl Default for OutputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct OutputResult {
    pub raw: Vec<u8>,
    pub clean: String,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_append() {
        let mut buf = OutputBuffer::new();
        buf.append(b"hello ");
        buf.append(b"world");
        let result = buf.finish();
        assert_eq!(result.clean, "hello world");
        assert!(!result.truncated);
    }

    #[test]
    fn test_buffer_truncation() {
        // Use a small max size for testing
        let max_bytes = 1024; // 1KB
        let mut buf = OutputBuffer::with_max_bytes(max_bytes);

        let chunk = vec![b'x'; 512]; // 512 bytes
        buf.append(&chunk);
        buf.append(&chunk); // 1KB total (at limit)
        buf.append(&chunk); // This should be truncated

        let result = buf.finish();
        assert_eq!(result.raw.len(), max_bytes);
        assert!(result.truncated);
    }

    #[test]
    fn test_buffer_with_custom_max_bytes() {
        let mut buf = OutputBuffer::with_max_bytes(100);
        buf.append(&[b'a'; 150]);

        let result = buf.finish();
        assert_eq!(result.raw.len(), 100);
        assert!(result.truncated);
    }

    #[test]
    fn test_ansi_stripping() {
        let mut buf = OutputBuffer::new();
        buf.append(b"\x1b[31mred text\x1b[0m normal");
        let result = buf.finish();
        assert_eq!(result.clean, "red text normal");
    }

    #[test]
    fn test_buffer_clear() {
        let mut buf = OutputBuffer::new();
        buf.append(b"data");
        buf.clear();
        let result = buf.finish();
        assert_eq!(result.clean, "");
        assert!(!result.truncated);
    }
}
