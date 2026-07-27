//! Syntax highlighting and the line-number gutter for the scratchpad body.
//!
//! `ratatui-textarea` has no hook for per-token styling (checked the vendored
//! source: its `Widget` impl supports exactly one uniform `Style` for the
//! whole buffer, plus cursor/selection/search overlays), so the scratchpad
//! stops rendering the editor as a widget and builds this output by hand
//! instead — see `ui.rs`. `TextArea` still owns all the actual editing state
//! (insert/delete/cursor/undo); this module only turns its current lines
//! into styled `Line`s.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::app::common::theme;

/// The scratchpad's curated language cycle. Deliberately short: syntect
/// bundles ~200 syntaxes, far too many to cycle through one at a time, so
/// this is the handful an actual pairing session would reach for. Rust is
/// first to match this app's own audience.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Language {
    #[default]
    Plain,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    C,
    Cpp,
    Java,
    Ruby,
    Bash,
    Json,
    Yaml,
}

const CYCLE: &[Language] = &[
    Language::Plain,
    Language::Rust,
    Language::Python,
    Language::JavaScript,
    Language::TypeScript,
    Language::Go,
    Language::C,
    Language::Cpp,
    Language::Java,
    Language::Ruby,
    Language::Bash,
    Language::Json,
    Language::Yaml,
];

impl Language {
    fn cycle_index(self) -> usize {
        CYCLE.iter().position(|l| *l == self).unwrap_or(0)
    }

    pub(crate) fn next(self) -> Self {
        CYCLE[(self.cycle_index() + 1) % CYCLE.len()]
    }

    /// Short label for the scratchpad header.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Language::Plain => "plain text",
            Language::Rust => "rust",
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::C => "c",
            Language::Cpp => "c++",
            Language::Java => "java",
            Language::Ruby => "ruby",
            Language::Bash => "bash",
            Language::Json => "json",
            Language::Yaml => "yaml",
        }
    }

    /// The file extension syntect's bundled default syntax set indexes by.
    /// `None` for `Plain`, which never touches syntect at all.
    fn extension(self) -> Option<&'static str> {
        match self {
            Language::Plain => None,
            Language::Rust => Some("rs"),
            Language::Python => Some("py"),
            Language::JavaScript => Some("js"),
            Language::TypeScript => Some("ts"),
            Language::Go => Some("go"),
            Language::C => Some("c"),
            Language::Cpp => Some("cpp"),
            Language::Java => Some("java"),
            Language::Ruby => Some("rb"),
            Language::Bash => Some("sh"),
            Language::Json => Some("json"),
            Language::Yaml => Some("yaml"),
        }
    }
}

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

fn active_theme() -> &'static Theme {
    &theme_set().themes["base16-ocean.dark"]
}

fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

/// Right-aligned gutter width for `total_lines`, e.g. 3 for anything up to
/// 999 lines. Matches `ratatui-textarea`'s own line-number sizing, which this
/// module otherwise has to reproduce by hand since it isn't rendering that
/// widget anymore.
fn gutter_width(total_lines: usize) -> usize {
    total_lines.max(1).to_string().len()
}

/// Full gutter prefix width (digits plus the trailing separator space), so
/// callers positioning the real cursor agree with what `highlighted_lines`
/// actually rendered.
pub(crate) fn gutter_prefix_width(total_lines: usize) -> usize {
    gutter_width(total_lines) + 1
}

/// Right-aligned line-number `Line`s, one per source line, meant for a
/// fixed-width column that never scrolls horizontally. Kept as its own
/// `Paragraph` in `ui.rs` rather than a prefix baked into each content line:
/// `Paragraph::scroll`'s horizontal offset applies uniformly to the whole
/// widget, so a gutter sharing the same widget as the content would scroll
/// away sideways right along with it the moment a long line pushed the
/// cursor past the right edge -- exactly the wrong thing for line numbers,
/// which real editors always keep pinned.
pub(crate) fn gutter_lines(total_lines: usize) -> Vec<Line<'static>> {
    let width = gutter_width(total_lines);
    let gutter_style = Style::default().fg(theme::TEXT_DIM());
    (1..=total_lines.max(1))
        .map(|n| Line::from(Span::styled(format!("{n:>width$} "), gutter_style)))
        .collect()
}

/// The rendered body, kept between frames so a render that changed nothing
/// about the text does not re-run syntect over the whole buffer.
///
/// Highlighting is by far the most expensive thing this app draws: measured on
/// a release build, a full 8000-char buffer (`MAX_CHARS`, ~200 lines) costs
/// ~23ms per frame against ~15us for the plain-text path, and the render loop's
/// whole input-to-frame budget is 15ms. Nearly all of that is syntect's parse,
/// not the span building, so there is no cheap way to redo part of it: the win
/// is not redoing it at all. Cursor moves, scrolling, resizes, and partner
/// cursor updates all re-render without touching the text, and arrow keys
/// publish (bumping `revision`) without changing content, which is why this
/// keys on the lines themselves rather than the revision.
#[derive(Default)]
pub(crate) struct HighlightCache {
    entry: Option<CacheEntry>,
    /// How many times syntect has actually run. Not used in the render path;
    /// caching is this type's entire behavior, so the tests assert on it.
    parses: u64,
}

struct CacheEntry {
    lines: Vec<String>,
    language: Language,
    /// Exclusive line index this render stops at.
    parsed_through: usize,
    rendered: Vec<Line<'static>>,
}

impl CacheEntry {
    fn build(lines: &[String], language: Language, visible_end: usize) -> Self {
        let parsed_through = visible_end.min(lines.len());
        Self {
            lines: lines.to_vec(),
            language,
            parsed_through,
            rendered: highlighted_lines(&lines[..parsed_through], language),
        }
    }

    fn covers(&self, lines: &[String], language: Language, visible_end: usize) -> bool {
        // Reaching further down the buffer than the viewport needs is still a
        // hit: `Paragraph` draws only what fits, so the extra tail is inert.
        self.language == language
            && self.parsed_through >= visible_end.min(lines.len())
            && self.lines.as_slice() == lines
    }
}

impl HighlightCache {
    /// Styled body lines for the current buffer, down to `visible_end`
    /// (exclusive). Cloned out because `Paragraph` takes its text by value;
    /// cloning finished spans is three orders of magnitude cheaper than
    /// re-running syntect.
    pub(crate) fn body(
        &mut self,
        lines: &[String],
        language: Language,
        visible_end: usize,
    ) -> Vec<Line<'static>> {
        let entry = match self.entry.take() {
            Some(entry) if entry.covers(lines, language, visible_end) => entry,
            _ => {
                self.parses += 1;
                CacheEntry::build(lines, language, visible_end)
            }
        };
        let rendered = entry.rendered.clone();
        self.entry = Some(entry);
        rendered
    }

    pub(crate) fn parses(&self) -> u64 {
        self.parses
    }
}

/// Turn the editor's current lines into styled `Line`s, content only (no
/// gutter prefix -- see `gutter_lines`). `Language::Plain` never touches
/// syntect: every line becomes one theme-colored span, same cost as the old
/// plain-text render.
///
/// Callers pass only the lines down to the bottom of the viewport: syntect
/// parses from the first line regardless (multi-line strings and comments
/// carry state across lines), so the tail is what can be trimmed, never the
/// head.
pub(crate) fn highlighted_lines(lines: &[String], language: Language) -> Vec<Line<'static>> {
    let plain = |line: &String| {
        Line::from(Span::styled(
            line.clone(),
            Style::default().fg(theme::TEXT()),
        ))
    };

    let Some(extension) = language.extension() else {
        return lines.iter().map(plain).collect();
    };
    let Some(syntax) = syntax_set().find_syntax_by_extension(extension) else {
        return lines.iter().map(plain).collect();
    };

    let mut highlighter = HighlightLines::new(syntax, active_theme());
    lines
        .iter()
        .map(|line| {
            let with_newline = format!("{line}\n");
            let ranges = highlighter
                .highlight_line(&with_newline, syntax_set())
                .unwrap_or_default();
            let spans = ranges
                .into_iter()
                .map(|(style, text)| {
                    Span::styled(
                        text.trim_end_matches('\n').to_string(),
                        Style::default().fg(syntect_color_to_ratatui(style.foreground)),
                    )
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
#[path = "highlight_test.rs"]
mod highlight_test;
