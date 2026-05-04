//! Line-oriented parsing primitives shared by CircleMUD's `.wld`, `.mob`,
//! and (future) `.obj` parsers. The stock formats are all built from the
//! same building blocks: tilde-terminated strings, optionally multi-line;
//! one-statement-per-line headers; and `~`-comment / blank lines tolerated
//! between records. Centralising them avoids three near-identical
//! implementations diverging over time.

use anyhow::{Result, anyhow};
use std::path::Path;

use crate::import::SourceLoc;

pub struct LineParser<'a> {
    src: &'a str,
    pos: usize,
    pub line: u32,
    path: &'a Path,
}

impl<'a> LineParser<'a> {
    pub fn new(src: &'a str, path: &'a Path) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            path,
        }
    }

    pub fn path(&self) -> &Path {
        self.path
    }

    pub fn err(&self, msg: &str) -> anyhow::Error {
        anyhow!("{}:{}: {}", self.path.display(), self.line, msg)
    }

    pub fn loc(&self) -> SourceLoc {
        SourceLoc::file(self.path.to_path_buf()).with_line(self.line)
    }

    pub fn at_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    pub fn peek_line(&self) -> Option<&'a str> {
        if self.pos >= self.src.len() {
            return None;
        }
        let rest = &self.src[self.pos..];
        let end = rest.find('\n').unwrap_or(rest.len());
        let line = &rest[..end];
        Some(line.strip_suffix('\r').unwrap_or(line))
    }

    pub fn consume_line(&mut self) -> Option<&'a str> {
        if self.pos >= self.src.len() {
            return None;
        }
        let rest = &self.src[self.pos..];
        match rest.find('\n') {
            Some(end) => {
                let line = &rest[..end];
                let line = line.strip_suffix('\r').unwrap_or(line);
                self.pos += end + 1;
                self.line += 1;
                Some(line)
            }
            None => {
                let line = rest;
                self.pos = self.src.len();
                Some(line)
            }
        }
    }

    pub fn skip_blank(&mut self) {
        while let Some(line) = self.peek_line() {
            if line.trim().is_empty() {
                self.consume_line();
            } else {
                break;
            }
        }
    }

    /// Read a tilde-terminated string. Lines are concatenated with `\n`. The
    /// terminator may be either `text~` on a single line or a lone `~` after
    /// arbitrary lines of content. Anything on the terminator line *after*
    /// the tilde is dropped, matching `fread_string` in CircleMUD's `db.c`.
    pub fn read_string(&mut self) -> Result<String> {
        let mut buf = String::new();
        loop {
            let Some(line) = self.consume_line() else {
                return Err(self.err("unexpected EOF inside ~-terminated string"));
            };
            if let Some(idx) = line.find('~') {
                let head = &line[..idx];
                if !head.is_empty() {
                    if !buf.is_empty() {
                        buf.push('\n');
                    }
                    buf.push_str(head);
                }
                return Ok(buf);
            }
            if !buf.is_empty() {
                buf.push('\n');
            }
            buf.push_str(line);
        }
    }
}
