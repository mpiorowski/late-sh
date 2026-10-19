//! Local rectangular framing, preview, and input for board avatars.
use dartboard_core::{Canvas, Pos};
use late_core::models::avatar::{
    AVATAR_DEFAULT_HEIGHT, AVATAR_DEFAULT_WIDTH, AVATAR_MAX_DIMENSION, AVATAR_MIN_DIMENSION, Avatar,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::State;
use crate::app::{
    common::theme,
    input::{MouseButton, MouseEventKind, ParsedInput},
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AvatarFrame {
    pub(crate) origin: Pos,
    pub(crate) width: usize,
    pub(crate) height: usize,
    drag: Option<Pos>,
}

impl AvatarFrame {
    pub(crate) fn new(origin: Pos, board: &Canvas) -> Self {
        let mut frame = Self {
            origin,
            width: AVATAR_DEFAULT_WIDTH,
            height: AVATAR_DEFAULT_HEIGHT,
            drag: None,
        };
        frame.clamp(board);
        frame
    }

    fn clamp(&mut self, board: &Canvas) {
        self.width = self
            .width
            .clamp(AVATAR_MIN_DIMENSION, AVATAR_MAX_DIMENSION)
            .min(board.width);
        self.height = self
            .height
            .clamp(AVATAR_MIN_DIMENSION, AVATAR_MAX_DIMENSION)
            .min(board.height);
        self.origin.x = self.origin.x.min(board.width.saturating_sub(self.width));
        self.origin.y = self.origin.y.min(board.height.saturating_sub(self.height));
    }

    fn resize(&mut self, dx: isize, dy: isize, board: &Canvas) {
        self.width = self.width.saturating_add_signed(dx);
        self.height = self.height.saturating_add_signed(dy);
        self.clamp(board);
    }

    fn move_by(&mut self, dx: isize, dy: isize, board: &Canvas) {
        self.origin.x = self.origin.x.saturating_add_signed(dx);
        self.origin.y = self.origin.y.saturating_add_signed(dy);
        self.clamp(board);
    }

    pub(super) fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    fn drag_to(&mut self, anchor: Pos, pos: Pos, board: &Canvas) {
        let dx = anchor.x.abs_diff(pos.x).min(AVATAR_MAX_DIMENSION - 1);
        let dy = anchor.y.abs_diff(pos.y).min(AVATAR_MAX_DIMENSION - 1);
        self.origin = Pos {
            x: if pos.x < anchor.x {
                anchor.x - dx
            } else {
                anchor.x
            },
            y: if pos.y < anchor.y {
                anchor.y - dy
            } else {
                anchor.y
            },
        };
        self.width = dx + 1;
        self.height = dy + 1;
        self.clamp(board);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum AvatarFlow {
    #[default]
    Idle,
    Framing(AvatarFrame),
    Confirm {
        frame: AvatarFrame,
        avatar: Option<Avatar>,
        saving: bool,
        scroll: usize,
    },
}

impl AvatarFlow {
    pub(crate) fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }
}

pub(crate) fn handle_key(state: &mut State, byte: u8) -> bool {
    let flow = state.gallery().avatar.clone();
    match flow {
        AvatarFlow::Idle => return false,
        AvatarFlow::Framing(frame) => match byte {
            0x1b => {
                state.clear_local_state();
                state.gallery_mut().cancel_avatar();
            }
            b'\r' | b'\n' => {
                match Avatar::crop(
                    &state.snapshot.canvas,
                    frame.origin,
                    frame.width,
                    frame.height,
                ) {
                    Ok(avatar) => {
                        state.gallery_mut().avatar = AvatarFlow::Confirm {
                            frame,
                            avatar: Some(avatar),
                            saving: false,
                            scroll: 0,
                        }
                    }
                    Err(error) => state.gallery_mut().set_framing_notice(error.to_string()),
                }
            }
            b'x' | b'X' => {
                state.gallery_mut().avatar = AvatarFlow::Confirm {
                    frame,
                    avatar: None,
                    saving: false,
                    scroll: 0,
                }
            }
            b'+' | b'=' | b']' => resize(state, 1, 1),
            b'-' | b'_' | b'[' => resize(state, -1, -1),
            b'h' => {
                handle_arrow(state, b'D', false);
            }
            b'j' => {
                handle_arrow(state, b'B', false);
            }
            b'k' => {
                handle_arrow(state, b'A', false);
            }
            b'l' => {
                handle_arrow(state, b'C', false);
            }
            _ => {}
        },
        AvatarFlow::Confirm { saving: true, .. } => {}
        AvatarFlow::Confirm { frame, .. } => match byte {
            0x1b => {
                state.gallery_mut().avatar = AvatarFlow::Framing(frame);
            }
            b'\r' | b'\n' => state.gallery_mut().submit_avatar(),
            _ => {}
        },
    }
    true
}

fn resize(state: &mut State, dx: isize, dy: isize) {
    let board = &state.snapshot.canvas;
    let mut frame = match &state.gallery().avatar {
        AvatarFlow::Framing(frame) => frame.clone(),
        _ => return,
    };
    frame.resize(dx, dy, board);
    state.gallery_mut().avatar = AvatarFlow::Framing(frame);
    state.sync_avatar_selection();
}

pub(crate) fn handle_arrow(state: &mut State, key: u8, shift: bool) -> bool {
    if !state.gallery().avatar.is_active() {
        return false;
    }
    let visible = state.gallery().avatar_preview_rows.get();
    if let AvatarFlow::Confirm {
        avatar: Some(avatar),
        saving: false,
        scroll,
        ..
    } = &mut state.gallery_mut().avatar
    {
        let max = avatar.height().saturating_sub(visible);
        match key {
            b'A' => *scroll = scroll.saturating_sub(1),
            b'B' => *scroll = (*scroll + 1).min(max),
            _ => {}
        }
        return true;
    }
    let (dx, dy) = match key {
        b'A' => (0, -1),
        b'B' => (0, 1),
        b'C' => (1, 0),
        b'D' => (-1, 0),
        _ => return true,
    };
    if shift {
        resize(state, dx, dy);
    } else if let AvatarFlow::Framing(mut frame) = state.gallery().avatar.clone() {
        frame.move_by(dx, dy, &state.snapshot.canvas);
        state.gallery_mut().avatar = AvatarFlow::Framing(frame);
        state.sync_avatar_selection();
    }
    true
}

pub(crate) fn handle_event(state: &mut State, size: (u16, u16), event: &ParsedInput) -> bool {
    if !state.gallery().avatar.is_active() {
        return false;
    }
    state.set_viewport_for_screen(size);
    match event {
        ParsedInput::Char(_) | ParsedInput::Byte(_) | ParsedInput::Arrow(_) => return false,
        ParsedInput::ShiftArrow(key) => {
            handle_arrow(state, *key, true);
        }
        ParsedInput::Mouse(mouse) => {
            let AvatarFlow::Framing(mut frame) = state.gallery().avatar.clone() else {
                return true;
            };
            if mouse.button != Some(MouseButton::Left) {
                return true;
            }
            if mouse.kind == MouseEventKind::Up {
                frame.drag = None;
            } else if let Some(pos) = state.canvas_pos_for_screen_point(size, mouse.x, mouse.y) {
                match mouse.kind {
                    MouseEventKind::Down => {
                        frame.origin = pos;
                        frame.drag = Some(pos);
                    }
                    MouseEventKind::Drag => {
                        if let Some(anchor) = frame.drag {
                            frame.drag_to(anchor, pos, &state.snapshot.canvas);
                        }
                    }
                    _ => {}
                }
                frame.clamp(&state.snapshot.canvas);
            }
            state.gallery_mut().avatar = AvatarFlow::Framing(frame);
            state.sync_avatar_selection();
        }
        _ => {}
    }
    true
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &State) {
    match &state.gallery().avatar {
        AvatarFlow::Idle => {}
        AvatarFlow::Framing(crop) => {
            let bar = Rect::new(
                area.x,
                area.bottom().saturating_sub(2),
                area.width,
                2.min(area.height),
            );
            frame.render_widget(Clear, bar);
            let hint = format!(
                "Avatar {}x{} · arrows move · Shift+arrows resize · drag frame · Enter preview · x remove · Esc cancel",
                crop.width, crop.height
            );
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(hint),
                    Line::from(state.gallery().notice().unwrap_or_default()),
                ])
                .style(
                    Style::default()
                        .fg(theme::AMBER_GLOW())
                        .bg(theme::BG_HIGHLIGHT()),
                ),
                bar,
            );
        }
        AvatarFlow::Confirm {
            avatar,
            saving,
            scroll,
            ..
        } => {
            let art_width = avatar.as_ref().map_or(0, Avatar::width) as u16;
            let art_height = avatar.as_ref().map_or(0, Avatar::height) as u16;
            let width = 56.min(area.width);
            let height = (art_height + 7).min(area.height);
            let popup = Rect::new(
                area.x + area.width.saturating_sub(width) / 2,
                area.y + area.height.saturating_sub(height) / 2,
                width,
                height,
            );
            frame.render_widget(Clear, popup);
            let block = Block::default()
                .title(if avatar.is_some() {
                    " Use this avatar? "
                } else {
                    " Remove your avatar? "
                })
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::AMBER_GLOW()));
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            if let Some(avatar) = avatar {
                let art = Rect::new(
                    inner.x + inner.width.saturating_sub(art_width) / 2,
                    inner.y,
                    art_width.min(inner.width),
                    art_height.min(inner.height.saturating_sub(4)),
                );
                state.gallery().avatar_preview_rows.set(art.height as usize);
                super::ui::render_piece_canvas_at(
                    frame.buffer_mut(),
                    art,
                    avatar.canvas(),
                    Pos {
                        x: 0,
                        y: (*scroll).min(avatar.height().saturating_sub(art.height as usize)),
                    },
                );
            }
            let footer = Rect::new(
                inner.x,
                inner.bottom().saturating_sub(3),
                inner.width,
                3.min(inner.height),
            );
            let hint = if *saving {
                "Saving…"
            } else {
                "Enter confirms · Esc back · ↑↓ preview"
            };
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(hint),
                    Line::from("Shown on your terminal and web profiles."),
                    Line::from(state.gallery().notice().unwrap_or_default()),
                ])
                .style(Style::default().fg(theme::TEXT())),
                footer,
            );
        }
    }
}

pub(crate) fn begin(state: &mut State) {
    state.clear_local_state();
    let crop = AvatarFrame::new(state.cursor(), &state.snapshot.canvas);
    state.gallery_mut().avatar = AvatarFlow::Framing(crop);
    state.gallery_mut().focus_canvas();
    state.gallery_mut().set_rail_visible(false);
    state.gallery_mut().set_framing_notice(
        "Choose any rectangle up to 32x32 cells. +/- changes both dimensions.".to_string(),
    );
    state.sync_avatar_selection();
}

pub(crate) fn finish(state: &mut State) {
    state.clear_local_state();
    state.gallery_mut().focus_rail();
}

#[cfg(test)]
#[path = "avatar_test.rs"]
mod avatar_test;
