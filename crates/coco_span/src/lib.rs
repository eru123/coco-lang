//! Source location tracking for the Coco compiler.
//!
//! This crate provides types for tracking locations in source files:
//! - `Span`: byte range in a source file
//! - `Location`: line and column position
//! - `SourceFile`: file content and metadata
//! - `SourceMap`: registry of all loaded files

use std::path::PathBuf;

/// A byte range in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn merge(&self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A line and column position in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: usize,   // 1-based
    pub column: usize, // 1-based
}

impl Location {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A unique identifier for a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub usize);

/// A source file with its content.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub path: PathBuf,
    pub content: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(id: FileId, path: PathBuf, content: String) -> Self {
        let line_starts = Self::compute_line_starts(&content);
        Self {
            id,
            path,
            content,
            line_starts,
        }
    }

    fn compute_line_starts(content: &str) -> Vec<usize> {
        let mut starts = vec![0];
        for (i, ch) in content.char_indices() {
            if ch == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    pub fn get_location(&self, offset: usize) -> Location {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };
        let line_start = self.line_starts[line];
        let column = offset - line_start;
        Location::new(line + 1, column + 1)
    }

    pub fn get_line(&self, line: usize) -> Option<&str> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        let start = self.line_starts[line - 1];
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.content.len());
        Some(&self.content[start..end])
    }

    pub fn get_span_text(&self, span: Span) -> &str {
        let start = span.start.min(self.content.len());
        let end = span.end.min(self.content.len());
        &self.content[start..end]
    }
}

/// Registry of all loaded source files.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, path: PathBuf, content: String) -> FileId {
        let id = FileId(self.files.len());
        let file = SourceFile::new(id, path, content);
        self.files.push(file);
        id
    }

    pub fn get_file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0)
    }

    pub fn get_location(&self, file: FileId, offset: usize) -> Option<Location> {
        self.get_file(file).map(|f| f.get_location(offset))
    }

    pub fn get_span_text(&self, file: FileId, span: Span) -> Option<&str> {
        self.get_file(file).map(|f| f.get_span_text(span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_span_merge() {
        let s1 = Span::new(5, 10);
        let s2 = Span::new(8, 15);
        assert_eq!(s1.merge(s2), Span::new(5, 15));
    }

    #[test]
    fn test_location() {
        let content = "hello\nworld\nfoo";
        let file = SourceFile::new(FileId(0), PathBuf::from("test.co"), content.to_string());

        assert_eq!(file.get_location(0), Location::new(1, 1)); // 'h'
        assert_eq!(file.get_location(5), Location::new(1, 6)); // '\n'
        assert_eq!(file.get_location(6), Location::new(2, 1)); // 'w'
        assert_eq!(file.get_location(12), Location::new(3, 1)); // 'f'
    }

    #[test]
    fn test_get_line() {
        let content = "line 1\nline 2\nline 3";
        let file = SourceFile::new(FileId(0), PathBuf::from("test.co"), content.to_string());

        assert_eq!(file.get_line(1), Some("line 1\n"));
        assert_eq!(file.get_line(2), Some("line 2\n"));
        assert_eq!(file.get_line(3), Some("line 3"));
        assert_eq!(file.get_line(0), None);
        assert_eq!(file.get_line(4), None);
    }
}
