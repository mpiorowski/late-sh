//! The look: one vision call at pick time that describes the day's
//! picture for readers who cannot see it and labels what it shows
//! (`AUDIENCE.md`, the herald's "The look"). The model gets the picture
//! whole and in four quarters: a 1536-pixel board reaches it shrunk, and
//! only the quarters let it see a small piece (a doodle the whole picture
//! hid in the hand run over the gallery). Returns the `Look` or an error;
//! `svc.rs` logs and counts, and posts an error as `Look::Unseen`.

use std::io::Cursor;

use anyhow::{Context, bail};
use image::{ImageFormat, ImageReader};
use serde::Deserialize;
use serde_json::json;

use super::state::{ALT_MAX_CHARS, Look, Picture, Warning};
use crate::app::ai::svc::{AI_MODEL, AiService};

/// Tuned by hand over the gallery's archived boards: words never set a
/// label (the room writes "boobs" more than it draws them), identity
/// never does, and a character is named only when the model is sure.
const BOARD_SYSTEM_PROMPT: &str = r#"You write the alt text and the content label for one picture that late.sh posts on Mastodon and Bluesky.

The picture is a day of the late.sh Artboard: a shared canvas in a terminal that anyone in the room can draw on, so it is usually a collage of many unrelated pieces by different people, overlapping, some finished and some abandoned. Everything is drawn in coloured text glyphs (letters, digits, block and box-drawing characters, symbols) on a dark navy background. Shapes are built from characters, and photos converted to glyphs look like coarse coloured mosaics; read them as the subject they show, not as text.

alt: a description for a reader who cannot see the picture, in plain sentences. Open with the overall impression in one sentence, then name the largest or most striking pieces and where they sit (top left, centre, along the bottom). Name a character, a person or a logo only when you are sure of it (its title is drawn with it, or the design is unmistakable); otherwise describe what you see. Quote a short word or phrase only when it is drawn large as part of the art. Never quote or mention usernames, handles or people's names written on the board, nothing with an @ sign. Do not open with "ASCII art", "An image of" or "This picture"; a prefix is added for you. No markdown. At most 800 characters.

warning: one label for the most serious thing drawn anywhere in the picture, however small or partly covered. Look at every region in turn, including small pieces at the edges and shapes inside larger pieces. Label what is drawn, not what is written: small text, chat lines and jokes in words never set a label; the one exception is a slur or hate phrase drawn large enough to read at a glance, which is offensive. Words, flags and symbols of identity (gay, a pride or trans flag) are never a label.
- none: nothing below.
- nudity: a bare breast, buttocks or nude body drawn without a sexual purpose.
- suggestive: a body drawn to be sexual but without genitals or a sex act: a pin-up pose, lingerie, exaggerated sexual anatomy.
- explicit: genitals (a drawn penis or vulva, even as a joke or a doodle), a sex act, or nudity posed for sex.
- gore: graphic blood, open wounds, mutilation, dead bodies.
- offensive: a slur, a hate symbol, or harassment of a real person.
When more than one applies, pick the first in this order: explicit, gore, nudity, suggestive, offensive. When unsure between none and a label, give the label.

reason: for any label but none, the piece that set it and where it sits, in one short sentence; empty for none.

The label is not a verdict on whether to post: the picture is posted either way, and the label decides only the warning readers see before it."#;

/// The model's answer as the schema holds it.
#[derive(Deserialize)]
struct Answer {
    alt: String,
    warning: String,
    reason: String,
}

/// Ask the model about `png`, the picture `picture` names. AI off is
/// `Unseen` without a call; a call that fails or answers nothing usable
/// is an error.
pub(crate) async fn look(ai: &AiService, picture: &Picture, png: &[u8]) -> anyhow::Result<Look> {
    if !ai.is_enabled() {
        return Ok(Look::Unseen);
    }
    let whole = png.to_vec();
    let quarters = tokio::task::spawn_blocking(move || quarters(&whole)).await??;
    let mut images: Vec<&[u8]> = vec![png];
    images.extend(quarters.iter().map(Vec::as_slice));
    let answer = ai
        .generate_json_with_images(
            AI_MODEL,
            system_prompt(picture),
            &images,
            &prompt(picture),
            schema(),
        )
        .await
        .context("asking the model")?;
    match answer {
        Some(text) => parse(&text),
        None => bail!("the model gave no answer"),
    }
}

fn system_prompt(picture: &Picture) -> &'static str {
    match picture {
        Picture::Board(_) => BOARD_SYSTEM_PROMPT,
    }
}

fn prompt(picture: &Picture) -> String {
    match picture {
        Picture::Board(board) => format!(
            "Label this picture: the late.sh Artboard as the room left it on {}. The first image is the whole board; the next four are its quarters at full detail (top left, top right, bottom left, bottom right), for reading small pieces. Describe and label the whole board.",
            board.shown.format("%-d %B %Y"),
        ),
    }
}

/// The answer's shape. `warning`'s values are `parse`'s, one for one.
pub(crate) fn schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "alt": { "type": "string" },
            "warning": {
                "type": "string",
                "enum": ["none", "nudity", "suggestive", "explicit", "gore", "offensive"],
            },
            "reason": { "type": "string" },
        },
        "required": ["alt", "warning", "reason"],
        "propertyOrdering": ["alt", "warning", "reason"],
    })
}

/// The answer as a `Look`: the warning one of the closed set, the alt
/// text trimmed and cut to `ALT_MAX_CHARS`. An unknown warning or an
/// empty description is refused, never guessed.
pub(crate) fn parse(text: &str) -> anyhow::Result<Look> {
    let answer: Answer = serde_json::from_str(text).context("decoding the look's answer")?;
    let warning = match answer.warning.as_str() {
        "none" => None,
        "nudity" => Some(Warning::Nudity),
        "suggestive" => Some(Warning::Suggestive),
        "explicit" => Some(Warning::Explicit),
        "gore" => Some(Warning::Gore),
        "offensive" => Some(Warning::Offensive),
        other => bail!("unknown warning {other:?}"),
    };
    let alt = answer.alt.trim();
    if alt.is_empty() {
        bail!("the look's alt text is empty");
    }
    let alt = if alt.chars().count() > ALT_MAX_CHARS {
        let mut cut: String = alt.chars().take(ALT_MAX_CHARS - 1).collect();
        cut.push('…');
        cut
    } else {
        alt.to_string()
    };
    Ok(Look::Seen {
        alt,
        warning,
        reason: answer.reason.trim().to_string(),
    })
}

/// The picture's four quarters as PNGs: top left, top right, bottom left,
/// bottom right. An odd side gives its extra pixel to the right or bottom
/// quarters, so every pixel is in one.
pub(crate) fn quarters(png: &[u8]) -> anyhow::Result<[Vec<u8>; 4]> {
    let image = ImageReader::with_format(Cursor::new(png), ImageFormat::Png)
        .decode()
        .context("decoding the picture")?;
    let (width, height) = (image.width(), image.height());
    let (left, top) = (width / 2, height / 2);
    let corners = [
        (0, 0, left, top),
        (left, 0, width - left, top),
        (0, top, left, height - top),
        (left, top, width - left, height - top),
    ];
    let mut out: [Vec<u8>; 4] = Default::default();
    for (slot, (x, y, w, h)) in out.iter_mut().zip(corners) {
        image
            .crop_imm(x, y, w, h)
            .write_to(&mut Cursor::new(slot), ImageFormat::Png)
            .context("encoding a quarter")?;
    }
    Ok(out)
}
