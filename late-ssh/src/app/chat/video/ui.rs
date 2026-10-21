//! The inline video's frame. The chat draw finds the thumbnail among its
//! rows and, while the video holds it, paints the picture black for mpv to
//! draw over, with the play's caption on the hint row under it. Once every
//! layer of the frame is drawn, `check_clear` looks at what became of that
//! black: mpv's frames sit above the text, so anything drawn over the
//! rectangle would end up under the video, and the video parks instead.

use std::time::Instant;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use super::state::{InlineVideo, WATCH_HINT, video_rect};
use crate::app::chat::state::InlineImagePreview;
use crate::app::chat::ui::ChatRowHit;
use crate::app::common::theme;

/// A YouTube thumbnail's rows in the chat: the picture, then the hint row
/// the caption takes over while its video is here.
pub(crate) fn with_watch_hint(picture: &InlineImagePreview) -> InlineImagePreview {
    let mut lines = picture.clone();
    lines.push(Line::from(Span::styled(
        WATCH_HINT,
        Style::default().fg(theme::AMBER_DIM()),
    )));
    lines
}

/// Over the chat rows just drawn into `area`: record where the video's
/// picture is (nowhere unless all of the thumbnail is on screen), paint it
/// black while the video holds it, and write the caption under it.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    hits: &[ChatRowHit],
    video: &InlineVideo,
    picture: Option<&InlineImagePreview>,
) {
    let rect = picture
        .and_then(|picture| video_rect(area, hits, video.message_id(), picture_size(picture)));
    video.set_drawn(rect);
    let Some(rect) = rect else {
        return;
    };
    if video.holds_rect() {
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Black)),
            rect,
        );
    }
    let caption = Rect::new(
        rect.x,
        rect.bottom(),
        area.right().saturating_sub(rect.x),
        1,
    );
    frame.render_widget(Clear, caption);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            video.caption(Instant::now()),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))),
        caption,
    );
}

/// After every layer of the frame: whether the rectangle the chat draw
/// painted is still exactly that black. Only a video that holds it is
/// checked; any other frame leaves it not clear.
pub(crate) fn check_clear(buffer: &Buffer, video: &InlineVideo) {
    let clear = match (video.drawn(), video.holds_rect()) {
        (Some(rect), true) => rect.positions().all(|position| {
            buffer.cell(position).is_some_and(|cell| {
                cell.symbol() == " "
                    && cell.bg == Color::Black
                    && cell.fg == Color::Reset
                    && cell.modifier.is_empty()
            })
        }),
        (Some(_), false) | (None, _) => false,
    };
    video.set_clear(clear);
}

/// The picture's (cols, rows) in cells.
fn picture_size(picture: &InlineImagePreview) -> (u16, u16) {
    let cols = picture.first().map_or(0, Line::width);
    (
        u16::try_from(cols).unwrap_or(u16::MAX),
        u16::try_from(picture.len()).unwrap_or(u16::MAX),
    )
}
