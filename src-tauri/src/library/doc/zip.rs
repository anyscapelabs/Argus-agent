// The minimal zip container the OOXML writers share: one local file header
// per entry, a central directory, and a CRC so Word/Excel accept it. No
// compression — these payloads are already small.
pub(super) fn esc_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

pub(super) fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

pub(super) struct ZipEntry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

pub(super) struct ZipWriter {
    buf: Vec<u8>,
    entries: Vec<ZipEntry>,
}

impl ZipWriter {
    pub(super) fn new() -> Self {
        ZipWriter {
            buf: vec![],
            entries: vec![],
        }
    }

    pub(super) fn file(&mut self, name: &str, data: &[u8]) {
        let offset = self.buf.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name_bytes = name.as_bytes();
        self.buf.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        self.buf.extend_from_slice(&20u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf
            .extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(name_bytes);
        self.buf.extend_from_slice(data);
        self.entries.push(ZipEntry {
            name: name.into(),
            crc,
            size,
            offset,
        });
    }

    pub(super) fn finish(mut self) -> Vec<u8> {
        let dir_off = self.buf.len() as u32;
        let mut count: u32 = 0;
        for e in &self.entries {
            let name_bytes = e.name.as_bytes();
            self.buf.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
            self.buf.extend_from_slice(&20u16.to_le_bytes());
            self.buf.extend_from_slice(&20u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&e.crc.to_le_bytes());
            self.buf.extend_from_slice(&e.size.to_le_bytes());
            self.buf.extend_from_slice(&e.size.to_le_bytes());
            self.buf
                .extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u32.to_le_bytes());
            self.buf.extend_from_slice(&e.offset.to_le_bytes());
            self.buf.extend_from_slice(name_bytes);
            count += 1;
        }
        let dir_size = self.buf.len() as u32 - dir_off;
        self.buf.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        // Both counts are u16 in the format. Writing the u32 made the record
        // four bytes too long and no conformant reader could open the file.
        let n = count as u16;
        self.buf.extend_from_slice(&n.to_le_bytes());
        self.buf.extend_from_slice(&n.to_le_bytes());
        self.buf.extend_from_slice(&dir_size.to_le_bytes());
        self.buf.extend_from_slice(&dir_off.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf
    }
}

/// Word-wrap so a long line becomes several lines the renderer can place.
pub(super) fn wrap_lines(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            if cur.is_empty() {
                cur.push_str(word);
                continue;
            }
            if cur.len() + 1 + word.len() > width {
                lines.push(cur);
                cur = word.to_string();
                continue;
            }
            cur.push(' ');
            cur.push_str(word);
        }
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
