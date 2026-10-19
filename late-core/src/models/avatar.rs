//! A durable crop of the public board. Replacing or clearing an avatar never
//! changes the board; later painting and monthly wipes never change the crop.

use anyhow::{Context, Result, ensure};
use dartboard_core::{Canvas, Pos};
use tokio_postgres::Client;
use uuid::Uuid;

pub const AVATAR_MIN_DIMENSION: usize = 1;
pub const AVATAR_DEFAULT_WIDTH: usize = 16;
pub const AVATAR_DEFAULT_HEIGHT: usize = 8;
pub const AVATAR_MAX_DIMENSION: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    canvas: Canvas,
}

impl Avatar {
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    pub fn width(&self) -> usize {
        self.canvas.width
    }

    pub fn height(&self) -> usize {
        self.canvas.height
    }

    /// Only whole glyphs fit: neither half of a wide glyph crossing the
    /// frame's edge is included. Spaces, colours and cell positions survive.
    pub fn crop(board: &Canvas, origin: Pos, width: usize, height: usize) -> Result<Self> {
        ensure!(
            valid_dimensions(width, height),
            "Choose a frame from 1 to 32 cells on each axis."
        );
        ensure!(
            origin
                .x
                .checked_add(width)
                .is_some_and(|x| x <= board.width)
                && origin
                    .y
                    .checked_add(height)
                    .is_some_and(|y| y <= board.height),
            "Keep the whole frame on the board."
        );
        let mut canvas = Canvas::with_size(width, height);
        for y in origin.y..origin.y + height {
            for x in origin.x..origin.x + width {
                let pos = Pos { x, y };
                let Some(glyph) = board.glyph_at(pos) else {
                    continue;
                };
                if glyph.pos != pos || x + glyph.width > origin.x + width {
                    continue;
                }
                let local = Pos {
                    x: x - origin.x,
                    y: y - origin.y,
                };
                if let Some(fg) = glyph.fg {
                    canvas.put_glyph_colored(local, glyph.ch, fg);
                } else {
                    canvas.put_glyph(local, glyph.ch);
                }
            }
        }
        Self::from_canvas(canvas)
    }

    fn from_canvas(canvas: Canvas) -> Result<Self> {
        ensure!(
            valid_dimensions(canvas.width, canvas.height),
            "Invalid avatar dimensions."
        );
        let has_art = canvas.iter().any(|(pos, _)| {
            canvas
                .glyph_at(*pos)
                .is_some_and(|glyph| !glyph.ch.is_whitespace())
        });
        ensure!(
            has_art,
            "The frame is empty. Draw something on the board first."
        );
        Ok(Self { canvas })
    }

    pub async fn load(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        client
            .query_opt(
                "SELECT canvas FROM user_avatars WHERE user_id = $1",
                &[&user_id],
            )
            .await?
            .map(|row| {
                let canvas = serde_json::from_value(row.get("canvas")).context("decode avatar")?;
                Self::from_canvas(canvas)
            })
            .transpose()
    }

    pub async fn list_by_user_ids(
        client: &Client,
        user_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Self>> {
        let rows = client
            .query(
                "SELECT user_id, canvas FROM user_avatars WHERE user_id = ANY($1)",
                &[&user_ids],
            )
            .await?;
        rows.into_iter()
            .map(|row| {
                let canvas = serde_json::from_value(row.get("canvas")).context("decode avatar")?;
                Ok((row.get("user_id"), Self::from_canvas(canvas)?))
            })
            .collect()
    }

    pub async fn save(client: &Client, user_id: Uuid, avatar: Option<&Self>) -> Result<()> {
        match avatar {
            Some(avatar) => {
                let canvas = serde_json::to_value(avatar.canvas())?;
                client
                    .execute(
                        "INSERT INTO user_avatars (user_id, size, canvas) VALUES ($1, $2, $3)
                     ON CONFLICT (user_id) DO UPDATE SET size = EXCLUDED.size,
                         canvas = EXCLUDED.canvas, updated = current_timestamp",
                        &[&user_id, &(avatar.width() as i16), &canvas],
                    )
                    .await?;
            }
            None => {
                client
                    .execute("DELETE FROM user_avatars WHERE user_id = $1", &[&user_id])
                    .await?;
            }
        }
        Ok(())
    }
}

fn valid_dimensions(width: usize, height: usize) -> bool {
    let range = AVATAR_MIN_DIMENSION..=AVATAR_MAX_DIMENSION;
    range.contains(&width) && range.contains(&height)
}
