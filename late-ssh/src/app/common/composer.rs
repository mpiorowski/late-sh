//! Legacy word-wrap helpers used for composer-height estimation and for
//! rendering read-only wrapped text (e.g. the profile bio). The interactive
//! composer/editor state lives in `ratatui_textarea::TextArea`.

#[derive(Clone, Debug)]
pub struct ComposerRow {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub fn build_composer_rows(text: &str, width: usize) -> Vec<ComposerRow> {
    let mut rows = Vec::new();
    let mut offset = 0;

    for paragraph in text.split('\n') {
        let wrapped = wrap_composer_paragraph(paragraph, width);
        if wrapped.is_empty() {
            rows.push(ComposerRow {
                text: String::new(),
                start: offset,
                end: offset,
            });
        } else {
            for (row_text, start, end) in wrapped {
                rows.push(ComposerRow {
                    text: row_text,
                    start: offset + start,
                    end: offset + end,
                });
            }
        }
        offset += paragraph.chars().count() + 1;
    }

    rows
}

fn wrap_composer_paragraph(paragraph: &str, width: usize) -> Vec<(String, usize, usize)> {
    if paragraph.is_empty() {
        return Vec::new();
    }
    if width == 0 {
        return vec![(String::new(), 0, 0)];
    }

    let chars: Vec<char> = paragraph.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + width).min(chars.len());
        if end == chars.len() {
            out.push((chars[start..end].iter().collect(), start, end));
            break;
        }

        let break_at = chars[start..end]
            .iter()
            .rposition(|ch| ch.is_whitespace())
            .map(|idx| start + idx);

        match break_at {
            Some(split) if split > start => {
                out.push((chars[start..split].iter().collect(), start, split));
                start = split + 1;
            }
            _ => {
                out.push((chars[start..end].iter().collect(), start, end));
                start = end;
            }
        }
    }

    out
}

pub fn composer_line_count(text: &str, width: usize) -> usize {
    if text.is_empty() {
        1
    } else {
        build_composer_rows(text, width).len().max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_rows_soft_wrap_words() {
        let rows = build_composer_rows("hello wide world", 8);
        let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts, vec!["hello", "wide", "world"]);
    }
}
