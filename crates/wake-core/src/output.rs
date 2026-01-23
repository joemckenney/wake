const MAX_OUTPUT_BYTES: usize = 100 * 1024; // 100KB

#[derive(Debug)]
pub struct OutputBuffer {
    raw: Vec<u8>,
    truncated: bool,
}

impl OutputBuffer {
    pub fn new() -> Self {
        Self { raw: Vec::new(), truncated: false }
    }

    pub fn append(&mut self, data: &[u8]) {
        let remaining = MAX_OUTPUT_BYTES.saturating_sub(self.raw.len());
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
        let mut buf = OutputBuffer::new();
        // Append more than MAX_OUTPUT_BYTES
        let chunk = vec![b'x'; 50 * 1024]; // 50KB
        buf.append(&chunk);
        buf.append(&chunk); // 100KB total
        buf.append(&chunk); // This should be truncated

        let result = buf.finish();
        assert_eq!(result.raw.len(), MAX_OUTPUT_BYTES);
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
