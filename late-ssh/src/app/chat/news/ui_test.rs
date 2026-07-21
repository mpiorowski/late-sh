use super::{ArticleModalView, ascii_preview_if_fit, build_article_modal_lines};
use crate::app::chat::ui_text::NewsPayload;
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

fn lines_to_strings(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn ascii_preview_keeps_original_lines_when_fit() {
    let input = "abcd\nefgh\nijkl\nmnop";
    let out = ascii_preview_if_fit(input, 4, 2);
    assert_eq!(out, vec!["abcd".to_string(), "efgh".to_string()]);
}

#[test]
fn ascii_preview_hides_art_when_width_too_small() {
    let out = ascii_preview_if_fit("abcdef\n123456", 5, 6);
    assert!(out.is_empty());
}

#[test]
fn ascii_preview_returns_empty_for_empty_input() {
    assert!(ascii_preview_if_fit("", 10, 10).is_empty());
}

#[test]
fn ascii_preview_returns_empty_for_zero_dimensions() {
    assert!(ascii_preview_if_fit("abc", 0, 5).is_empty());
    assert!(ascii_preview_if_fit("abc", 5, 0).is_empty());
}

#[test]
fn ascii_preview_drops_blank_lines_before_width_check() {
    let out = ascii_preview_if_fit("\n   \n ab \n\\n cd  \n", 3, 6);
    assert_eq!(out, vec![" ab".to_string(), " cd".to_string()]);
}

#[test]
fn article_modal_lines_use_feed_style_without_news_emoji() {
    let payload = NewsPayload {
        title: "Nobody understands the point of hybrid cars".to_string(),
        summary: "YouTube video by Technology Connections.\nOpen the link to watch on YouTube."
            .to_string(),
        url: "https://www.youtube.com/watch?v=KnUFH5GX_fI".to_string(),
        ascii_art: ".. .-:::----\n. .:==-.....\n:-:--:     .\n-===---:   :\n      ..".to_string(),
    };
    let view = ArticleModalView {
        payload: &payload,
        meta: "@artboard - 12 mins ago - Wed 2026-05-06 20:12 UTC",
    };
    let rendered = lines_to_strings(&build_article_modal_lines(&view, 100));

    assert_eq!(rendered.first().map(String::as_str), Some(""));
    assert_eq!(rendered.last().map(String::as_str), Some(""));
    assert!(!rendered.join("\n").contains('📰'));
    assert!(rendered[1].contains(".. .-:::----"));
    assert!(rendered[1].contains("Nobody understands the point of hybrid cars"));
    assert!(rendered[2].contains("https://www.youtube.com/watch?v=KnUFH5GX_fI"));
    assert!(rendered[3].contains("@artboard - 12 mins ago - Wed 2026-05-06 20:12 UTC"));
}

#[test]
fn article_modal_lines_respect_terminal_cell_width() {
    let payload = NewsPayload {
        title: "Nobody understands the point of hybrid cars".to_string(),
        summary: "YouTube video by Technology Connections.\nOpen the link to watch on YouTube."
            .to_string(),
        url: "https://www.youtube.com/watch?v=KnUFH5GX_fI".to_string(),
        ascii_art: ".. .-:::----\n. .:==-.....\n:-:--:     .".to_string(),
    };
    let view = ArticleModalView {
        payload: &payload,
        meta: "@artboard - 12 mins ago - Wed 2026-05-06 20:12 UTC",
    };
    let width = 58;

    for rendered in lines_to_strings(&build_article_modal_lines(&view, width)) {
        assert!(
            UnicodeWidthStr::width(rendered.as_str()) <= width,
            "line overflowed {width} cells: {rendered:?}"
        );
    }
}

#[test]
fn article_modal_lines_tolerate_less_than_six_art_rows() {
    let payload = NewsPayload {
        title: "Tiny art".to_string(),
        summary: "One summary line.".to_string(),
        url: "https://example.com".to_string(),
        ascii_art: "\n  *  \n\n".to_string(),
    };
    let view = ArticleModalView {
        payload: &payload,
        meta: "@artboard - now - Wed 2026-05-06 20:12 UTC",
    };
    let rendered = lines_to_strings(&build_article_modal_lines(&view, 80)).join("\n");

    assert!(rendered.contains("  *"));
    assert!(rendered.contains("Tiny art"));
    assert!(rendered.contains("One summary line."));
}

#[test]
fn article_modal_lines_expand_each_summary_bullet_to_two_rows() {
    let payload = NewsPayload {
        title: "Modal expansion".to_string(),
        summary: [
            "First bullet has enough words to wrap into a second visible row in the modal.",
            "Second bullet also has enough words to use two visible rows in the modal.",
            "Third bullet should still appear with the same two row budget.",
            "Fourth bullet should not appear because the modal caps summary bullets.",
        ]
        .join("\n"),
        url: "https://example.com/news".to_string(),
        ascii_art: String::new(),
    };
    let view = ArticleModalView {
        payload: &payload,
        meta: "",
    };
    let rendered = lines_to_strings(&build_article_modal_lines(&view, 48));
    let body = rendered.join("\n");

    assert!(body.contains("First bullet"));
    assert!(body.contains("second visible"));
    assert!(body.contains("Second bullet"));
    assert!(body.contains("two visible"));
    assert!(body.contains("Third bullet"));
    assert!(!body.contains("Fourth bullet"));
}
