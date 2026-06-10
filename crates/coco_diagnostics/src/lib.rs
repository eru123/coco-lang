//! Beautiful error reporting for the Coco compiler.
//!
//! Uses `ariadne` to emit colored, annotated diagnostics with source context.

use ariadne::{Color, Label, Report, ReportKind, Source};
use coco_span::{FileId, SourceMap, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone)]
pub struct DiagnosticLabel {
    pub span: Span,
    pub message: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub file: FileId,
    pub labels: Vec<DiagnosticLabel>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(file: FileId, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            message: message.into(),
            file,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn warning(file: FileId, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            message: message.into(),
            file,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>, is_primary: bool) -> Self {
        self.labels.push(DiagnosticLabel {
            span,
            message: message.into(),
            is_primary,
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn emit(&self, source_map: &SourceMap) {
        let source_file = source_map
            .get_file(self.file)
            .expect("file not found in source map");

        let kind = match self.level {
            DiagnosticLevel::Error => ReportKind::Error,
            DiagnosticLevel::Warning => ReportKind::Warning,
            DiagnosticLevel::Note => ReportKind::Advice,
        };

        let mut report = Report::build(kind, source_file.path.display().to_string(), 0)
            .with_message(&self.message);

        for label in &self.labels {
            let color = if label.is_primary {
                Color::Red
            } else {
                Color::Blue
            };
            let ariadne_label = Label::new((
                source_file.path.display().to_string(),
                label.span.start..label.span.end,
            ))
            .with_message(&label.message)
            .with_color(color);
            report = report.with_label(ariadne_label);
        }

        for note in &self.notes {
            report = report.with_note(note);
        }

        let source = Source::from(&source_file.content);
        report
            .finish()
            .eprint((source_file.path.display().to_string(), source))
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_creation() {
        let diag = Diagnostic::error(FileId(0), "unexpected token")
            .with_label(Span::new(10, 15), "here", true)
            .with_note("expected ';'");

        assert_eq!(diag.level, DiagnosticLevel::Error);
        assert_eq!(diag.message, "unexpected token");
        assert_eq!(diag.labels.len(), 1);
        assert_eq!(diag.notes.len(), 1);
    }
}
