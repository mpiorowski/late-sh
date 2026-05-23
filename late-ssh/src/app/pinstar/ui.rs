use crate::app::pinstar::helpers::{
    PinstarTheme, fill_cursor_line_bg, get_textarea_scroll, line_number_gutter,
};
use crate::app::pinstar::state::PinstarState;
use ratatui::{prelude::*, widgets::*};

use super::browser::{BrowserMode, BrowserTab, DiagramBrowser};

pub fn draw_pinstar_view(
    frame: &mut Frame,
    area: Rect,
    state: &mut PinstarState,
    theme: &PinstarTheme,
) {
    let total_area = area;
    let mut area = total_area;
    area.height = area.height.saturating_sub(1);

    let (editor_area, canvas_area) = if state.show_editor_pane {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);
        (Some(main_chunks[0]), main_chunks[1])
    } else {
        (None, area)
    };

    if let Some(editor_area) = editor_area {
        let editor_border_color = if state.editor_focus {
            theme.accent
        } else {
            theme.muted
        };
        let editor_block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(editor_border_color))
            .title(" Source (JSON) ")
            .style(theme.preview_bg_style());

        let line_count = state.raw_editor.lines().len();
        let cursor_row = state.raw_editor.cursor().0;
        let scroll_row = get_textarea_scroll(&state.raw_editor).0;

        let content_area = editor_area;
        let digits = line_count.max(1).to_string().len() as u16;
        let gutter_width = digits + 1;
        let gutter_area = Rect::new(
            content_area.x,
            content_area.y,
            gutter_width.min(content_area.width),
            content_area.height,
        );
        let gutter = line_number_gutter(
            line_count,
            cursor_row,
            scroll_row,
            content_area.height,
            theme,
            1,
        );
        frame.render_widget(gutter, gutter_area);

        let editor_rect = Rect::new(
            content_area.x + gutter_area.width,
            content_area.y,
            content_area.width.saturating_sub(gutter_area.width),
            content_area.height,
        );

        state.raw_editor.set_block(editor_block);
        state.raw_editor.set_style(theme.preview_bg_style());
        state
            .raw_editor
            .set_cursor_line_style(if state.editor_focus {
                Style::default().bg(theme.preview_bg())
            } else {
                Style::default()
            });
        frame.render_widget(&state.raw_editor, editor_rect);

        if state.editor_focus {
            let cursor_bg = theme.preview_bg();
            fill_cursor_line_bg(frame, &state.raw_editor, editor_rect, cursor_bg);
        }
    }

    let canvas_border_color = if !state.editor_focus || !state.show_editor_pane {
        theme.accent
    } else {
        theme.muted
    };
    let canvas_block = Block::default()
        .borders(Borders::NONE)
        .border_style(Style::default().fg(canvas_border_color))
        .style(theme.bg_style());
    frame.render_widget(canvas_block, canvas_area);

    if state.show_grid {
        let mut grid_step_x = 100.0;
        let mut grid_step_y = 50.0;
        while grid_step_y * state.zoom < 6.0 {
            grid_step_x *= 2.0;
            grid_step_y *= 2.0;
        }

        let (cx1, cy1) = state.screen_to_canvas(canvas_area.left(), canvas_area.top(), canvas_area);
        let (cx2, cy2) =
            state.screen_to_canvas(canvas_area.right(), canvas_area.bottom(), canvas_area);

        let min_cx = cx1.min(cx2);
        let max_cx = cx1.max(cx2);
        let min_cy = cy1.min(cy2);
        let max_cy = cy1.max(cy2);

        let start_x = (min_cx / grid_step_x).floor() * grid_step_x;
        let end_x = (max_cx / grid_step_x).ceil() * grid_step_x;
        let start_y = (min_cy / grid_step_y).floor() * grid_step_y;
        let end_y = (max_cy / grid_step_y).ceil() * grid_step_y;

        let buf = frame.buffer_mut();
        let mut cur_x = start_x;
        while cur_x <= end_x {
            let mut cur_y = start_y;
            while cur_y <= end_y {
                let sx = (((cur_x - state.viewport_x) * state.zoom)
                    + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0))
                    .round() as i32;
                let sy = (((cur_y - state.viewport_y) * state.zoom)
                    + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0))
                    .round() as i32;

                if sx >= canvas_area.left() as i32
                    && sx < canvas_area.right() as i32
                    && sy >= canvas_area.top() as i32
                    && sy < canvas_area.bottom() as i32
                    && sx >= 0
                    && sx < buf.area.width as i32
                    && sy >= 0
                    && sy < buf.area.height as i32
                    && let Some(cell) = buf.cell_mut((sx as u16, sy as u16))
                {
                    cell.set_char('·').set_fg(theme.muted);
                }
                cur_y += grid_step_y;
            }
            cur_x += grid_step_x;
        }
    }

    let mut groups: Vec<&crate::app::pinstar::data::CanvasNode> = state
        .data
        .nodes
        .iter()
        .filter(|n| matches!(n, crate::app::pinstar::data::CanvasNode::Group(_)))
        .collect();

    // Sort descending by area so larger (parent) groups render first (drawn under smaller nested child groups)
    groups.sort_by(|a, b| {
        let (wa, ha) = a.size();
        let (wb, hb) = b.size();
        let area_a = wa * ha;
        let area_b = wb * hb;
        area_b
            .partial_cmp(&area_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for node in groups {
        if let crate::app::pinstar::data::CanvasNode::Group(g) = node {
            let (nx, ny) = node.pos();
            let (nw, nh) = node.size();

            let sx = ((nx - state.viewport_x) * state.zoom)
                + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0);
            let sy = ((ny - state.viewport_y) * state.zoom)
                + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0);
            let sw = nw * state.zoom;
            let sh = nh * state.zoom;

            if sx + sw < canvas_area.left() as f64
                || sx > canvas_area.right() as f64
                || sy + sh < canvas_area.top() as f64
                || sy > canvas_area.bottom() as f64
            {
                continue;
            }

            let left = sx.max(canvas_area.left() as f64);
            let top = sy.max(canvas_area.top() as f64);
            let right = (sx + sw).min(canvas_area.right() as f64);
            let bottom = (sy + sh).min(canvas_area.bottom() as f64);

            if right <= left || bottom <= top {
                continue;
            }

            let node_rect = Rect::new(
                left.round() as u16,
                top.round() as u16,
                (right - left).round() as u16,
                (bottom - top).round() as u16,
            );

            let is_selected = state.selected_node_id.as_ref() == Some(&g.id.to_string())
                || state.drag_captured_nodes.contains(&g.id);
            let is_editing = is_selected && state.floating_editor.is_some();
            let base_color = PinstarTheme::parse_color(g.color.as_deref(), theme);

            let is_connected_to_selected = if let Some(sel_id) = &state.selected_node_id {
                sel_id != &g.id
                    && state.data.edges.iter().any(|e| {
                        (e.from_node == *sel_id && e.to_node == g.id)
                            || (e.to_node == *sel_id && e.from_node == g.id)
                    })
            } else {
                false
            };

            let border_color = if is_editing {
                theme.accent
            } else if is_connected_to_selected {
                theme.success
            } else {
                base_color
            };

            let mut label = g.label.as_deref().unwrap_or("Group").to_string();
            if is_editing {
                label = format!("[EDITING] {}", label);
            }

            let mut block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(
                    Line::from(Span::styled(
                        label,
                        Style::default().fg(if is_editing { theme.accent } else { base_color }),
                    ))
                    .alignment(Alignment::Center),
                )
                .style(theme.bg_style());

            if is_selected && !is_editing {
                block = block.border_set(ratatui::symbols::border::Set {
                    top_left: "┌",
                    top_right: "┐",
                    bottom_left: "└",
                    bottom_right: "┘",
                    vertical_left: "┆",
                    vertical_right: "┆",
                    horizontal_top: "┄",
                    horizontal_bottom: "┄",
                });
            } else {
                block = block.border_type(if is_editing {
                    BorderType::Rounded
                } else {
                    BorderType::Double
                });
            }

            frame.render_widget(block, node_rect);

            // Draw titlebar background — clickable area indicator for groups
            if node_rect.height >= 3 {
                let tbar = Rect::new(
                    node_rect.x + 1,
                    node_rect.y + 1,
                    node_rect.width.saturating_sub(2),
                    1,
                );
                let tbar_color = if is_selected {
                    theme.accent
                } else {
                    theme.muted
                };
                frame.render_widget(
                    Paragraph::new(" ".repeat(tbar.width as usize))
                        .style(Style::default().bg(tbar_color)),
                    tbar,
                );
            }

            if is_selected {
                let corner_style = Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD);
                if node_rect.width > 0 && node_rect.height > 0 {
                    frame.render_widget(
                        Paragraph::new("⇘").style(corner_style),
                        Rect::new(node_rect.x, node_rect.y, 1, 1),
                    );
                    if node_rect.width > 1 {
                        frame.render_widget(
                            Paragraph::new("⇙").style(corner_style),
                            Rect::new(node_rect.x + node_rect.width - 1, node_rect.y, 1, 1),
                        );
                    }
                    if node_rect.height > 1 {
                        frame.render_widget(
                            Paragraph::new("⇗").style(corner_style),
                            Rect::new(node_rect.x, node_rect.y + node_rect.height - 1, 1, 1),
                        );
                    }
                    if node_rect.width > 1 && node_rect.height > 1 {
                        frame.render_widget(
                            Paragraph::new("⇖").style(corner_style),
                            Rect::new(
                                node_rect.x + node_rect.width - 1,
                                node_rect.y + node_rect.height - 1,
                                1,
                                1,
                            ),
                        );
                    }
                }
            }

            if state.resizing_node_id.as_ref() == Some(&g.id.to_string()) {
                let handle_text = "[↘]";
                let handle_style = Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD);
                let handle_rect = Rect::new(
                    (sx + sw - 3.0).max(0.0) as u16,
                    (sy + sh - 1.0).max(0.0) as u16,
                    3,
                    1,
                );
                frame.render_widget(Paragraph::new(handle_text).style(handle_style), handle_rect);
            }
        }
    }

    for edge in &state.data.edges {
        let from_node = state.data.nodes.iter().find(|n| n.id() == edge.from_node);
        let to_node = state.data.nodes.iter().find(|n| n.id() == edge.to_node);

        if let (Some(f), Some(t)) = (from_node, to_node) {
            let effective_style = edge.style;
            let (fx, fy) = f.pos();
            let (fw, fh) = f.size();
            let (tx, ty) = t.pos();
            let (tw, th) = t.size();

            let scx = fx + fw / 2.0;
            let scy = fy + fh / 2.0;
            let tcx = tx + tw / 2.0;
            let tcy = ty + th / 2.0;

            let dx = tcx - scx;
            let dy = tcy - scy;

            let is_horizontal_exit = dx.abs() > dy.abs();

            let (ax, ay) = if is_horizontal_exit {
                if dx > 0.0 { (fx + fw, scy) } else { (fx, scy) }
            } else {
                if dy > 0.0 { (scx, fy + fh) } else { (scx, fy) }
            };

            let (bx, by) = if is_horizontal_exit {
                if dx > 0.0 { (tx, tcy) } else { (tx + tw, tcy) }
            } else {
                if dy > 0.0 { (tcx, ty) } else { (tcx, ty + th) }
            };

            let mut sfx = ((ax - state.viewport_x) * state.zoom)
                + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0);
            let mut sfy = ((ay - state.viewport_y) * state.zoom)
                + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0);
            let mut stx = ((bx - state.viewport_x) * state.zoom)
                + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0);
            let mut sty = ((by - state.viewport_y) * state.zoom)
                + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0);

            // Adjust coordinates for RIGHT and BOTTOM edges to account for grid rendering offset
            if is_horizontal_exit {
                if dx > 0.0 {
                    sfx -= 1.0; // Source exiting from right
                } else {
                    stx -= 1.0; // Target entering from right
                }
            } else {
                if dy > 0.0 {
                    sfy -= 1.0; // Source exiting from bottom
                } else {
                    sty -= 1.0; // Target entering from bottom
                }
            }

            let edge_color = if state.selected_edge_id.as_ref() == Some(&edge.id) {
                theme.accent
            } else if edge.color.is_some() {
                crate::app::pinstar::helpers::PinstarTheme::parse_color(
                    edge.color.as_deref(),
                    theme,
                )
            } else {
                theme.muted
            };

            let buf = frame.buffer_mut();

            let draw_box_line = |buf: &mut ratatui::prelude::Buffer,
                                 x1: i32,
                                 y1: i32,
                                 x2: i32,
                                 y2: i32,
                                 horz_char: char,
                                 vert_char: char| {
                if y1 == y2 {
                    let (start, end) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
                    for x in start..=end {
                        if x < canvas_area.left() as i32
                            || x >= canvas_area.right() as i32
                            || y1 < canvas_area.top() as i32
                            || y1 >= canvas_area.bottom() as i32
                        {
                            continue;
                        }
                        let ch = match effective_style {
                            crate::app::pinstar::data::EdgeStyle::Dashed => {
                                if (x - start) % 8 >= 4 {
                                    continue;
                                }
                                horz_char
                            }
                            _ => horz_char,
                        };
                        if let Some(cell) = buf.cell_mut((x as u16, y1 as u16)) {
                            cell.set_char(ch).set_fg(edge_color);
                        }
                    }
                } else if x1 == x2 {
                    let (start, end) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
                    for y in start..=end {
                        if x1 < canvas_area.left() as i32
                            || x1 >= canvas_area.right() as i32
                            || y < canvas_area.top() as i32
                            || y >= canvas_area.bottom() as i32
                        {
                            continue;
                        }
                        let ch = match effective_style {
                            crate::app::pinstar::data::EdgeStyle::Dashed => {
                                if (y - start) % 8 >= 4 {
                                    continue;
                                }
                                vert_char
                            }
                            _ => vert_char,
                        };
                        if let Some(cell) = buf.cell_mut((x1 as u16, y as u16)) {
                            cell.set_char(ch).set_fg(edge_color);
                        }
                    }
                }
            };

            let draw_corner = |buf: &mut ratatui::prelude::Buffer, x: i32, y: i32, ch: char| {
                if x >= canvas_area.left() as i32
                    && x < canvas_area.right() as i32
                    && y >= canvas_area.top() as i32
                    && y < canvas_area.bottom() as i32
                {
                    if let Some(cell) = buf.cell_mut((x as u16, y as u16)) {
                        cell.set_char(ch).set_fg(edge_color);
                    }
                }
            };

            let draw_arrow = |buf: &mut ratatui::prelude::Buffer, ch: char, col: i32, row: i32| {
                if col >= canvas_area.left() as i32
                    && col < canvas_area.right() as i32
                    && row >= canvas_area.top() as i32
                    && row < canvas_area.bottom() as i32
                {
                    if let Some(cell) = buf.cell_mut((col as u16, row as u16)) {
                        cell.set_char(ch).set_fg(edge_color);
                    }
                }
            };

            let use_orthogonal = state.orthogonal_connections;

            if use_orthogonal {
                let sx = sfx.round() as i32;
                let sy = sfy.round() as i32;
                let ex = stx.round() as i32;
                let ey = sty.round() as i32;

                if is_horizontal_exit {
                    let mid_x = (sx + ex) / 2;
                    draw_box_line(buf, sx, sy, mid_x, sy, '\u{2500}', '\u{2502}');
                    draw_box_line(buf, mid_x, sy, mid_x, ey, '\u{2500}', '\u{2502}');
                    draw_box_line(buf, mid_x, ey, ex, ey, '\u{2500}', '\u{2502}');

                    if ex > sx {
                        if ey > sy {
                            draw_corner(buf, mid_x, sy, '\u{2510}'); // ┐
                            draw_corner(buf, mid_x, ey, '\u{2514}'); // └
                        } else if sy > ey {
                            draw_corner(buf, mid_x, sy, '\u{2518}'); // ┘
                            draw_corner(buf, mid_x, ey, '\u{250C}'); // ┌
                        }
                    } else {
                        if ey > sy {
                            draw_corner(buf, mid_x, sy, '\u{250C}'); // ┌
                            draw_corner(buf, mid_x, ey, '\u{2518}'); // ┘
                        } else if sy > ey {
                            draw_corner(buf, mid_x, sy, '\u{2514}'); // └
                            draw_corner(buf, mid_x, ey, '\u{2510}'); // ┐
                        }
                    }

                    let (arrow_c, arrow_col, arrow_row) = if ex > sx {
                        ('\u{25b6}', ex - 1, ey)
                    } else {
                        ('\u{25c0}', ex + 1, ey)
                    };
                    draw_arrow(buf, arrow_c, arrow_col, arrow_row);
                } else {
                    let mid_y = (sy + ey) / 2;
                    draw_box_line(buf, sx, sy, sx, mid_y, '\u{2500}', '\u{2502}');
                    draw_box_line(buf, sx, mid_y, ex, mid_y, '\u{2500}', '\u{2502}');
                    draw_box_line(buf, ex, mid_y, ex, ey, '\u{2500}', '\u{2502}');

                    if ey > sy {
                        if ex > sx {
                            draw_corner(buf, sx, mid_y, '\u{2514}'); // └
                            draw_corner(buf, ex, mid_y, '\u{2510}'); // ┐
                        } else if sx > ex {
                            draw_corner(buf, sx, mid_y, '\u{2518}'); // ┘
                            draw_corner(buf, ex, mid_y, '\u{250C}'); // ┌
                        }
                    } else {
                        if ex > sx {
                            draw_corner(buf, sx, mid_y, '\u{250C}'); // ┌
                            draw_corner(buf, ex, mid_y, '\u{2518}'); // ┘
                        } else if sx > ex {
                            draw_corner(buf, sx, mid_y, '\u{2510}'); // ┐
                            draw_corner(buf, ex, mid_y, '\u{2514}'); // └
                        }
                    }

                    let (arrow_c, arrow_col, arrow_row) = if ey > sy {
                        ('\u{25bc}', ex, ey - 1)
                    } else {
                        ('\u{25b2}', ex, ey + 1)
                    };
                    draw_arrow(buf, arrow_c, arrow_col, arrow_row);
                }
            } else {
                // Non-orthogonal: braille pixel line
                let steps = ((sfx - stx).powi(2) + (sfy - sty).powi(2)).sqrt() * 4.0;
                let steps = steps.max(1.0) as usize;
                let sdx = (stx - sfx) / steps as f64;
                let sdy = (sty - sfy) / steps as f64;
                let mut cx = sfx;
                let mut cy = sfy;
                for step in 0..=steps {
                    let should_draw = match effective_style {
                        crate::app::pinstar::data::EdgeStyle::Dashed => step % 16 < 8,
                        _ => true,
                    };
                    if should_draw {
                        if cx >= canvas_area.left() as f64
                            && cx < canvas_area.right() as f64
                            && cy >= canvas_area.top() as f64
                            && cy < canvas_area.bottom() as f64
                        {
                            let cell_x = cx as u16;
                            let cell_y = cy as u16;
                            let dot_x = ((cx - cell_x as f64) * 2.0) as u16;
                            let dot_y = ((cy - cell_y as f64) * 4.0) as u16;
                            if let Some(cell) = buf.cell_mut((cell_x, cell_y)) {
                                let mut braille_char =
                                    cell.symbol().chars().next().unwrap_or('\u{2800}');
                                if !('\u{2800}'..='\u{28FF}').contains(&braille_char) {
                                    braille_char = '\u{2800}';
                                }
                                let dot_bit = match (dot_x, dot_y) {
                                    (0, 0) => 0x01,
                                    (0, 1) => 0x02,
                                    (0, 2) => 0x04,
                                    (1, 0) => 0x08,
                                    (1, 1) => 0x10,
                                    (1, 2) => 0x20,
                                    (0, 3) => 0x40,
                                    (1, 3) => 0x80,
                                    _ => 0,
                                };
                                let new_code = (braille_char as u32 - 0x2800) | dot_bit;
                                if let Some(c) = char::from_u32(0x2800 + new_code) {
                                    cell.set_char(c).set_fg(edge_color);
                                }
                            }
                        }
                    }
                    cx += sdx;
                    cy += sdy;
                }
            }
        }
    }
    for node in &state.data.nodes {
        if matches!(node, crate::app::pinstar::data::CanvasNode::Group(_)) {
            continue;
        }

        let (nx, ny) = node.pos();
        let (nw, nh) = node.size();

        let sx = ((nx - state.viewport_x) * state.zoom)
            + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0);
        let sy = ((ny - state.viewport_y) * state.zoom)
            + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0);
        let sw = nw * state.zoom;
        let sh = nh * state.zoom;

        if sx + sw < canvas_area.left() as f64
            || sx > canvas_area.right() as f64
            || sy + sh < canvas_area.top() as f64
            || sy > canvas_area.bottom() as f64
        {
            continue;
        }

        let left = sx.max(canvas_area.left() as f64);
        let top = sy.max(canvas_area.top() as f64);
        let right = (sx + sw).min(canvas_area.right() as f64);
        let bottom = (sy + sh).min(canvas_area.bottom() as f64);

        if right <= left || bottom <= top {
            continue;
        }

        let node_rect = Rect::new(
            left.round() as u16,
            top.round() as u16,
            (right - left).round() as u16,
            (bottom - top).round() as u16,
        );

        frame.render_widget(Clear, node_rect);

        let is_selected = state.selected_node_id.as_ref() == Some(&node.id().to_string())
            || state.drag_captured_nodes.contains(node.id());
        let is_editing = is_selected && state.floating_editor.is_some();

        let node_color_attr = match node {
            crate::app::pinstar::data::CanvasNode::Text(n) => n.color.as_deref(),
            crate::app::pinstar::data::CanvasNode::File(n) => n.color.as_deref(),
            crate::app::pinstar::data::CanvasNode::Link(n) => n.color.as_deref(),
            _ => None,
        };

        let base_color = PinstarTheme::parse_color(node_color_attr, theme);

        let is_connected_to_selected = if let Some(sel_id) = &state.selected_node_id {
            sel_id != node.id()
                && state.data.edges.iter().any(|e| {
                    (e.from_node == *sel_id && e.to_node == node.id())
                        || (e.to_node == *sel_id && e.from_node == node.id())
                })
        } else {
            false
        };

        let border_color = if is_editing {
            theme.accent
        } else if is_connected_to_selected {
            theme.success
        } else {
            base_color
        };

        let mut border_type = BorderType::Plain;
        if is_editing {
            border_type = BorderType::Double;
        }

        let mut node_title = match node {
            crate::app::pinstar::data::CanvasNode::File(n) => std::path::Path::new(&n.file)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&n.file)
                .to_string(),
            crate::app::pinstar::data::CanvasNode::Link(n) => n.url.clone(),
            _ => {
                if is_generated_id(node.id()) {
                    "".to_string()
                } else {
                    node.id().to_string()
                }
            }
        };

        if is_editing {
            node_title = format!("[EDITING] {}", node_title);
        }

        let use_braille_border = false;

        let mut block = Block::default().style(theme.bg_style());

        if !use_braille_border {
            block = block
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color));

            if is_selected && !is_editing {
                block = block.border_set(ratatui::symbols::border::Set {
                    top_left: "┌",
                    top_right: "┐",
                    bottom_left: "└",
                    bottom_right: "┘",
                    vertical_left: "┆",
                    vertical_right: "┆",
                    horizontal_top: "┄",
                    horizontal_bottom: "┄",
                });
            } else {
                block = block.border_type(border_type);
            }
        }

        let get_text_with_divider =
            |original: &str, inner_w: usize, color: ratatui::style::Color| -> ratatui::text::Text {
                if original
                    .split('\n')
                    .any(|l| l.trim_end_matches('\r').trim() == "---")
                {
                    let divider = if inner_w > 0 {
                        "─".repeat(inner_w)
                    } else {
                        "---".to_string()
                    };
                    let mut lines = Vec::new();
                    for line in original.split('\n') {
                        let clean = line.trim_end_matches('\r');
                        if clean.trim() == "---" {
                            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                                divider.clone(),
                                Style::default().fg(color),
                            )));
                        } else {
                            lines.push(ratatui::text::Line::from(clean.to_string()));
                        }
                    }
                    ratatui::text::Text::from(lines)
                } else {
                    ratatui::text::Text::from(original.to_string())
                }
            };

        if !use_braille_border {
            let inner_w = node_rect.width.saturating_sub(2) as usize;
            let text_content = get_text_with_divider(node.text(), inner_w, border_color);

            let text = Paragraph::new(text_content)
                .block(block)
                .style(Style::default().fg(theme.text))
                .wrap(Wrap { trim: false });
            frame.render_widget(text, node_rect);
        } else {
            // Clear background and draw node title/id
            frame.render_widget(block, node_rect);

            // Inset content area and center-align text for flowchart geometry
            let text_rect = node_rect.inner(ratatui::layout::Margin {
                horizontal: 2.min(node_rect.width.saturating_sub(1) / 2),
                vertical: 1.min(node_rect.height.saturating_sub(1) / 2),
            });

            // Mathematically compute vertical centroid offset based on estimated wrap lines
            let text_str = node.text();
            let mut est_lines = 0;
            for line in text_str.lines() {
                let char_count = line.chars().count();
                let needed = if text_rect.width > 0 {
                    ((char_count as f32) / (text_rect.width as f32)).ceil() as usize
                } else {
                    1
                };
                est_lines += needed.max(1);
            }
            let est_lines = est_lines.max(1);
            let available_h = text_rect.height as usize;
            let y_offset = if available_h > est_lines {
                (available_h - est_lines) / 2
            } else {
                0
            };

            let centered_rect = Rect::new(
                text_rect.x,
                text_rect.y + y_offset as u16,
                text_rect.width,
                text_rect.height.saturating_sub(y_offset as u16),
            );

            let text_content =
                get_text_with_divider(node.text(), text_rect.width as usize, border_color);

            let text = Paragraph::new(text_content)
                .alignment(ratatui::layout::Alignment::Center)
                .style(Style::default().fg(theme.text))
                .wrap(Wrap { trim: false });
            frame.render_widget(text, centered_rect);
        }

        if !node_title.is_empty() && node_rect.y > canvas_area.top() {
            let title_rect = Rect::new(node_rect.x, node_rect.y - 1, node_rect.width, 1);
            frame.render_widget(Clear, title_rect);
            let title_p = Paragraph::new(node_title.clone())
                .alignment(Alignment::Center)
                .style(Style::default().fg(if is_editing { theme.accent } else { base_color }));
            frame.render_widget(title_p, title_rect);
        }

        if is_selected {
            let corner_style = Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD);
            if node_rect.width > 0 && node_rect.height > 0 {
                frame.render_widget(
                    Paragraph::new("⇘").style(corner_style),
                    Rect::new(node_rect.x, node_rect.y, 1, 1),
                );
                if node_rect.width > 1 {
                    frame.render_widget(
                        Paragraph::new("⇙").style(corner_style),
                        Rect::new(node_rect.x + node_rect.width - 1, node_rect.y, 1, 1),
                    );
                }
                if node_rect.height > 1 {
                    frame.render_widget(
                        Paragraph::new("⇗").style(corner_style),
                        Rect::new(node_rect.x, node_rect.y + node_rect.height - 1, 1, 1),
                    );
                }
                if node_rect.width > 1 && node_rect.height > 1 {
                    frame.render_widget(
                        Paragraph::new("⇖").style(corner_style),
                        Rect::new(
                            node_rect.x + node_rect.width - 1,
                            node_rect.y + node_rect.height - 1,
                            1,
                            1,
                        ),
                    );
                }
            }
        }

        if state.resizing_node_id.as_ref() == Some(&node.id().to_string()) {
            let handle_text = "[↘]";
            let handle_style = Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD);
            let handle_rect = Rect::new(
                (sx + sw - 3.0).max(0.0) as u16,
                (sy + sh - 1.0).max(0.0) as u16,
                3,
                1,
            );
            frame.render_widget(Paragraph::new(handle_text).style(handle_style), handle_rect);
        }
    }

    if let Some(editor) = &mut state.floating_editor
        && let Some(node_id) = &state.selected_node_id
        && let Some(node) = state.data.nodes.iter().find(|n| n.id() == node_id)
    {
        let (nx, ny) = node.pos();
        let (nw, nh) = node.size();

        let sx = ((nx - state.viewport_x) * state.zoom)
            + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0);
        let sy = ((ny - state.viewport_y) * state.zoom)
            + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0);
        let sw = nw * state.zoom;
        let sh = nh * state.zoom;

        let left = sx.max(canvas_area.left() as f64);
        let top = sy.max(canvas_area.top() as f64);
        let right = (sx + sw).min(canvas_area.right() as f64);
        let bottom = (sy + sh).min(canvas_area.bottom() as f64);

        if right > left && bottom > top {
            let mut editor_rect = Rect::new(
                left.round() as u16,
                top.round() as u16,
                (right - left).round() as u16,
                (bottom - top).round() as u16,
            );
            let expansion_x = 2;
            let expansion_y = 1;
            editor_rect.x = editor_rect.x.saturating_sub(expansion_x);
            editor_rect.y = editor_rect.y.saturating_sub(expansion_y);
            editor_rect.width += expansion_x * 2;
            editor_rect.height += expansion_y * 2;
            editor_rect = editor_rect.intersection(canvas_area);

            editor.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent))
                    .style(theme.bg_style()),
            );
            editor.set_style(theme.bg_style());

            frame.render_widget(Clear, editor_rect);
            frame.render_widget(&*editor, editor_rect);
        }
    }

    // Draw selection rectangle
    if let (Some(start), Some(end)) = (state.select_rect_start, state.select_rect_end) {
        let sx = ((start.0 - state.viewport_x) * state.zoom)
            + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0);
        let sy = ((start.1 - state.viewport_y) * state.zoom)
            + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0);
        let ex = ((end.0 - state.viewport_x) * state.zoom)
            + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0);
        let ey = ((end.1 - state.viewport_y) * state.zoom)
            + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0);

        let (x1, x2) = if sx < ex { (sx, ex) } else { (ex, sx) };
        let (y1, y2) = if sy < ey { (sy, ey) } else { (ey, sy) };

        let buf = frame.buffer_mut();
        let mut dot = |x: f64, y: f64| {
            if x >= canvas_area.left() as f64
                && x < canvas_area.right() as f64
                && y >= canvas_area.top() as f64
                && y < canvas_area.bottom() as f64
            {
                if let Some(cell) = buf.cell_mut((x as u16, y as u16)) {
                    cell.set_char('·').set_fg(theme.accent);
                }
            }
        };

        let left = x1 as u16;
        let right = x2 as u16;
        let top = y1 as u16;
        let bot = y2 as u16;

        // Top and bottom edges
        for x in left..=right {
            if (x - left) % 3 == 0 {
                dot(x as f64, y1);
                dot(x as f64, y2);
            }
        }
        // Left and right edges
        for y in top..=bot {
            if (y - top) % 3 == 0 {
                dot(x1, y as f64);
                dot(x2, y as f64);
            }
        }
    }

    let mut hint_text =
        "Ctrl+L lock mode · Ctrl+F fit · Ctrl+E raw pane · Shift+I invite · ? help · Esc/q back"
            .to_string();
    if state.connection_source_id.is_some() {
        hint_text = "CONNECTION MODE: Select target node with mouse or Enter".to_string();
    } else if state.deleting_connection_source_id.is_some() {
        hint_text = "DELETE CONNECTION MODE: Select target node to remove link".to_string();
    } else if state.resizing_node_id.is_some() {
        hint_text = "RESIZE MODE: Drag mouse to resize, Right-click to confirm".to_string();
    }

    let mut spans = Vec::new();

    let (lock_label, lock_active) = match state.lock_mode() {
        crate::app::pinstar::data::DiagramLockMode::Unlocked => ("lock:off", false),
        crate::app::pinstar::data::DiagramLockMode::All => ("lock:all", true),
        crate::app::pinstar::data::DiagramLockMode::EditorOnly => ("lock:editors", true),
    };
    let lock_style = if lock_active {
        Style::default()
            .fg(theme.success)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };
    spans.push(Span::styled(format!(" {} ", lock_label), lock_style));
    spans.push(Span::raw("  "));

    let arrow_label = if state.orthogonal_connections {
        "arrow:on"
    } else {
        "arrow:off"
    };
    let arrow_style = if state.orthogonal_connections {
        Style::default()
            .fg(theme.success)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };
    spans.push(Span::styled(format!(" {} ", arrow_label), arrow_style));

    if let crate::app::pinstar::state::PinstarMode::Shared { role, .. } = &state.mode {
        let peer_count = state.peers().len();
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(" role:{} ", role),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(" peers:{} ", peer_count),
            Style::default().fg(theme.accent),
        ));
        spans.push(Span::raw("  "));
    }

    spans.push(Span::styled(hint_text, Style::default().fg(theme.muted)));

    let hint = Paragraph::new(Line::from(spans)).style(theme.hint_line_bg_style());

    let hint_area = Rect::new(
        total_area.x,
        total_area.bottom().saturating_sub(1),
        total_area.width,
        1,
    );
    frame.render_widget(hint, hint_area);

    if let Some(menu) = &state.context_menu {
        let menu_width = 32;
        let menu_height = menu.items.len() as u16;
        let menu_rect = Rect::new(
            menu.x.min(area.width.saturating_sub(menu_width)),
            menu.y.min(area.height.saturating_sub(menu_height)),
            menu_width,
            menu_height,
        );

        frame.render_widget(Clear, menu_rect);

        let items: Vec<ListItem> = menu
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let is_selected = i == menu.selected;
                let base_style = if is_selected {
                    Style::default()
                        .fg(theme.highlight_fg)
                        .bg(theme.highlight_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                };

                let label_text = format!("  {}", item);
                let shortcut =
                    crate::app::pinstar::helpers::get_menu_shortcut_char(menu.menu_type, item);

                let is_color_picker = menu.menu_type
                    == crate::app::pinstar::state::PinstarMenuType::ColorPicker
                    || menu.menu_type
                        == crate::app::pinstar::state::PinstarMenuType::EdgeColorPicker;

                if is_color_picker && item != "Default" {
                    let indicator_color = match item.as_str() {
                        "Red" => Some(Color::Rgb(255, 82, 82)),
                        "Orange" => Some(Color::Rgb(255, 152, 0)),
                        "Yellow" => Some(Color::Rgb(255, 235, 59)),
                        "Green" => Some(Color::Rgb(76, 175, 80)),
                        "Cyan" => Some(Color::Rgb(0, 188, 212)),
                        "Blue" => Some(Color::Rgb(33, 150, 243)),
                        "Purple" => Some(Color::Rgb(156, 39, 176)),
                        "Magenta" => Some(Color::Rgb(233, 30, 99)),
                        "White" => Some(Color::Rgb(255, 255, 255)),
                        _ => None,
                    };

                    if let Some(color) = indicator_color {
                        let display_text = " ■ ";
                        let spacer_len = 32usize
                            .saturating_sub(
                                label_text.chars().count() + display_text.chars().count(),
                            )
                            .max(1);
                        let spacer = " ".repeat(spacer_len);

                        ListItem::new(Line::from(vec![
                            Span::styled(label_text, Style::default()),
                            Span::styled(spacer, Style::default()),
                            Span::styled(
                                display_text,
                                Style::default().fg(color).add_modifier(Modifier::BOLD),
                            ),
                        ]))
                        .style(base_style)
                    } else {
                        ListItem::new(Line::from(vec![Span::styled(label_text, Style::default())]))
                            .style(base_style)
                    }
                } else if let Some(c) = shortcut {
                    let hint_str = format!(" [{}]", c);
                    let spacer_len = 32usize
                        .saturating_sub(label_text.chars().count() + hint_str.chars().count())
                        .max(1);
                    let spacer = " ".repeat(spacer_len);

                    let hint_style = if is_selected {
                        Style::default()
                            .fg(theme.highlight_fg)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.muted)
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(label_text, Style::default()),
                        Span::styled(spacer, Style::default()),
                        Span::styled(hint_str, hint_style),
                    ]))
                    .style(base_style)
                } else {
                    ListItem::new(Line::from(vec![Span::styled(label_text, Style::default())]))
                        .style(base_style)
                }
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::NONE)
                .style(theme.preview_bg_style()),
        );
        frame.render_widget(list, menu_rect);
    }

    if let Some(textarea) = &mut state.rename_popup {
        let popup_area = centered_rect(60, 20, area);
        frame.render_widget(Clear, popup_area);

        textarea.set_style(theme.bg_style());
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .style(theme.bg_style())
                .title(Span::styled(
                    " Rename Node (ID) - Enter to confirm, Esc to cancel ",
                    Style::default().fg(theme.accent),
                )),
        );

        frame.render_widget(&*textarea, popup_area);
    }

    if state.show_help {
        let popup_area = centered_rect(80, 85, area);
        frame.render_widget(Clear, popup_area);

        let shortcut_style = Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(theme.fg);

        let commands = vec![
            ("Alt+Enter", "Focus raw editor pane / return to canvas"),
            ("Arrows / hjkl", "Move selection to nearby nodes"),
            ("i / Enter", "Open inline node editor"),
            ("a", "Open context menu"),
            ("Right-drag", "Box select nodes / edges"),
            ("Shift+I", "Create invite link token (shared owner)"),
            (
                "Ctrl+L",
                "Cycle lock mode: off → all → editors (owner only)",
            ),
            ("Ctrl+O", "Toggle orthogonal connections"),
            ("Ctrl+S", "Save diagram"),
            ("Ctrl+F", "Fit all nodes into view"),
            ("Ctrl+R", "Reload from disk"),
            ("Ctrl+G", "Toggle grid"),
            ("Ctrl+E", "Toggle raw editor pane"),
            ("Ctrl+j / +", "Zoom in"),
            ("Ctrl+k / -", "Zoom out"),
            ("Esc / q", "Exit context / back to file list"),
            ("?", "Toggle this help"),
        ];

        let mut lines = Vec::new();
        lines.push(Line::from(""));

        for (key, desc) in commands {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:18}", key), shortcut_style),
                Span::raw(" : "),
                Span::styled(desc, desc_style),
            ]));
        }

        let help_widget = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent))
                    .style(theme.bg_style())
                    .title(Span::styled(
                        " Pinstar Keyboard Shortcuts - Press ANY KEY to close ",
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    )),
            )
            .alignment(Alignment::Left)
            .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(help_widget, popup_area);
    }

    if state.show_invite_dialog {
        draw_invite_dialog(frame, area, state);
    }
}

pub fn draw_invite_dialog(frame: &mut Frame, area: Rect, state: &PinstarState) {
    use crate::app::common::theme;

    let popup_area = centered_rect(60, 20, area);
    frame.render_widget(Clear, popup_area);

    let mut lines = vec![
        Line::from(Span::styled(
            "Diagram Invite",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if let Some(token) = &state.invite_token {
        lines.push(Line::from("Your invite link token:"));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            token,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from("(Valid for 24 hours)"));
    } else if let Some(err) = &state.invite_error {
        lines.push(Line::from(Span::styled(
            format!("Error: {}", err),
            Style::default().fg(Color::Red),
        )));
    } else {
        let mut msg = "Generating invite...".to_string();
        if state.invite_result_rx.is_some() {
            msg.push_str(" (Waiting for database)");
        }
        lines.push(Line::from(msg));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " copy to clipboard  ",
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::AMBER()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content = Paragraph::new(lines).centered();
    frame.render_widget(content, inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn is_generated_id(id: &str) -> bool {
    if id.starts_with("node_") && id.len() <= 16 {
        return true;
    }
    if id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    if id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return true;
    }
    false
}

// ── Diagram Browser UI ────────────────────────────────────────────────────

// ── Diagram Rail (left pane in editor view) ──────────────────────────────

// ── Diagram Browser UI ────────────────────────────────────────────────────

/// Draw browser popup overlays (create, rename, delete) over the canvas area
pub fn draw_browser_popups(frame: &mut Frame, area: Rect, browser: &DiagramBrowser) {
    match browser.mode {
        BrowserMode::ConfirmDelete => draw_confirm_delete(frame, area, browser),
        BrowserMode::RenameInput => draw_rename_diagram(frame, area, browser),
        BrowserMode::CreateDiagram => draw_create_diagram(frame, area, browser),
        BrowserMode::GenerateInvite => draw_generate_invite(frame, area, browser),
        _ => {}
    }
}

pub fn draw_diagram_browser(frame: &mut Frame, area: Rect, browser: &DiagramBrowser) {
    use crate::app::common::theme;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_DIM()))
        .title(Line::from(vec![
            Span::styled(
                " Pinstar ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("│ ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled(
                match browser.tab {
                    BrowserTab::MyDiagrams => "My Diagrams",
                    BrowserTab::SharedWithMe => "Shared with me",
                },
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match browser.mode {
        BrowserMode::List => draw_diagram_list(frame, inner, browser),
        BrowserMode::AcceptInvite => draw_accept_invite(frame, inner, browser),
        BrowserMode::ConfirmDelete => {
            draw_diagram_list(frame, inner, browser);
            draw_confirm_delete(frame, inner, browser);
        }
        BrowserMode::RenameInput => {
            draw_diagram_list(frame, inner, browser);
            draw_rename_diagram(frame, inner, browser);
        }
        BrowserMode::CreateDiagram => {
            draw_diagram_list(frame, inner, browser);
            draw_create_diagram(frame, inner, browser);
        }
        BrowserMode::GenerateInvite => {
            draw_diagram_list(frame, inner, browser);
            draw_generate_invite(frame, inner, browser);
        }
    }
}

fn draw_diagram_list(frame: &mut Frame, area: Rect, browser: &DiagramBrowser) {
    use crate::app::common::theme;

    if browser.loading {
        let loading = Paragraph::new("Loading diagrams...")
            .style(Style::default().fg(theme::TEXT_DIM()))
            .centered();
        frame.render_widget(loading, area);
        return;
    }

    if browser.entries.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No diagrams yet",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "n",
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" new diagram  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    "a",
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" accept invite  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    "Tab",
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" switch tab", Style::default().fg(theme::TEXT_DIM())),
            ]),
        ])
        .centered();
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = browser
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = i == browser.selected;
            let style = if is_selected {
                Style::default()
                    .fg(theme::BG_SELECTION())
                    .bg(theme::AMBER())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let role_icon = if entry.is_owner { "★" } else { "◆" };
            let role_str = if entry.is_owner {
                String::new()
            } else {
                format!("({})", entry.role)
            };
            let label = format!(
                " {} {} {}  {}",
                role_icon,
                entry.title,
                role_str,
                entry.updated.format("%m-%d %H:%M"),
            );
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, area);

    if let Some(err) = &browser.error {
        let err_area = Rect::new(area.x, area.y, area.width, 1);
        let err_line = Paragraph::new(Line::from(Span::styled(
            format!("Error: {}", err),
            Style::default().fg(Color::Red),
        )));
        frame.render_widget(err_line, err_area);
    }

    // Bottom hint
    let hint_y = area.bottom().saturating_sub(1);
    if hint_y > area.top() {
        let hint_area = Rect::new(area.x, hint_y, area.width, 1);
        let hint = Paragraph::new(Line::from(vec![
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" open  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "n",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" new  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "d",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" delete  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "r",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" rename  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "a",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" join  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "i",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" invite link  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "Tab",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" switch", Style::default().fg(theme::TEXT_DIM())),
        ]));
        frame.render_widget(hint, hint_area);
    }
}

fn draw_accept_invite(frame: &mut Frame, area: Rect, browser: &DiagramBrowser) {
    use crate::app::common::theme;

    let popup_area = centered_rect(60, 20, area);
    frame.render_widget(Clear, popup_area);

    let mut lines = vec![
        Line::from(Span::styled(
            "Join Diagram",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Enter invite token:"),
        Line::from(""),
    ];

    if browser.invite_token_input.is_empty() {
        lines.push(Line::from(Span::styled(
            "pi_...",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            &browser.invite_token_input,
            Style::default().fg(theme::TEXT()),
        )));
    }

    if let Some(err) = &browser.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Error: {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" join  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::AMBER()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content = Paragraph::new(lines);
    frame.render_widget(content, inner);
}

fn draw_rename_diagram(frame: &mut Frame, area: Rect, browser: &DiagramBrowser) {
    use crate::app::common::theme;

    let popup_area = centered_rect(50, 20, area);
    frame.render_widget(Clear, popup_area);

    let input_display = if browser.rename_input.is_empty() {
        Span::styled("Untitled", Style::default().fg(theme::TEXT_DIM()))
    } else {
        Span::styled(&browser.rename_input, Style::default().fg(theme::TEXT()))
    };

    let lines = vec![
        Line::from(Span::styled(
            "Rename Diagram",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("New name:"),
        Line::from(input_display),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::AMBER()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content = Paragraph::new(lines);
    frame.render_widget(content, inner);
}

fn draw_create_diagram(frame: &mut Frame, area: Rect, browser: &DiagramBrowser) {
    use crate::app::common::theme;

    let popup_area = centered_rect(50, 25, area);
    frame.render_widget(Clear, popup_area);

    let name_focused = matches!(
        browser.new_diagram_field,
        crate::app::pinstar::browser::NewDiagramField::Name
    );
    let format_focused = matches!(
        browser.new_diagram_field,
        crate::app::pinstar::browser::NewDiagramField::Format
    );

    let name_indicator = if name_focused { "▸ " } else { "  " };
    let format_indicator = if format_focused { "▸ " } else { "  " };

    // Build format selector
    let formats = crate::app::pinstar::browser::DiagramFormat::all();
    let mut format_spans = Vec::new();
    for (i, fmt) in formats.iter().enumerate() {
        if i > 0 {
            format_spans.push(Span::raw(" "))
        }
        if i == browser.new_diagram_format {
            format_spans.push(Span::styled(
                format!("< {} >", fmt.label()),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            format_spans.push(Span::styled(
                fmt.label().to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
    }

    let name_style = if name_focused {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };

    let lines = vec![
        Line::from(Span::styled(
            "New Diagram",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{}Name: ", name_indicator), name_style),
            Span::styled(
                if browser.new_diagram_name.is_empty() {
                    "Untitled Diagram"
                } else {
                    &browser.new_diagram_name
                },
                name_style,
            ),
            if name_focused {
                Span::styled("_", Style::default().fg(theme::AMBER()))
            } else {
                Span::raw("")
            },
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("{}Format: ", format_indicator),
            if format_focused {
                Style::default().fg(theme::TEXT())
            } else {
                Style::default().fg(theme::TEXT_DIM())
            },
        )]),
        Line::from(format_spans),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" create  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "Tab",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" switch field  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "←→",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" format  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::AMBER()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content = Paragraph::new(lines);
    frame.render_widget(content, inner);
}

fn draw_generate_invite(frame: &mut Frame, area: Rect, browser: &DiagramBrowser) {
    use crate::app::common::theme;

    let popup_area = centered_rect(70, 28, area);
    frame.render_widget(Clear, popup_area);

    let mut lines = vec![
        Line::from(Span::styled(
            "Create Invite Link",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if let Some(token) = &browser.generated_invite_token {
        lines.push(Line::from("Share this invite token:"));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            token,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" copy  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
        ]));
    } else if let Some(err) = &browser.error {
        lines.push(Line::from(Span::styled(
            format!("Error: {}", err),
            Style::default().fg(Color::Red),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "Generating invite token...",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::AMBER()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content = Paragraph::new(lines);
    frame.render_widget(content, inner);
}

fn draw_confirm_delete(frame: &mut Frame, area: Rect, browser: &DiagramBrowser) {
    use crate::app::common::theme;

    let target_title = browser
        .delete_target_id
        .and_then(|id| browser.entries.iter().find(|e| e.id == id))
        .map(|e| e.title.as_str())
        .unwrap_or("this diagram");

    let popup_area = centered_rect(50, 20, area);
    frame.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(Span::styled(
            "Delete Diagram",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("Delete '{}'?", target_title)),
        Line::from("This cannot be undone."),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                "n/Esc",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content = Paragraph::new(lines).centered();
    frame.render_widget(content, inner);
}
