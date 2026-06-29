use std::path::Path;

use anyhow::{Context, Result, anyhow};
use kdl::{KdlDocument, KdlError};

pub fn parse_document(path: &Path, source: &str) -> Result<KdlDocument> {
    source
        .parse::<KdlDocument>()
        .map_err(|error| anyhow!("{}", format_parse_error(path, source, &error)))
        .with_context(|| format!("parsing {}", path.display()))
}

pub fn format_parse_error(path: &Path, source: &str, error: &KdlError) -> String {
    let mut output = format!("KDL parse error in {}", path.display());

    if error.diagnostics.is_empty() {
        output.push_str(&format!("\n{error}"));
        return output;
    }

    for diagnostic in &error.diagnostics {
        let span = diagnostic.span;
        let position = SourcePosition::new(source, span.offset(), span.len());
        let message = diagnostic
            .message
            .as_deref()
            .unwrap_or("Unexpected KDL parse error");
        let label = diagnostic.label.as_deref().unwrap_or("here");

        output.push_str(&format!(
            "\n\n{}:{}:{}: {}",
            path.display(),
            position.line_number,
            position.column_number,
            message
        ));
        output.push_str(&position.format_snippet(label));

        if let Some(help) = &diagnostic.help {
            output.push_str(&format!("\n{}= help: {help}", position.gutter_padding()));
        }
    }

    output
}

struct SourcePosition<'a> {
    line_number: usize,
    column_number: usize,
    column_index: usize,
    highlight_width: usize,
    line_text: &'a str,
}

impl<'a> SourcePosition<'a> {
    fn new(source: &'a str, offset: usize, len: usize) -> Self {
        let offset = clamp_to_char_boundary(source, offset.min(source.len()));
        let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
        let line_end = source[offset..]
            .find('\n')
            .map_or(source.len(), |index| offset + index);
        let line_text = &source[line_start..line_end];
        let line_number = source[..line_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let column_index = source[line_start..offset].chars().count();
        let column_number = column_index + 1;
        let span_end = clamp_to_char_boundary(source, offset.saturating_add(len).min(line_end));
        let highlight_width = source[offset..span_end].chars().count().max(1);

        Self {
            line_number,
            column_number,
            column_index,
            highlight_width,
            line_text,
        }
    }

    fn format_snippet(&self, label: &str) -> String {
        let line_number_width = self.line_number.to_string().len();
        let prefix: String = self
            .line_text
            .chars()
            .take(self.column_index)
            .map(|ch| if ch == '\t' { '\t' } else { ' ' })
            .collect();
        format!(
            "\n  {line_number:>width$} | {line_text}\n  {padding:>width$} | {prefix}{carets} {label}",
            line_number = self.line_number,
            width = line_number_width,
            line_text = self.line_text,
            padding = "",
            prefix = prefix,
            carets = "^".repeat(self.highlight_width),
            label = label
        )
    }

    fn gutter_padding(&self) -> String {
        format!("  {} | ", " ".repeat(self.line_number.to_string().len()))
    }
}

fn clamp_to_char_boundary(source: &str, mut offset: usize) -> usize {
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
