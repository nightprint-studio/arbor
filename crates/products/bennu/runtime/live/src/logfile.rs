//! Reading a log FILE incrementally: where to start reading, which bytes make whole lines, and what to
//! hand back so the next call gets only what was appended since. The file I/O and the decoding stay with
//! the caller; this is the arithmetic, which is where the rotation and truncation bugs live.

/// Where to read from, and what that means for the caller to tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub start: u64,
    /// `since_offset` was past the end: the file was truncated or rotated, so it is read from its tail.
    pub rotated: bool,
    /// Bytes between `since_offset` and `start` that were not read (more was appended than one read takes).
    pub skipped: u64,
    /// `start` may fall inside a line, so the first partial line must be dropped.
    pub mid_line: bool,
}

/// Plan a read of a `size`-byte file, at most `max_bytes`, resuming after `since` if given.
pub fn plan(size: u64, since: Option<u64>, max_bytes: u64) -> Plan {
    let tail_start = size.saturating_sub(max_bytes);
    match since {
        None => Plan { start: tail_start, rotated: false, skipped: 0, mid_line: tail_start > 0 },
        Some(o) if o > size => Plan { start: tail_start, rotated: true, skipped: 0, mid_line: tail_start > 0 },
        Some(o) if size - o > max_bytes => Plan { start: tail_start, rotated: false, skipped: tail_start - o, mid_line: true },
        // Resuming at a line boundary: `next_offset` is only ever one.
        Some(o) => Plan { start: o, rotated: false, skipped: 0, mid_line: false },
    }
}

/// One whole line: where it starts in the file, its bytes without the line ending.
#[derive(Debug, PartialEq, Eq)]
pub struct FileLine<'a> {
    pub offset: u64,
    pub bytes: &'a [u8],
}

#[derive(Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    pub lines: Vec<FileLine<'a>>,
    /// Where the next call resumes: just after the last whole line. A line still being written is left for it.
    pub next_offset: u64,
}

/// Cut `bytes` (read from file offset `base`) into whole lines.
pub fn split_lines(bytes: &[u8], base: u64, mid_line: bool) -> Chunk<'_> {
    let mut lines = Vec::new();
    let mut at = 0usize;
    if mid_line {
        // The read began inside a line: its start is unknown, so it is not a line.
        match bytes.iter().position(|&b| b == b'\n') {
            Some(i) => at = i + 1,
            None => return Chunk { lines, next_offset: base + bytes.len() as u64 },
        }
    }
    while let Some(i) = bytes[at..].iter().position(|&b| b == b'\n') {
        let mut line = &bytes[at..at + i];
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        lines.push(FileLine { offset: base + at as u64, bytes: line });
        at += i + 1;
    }
    Chunk { lines, next_offset: base + at as u64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_read_takes_the_tail() {
        assert_eq!(plan(100, None, 1000), Plan { start: 0, rotated: false, skipped: 0, mid_line: false });
        assert_eq!(plan(5000, None, 1000), Plan { start: 4000, rotated: false, skipped: 0, mid_line: true });
    }

    #[test]
    fn resuming_reads_only_what_was_appended() {
        assert_eq!(plan(150, Some(100), 1000), Plan { start: 100, rotated: false, skipped: 0, mid_line: false });
        assert_eq!(plan(100, Some(100), 1000).start, 100);
    }

    #[test]
    fn an_offset_past_the_end_means_the_file_was_rotated() {
        let p = plan(40, Some(900), 1000);
        assert!(p.rotated);
        assert_eq!(p.start, 0);
    }

    #[test]
    fn a_huge_append_reads_the_tail_and_counts_what_it_skipped() {
        let p = plan(10_000, Some(100), 1000);
        assert_eq!((p.start, p.skipped, p.mid_line), (9_000, 8_900, true));
    }

    #[test]
    fn whole_lines_only_and_the_unfinished_one_waits() {
        let c = split_lines(b"one\r\ntwo\nthr", 10, false);
        assert_eq!(c.lines.iter().map(|l| (l.offset, l.bytes)).collect::<Vec<_>>(), vec![(10, &b"one"[..]), (15, &b"two"[..])]);
        assert_eq!(c.next_offset, 19);
    }

    #[test]
    fn a_read_that_starts_mid_line_drops_the_partial_head() {
        let c = split_lines(b"tial line\nnext\n", 100, true);
        assert_eq!(c.lines.len(), 1);
        assert_eq!(c.lines[0].offset, 110);
        let none = split_lines(b"no newline at all", 100, true);
        assert!(none.lines.is_empty());
        assert_eq!(none.next_offset, 100 + 17);
    }
}
