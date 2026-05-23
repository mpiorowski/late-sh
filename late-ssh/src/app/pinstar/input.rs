use crate::app::pinstar::data::DiagramLockMode;
use crate::app::pinstar::helpers::{contains_cell, move_textarea_cursor_to_mouse};
use crate::app::pinstar::state::{PinstarMenuType, PinstarState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui_textarea::{Input, Key, TextArea, WrapMode};

fn key_event_to_input(key: KeyEvent) -> Input {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    Input {
        key: match key.code {
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Enter => Key::Enter,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Esc => Key::Esc,
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            KeyCode::Home => Key::Home,
            KeyCode::End => Key::End,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,
            KeyCode::Tab => Key::Tab,
            KeyCode::Delete => Key::Delete,
            KeyCode::F(n) => Key::F(n),
            _ => {
                return Input {
                    key: Key::Null,
                    ctrl: false,
                    alt: false,
                    shift: false,
                };
            }
        },
        ctrl,
        alt,
        shift,
    }
}

pub fn handle_pinstar_mouse(
    state: &mut PinstarState,
    mouse: MouseEvent,
    area: ratatui::layout::Rect,
) -> bool {
    if state.rename_popup.is_some() || state.show_invite_dialog {
        return true;
    }

    let (editor_area, canvas_area) = if state.show_editor_pane {
        let main_chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([
                ratatui::layout::Constraint::Percentage(30),
                ratatui::layout::Constraint::Percentage(70),
            ])
            .split(area);
        (Some(main_chunks[0]), main_chunks[1])
    } else {
        (None, area)
    };

    let (cx, cy) = state.screen_to_canvas(mouse.column, mouse.row, canvas_area);
    state.last_mouse_canvas_pos = Some((cx, cy));

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Right) => {
            if state.resizing_node_id.is_some() {
                state.resizing_node_id = None;
                let _ = state.save();
                return true;
            }

            if let Some(editor_area) = editor_area
                && contains_cell(editor_area, mouse.column, mouse.row)
            {
                state.open_editor_context_menu(mouse.column, mouse.row);
                return true;
            }

            let (cx, cy) = state.screen_to_canvas(mouse.column, mouse.row, canvas_area);
            let hit_node = state.select_node_at(mouse.column, mouse.row, canvas_area);
            if hit_node.is_some() {
                state.open_context_menu(mouse.column, mouse.row, cx, cy);
            } else if state.select_edge_at(cx, cy).is_some() {
                state.open_edge_context_menu(mouse.column, mouse.row);
            } else {
                // Right-click on empty space: start selection rectangle
                state.select_rect_start = Some((cx, cy));
                state.select_rect_end = Some((cx, cy));
                state.last_mouse_pos = Some((mouse.column, mouse.row));
            }
            true
        }
        MouseEventKind::Down(MouseButton::Middle) => {
            state.last_mouse_pos = Some((mouse.column, mouse.row));
            true
        }
        MouseEventKind::Up(MouseButton::Middle) => {
            state.last_mouse_pos = None;
            true
        }
        MouseEventKind::Drag(MouseButton::Middle) => {
            if let Some((lx, ly)) = state.last_mouse_pos {
                let dx = mouse.column as f64 - lx as f64;
                let dy = mouse.row as f64 - ly as f64;
                state.pan(-dx, -dy);
                state.last_mouse_pos = Some((mouse.column, mouse.row));
                true
            } else {
                false
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let mut menu_action = None;
            let mut close_menu = false;

            if let Some(menu) = &state.context_menu {
                close_menu = true;
                let menu_width = 32;
                let menu_height = menu.items.len() as u16;

                if mouse.column >= menu.x
                    && mouse.column < menu.x + menu_width
                    && mouse.row >= menu.y
                    && mouse.row < menu.y + menu_height
                {
                    let selected = (mouse.row - menu.y) as usize;
                    if let Some(label) = menu.items.get(selected) {
                        menu_action = Some((label.clone(), menu.menu_type, menu.x, menu.y));
                    }
                }
            }

            if close_menu {
                state.context_menu = None;
            }

            if let Some((label, menu_type, mx, my)) = menu_action {
                execute_menu_action(state, &label, menu_type, mx, my);
                return true;
            }

            if let Some(editor_area) = editor_area {
                if contains_cell(editor_area, mouse.column, mouse.row) {
                    state.editor_focus = true;
                    let digits = state.raw_editor.lines().len().max(1).to_string().len() as u16;
                    let gutter_width = digits + 1;
                    let body_inner = ratatui::layout::Rect::new(
                        editor_area.x + gutter_width,
                        editor_area.y + 1,
                        editor_area.width.saturating_sub(gutter_width),
                        editor_area.height.saturating_sub(1),
                    );
                    move_textarea_cursor_to_mouse(
                        &mut state.raw_editor,
                        body_inner,
                        mouse.column,
                        mouse.row,
                    );
                    state.raw_editor.start_selection();
                    state.mouse_selecting = true;
                    state.mouse_dragged = false;
                    return true;
                } else {
                    state.editor_focus = false;
                }
            }

            let (cx, cy) = state.screen_to_canvas(mouse.column, mouse.row, canvas_area);

            if state.connection_source_id.is_some() {
                if let Some(target_id) = state.select_node_at(mouse.column, mouse.row, canvas_area)
                {
                    state.finish_connection(&target_id);
                } else {
                    state.connection_source_id = None;
                }
                return true;
            }

            if state.deleting_connection_source_id.is_some() {
                if let Some(target_id) = state.select_node_at(mouse.column, mouse.row, canvas_area)
                {
                    state.finish_delete_connection(&target_id);
                } else {
                    state.deleting_connection_source_id = None;
                }
                return true;
            }

            if let Some(resizing_id) = &state.resizing_node_id
                && let Some(node) = state.data.nodes.iter().find(|n| n.id() == resizing_id)
            {
                let (nx, ny) = node.pos();
                let (nw, nh) = node.size();
                let handle_x = nx + nw;
                let handle_y = ny + nh;

                let tolerance = 10.0 / state.zoom;
                if cx >= handle_x - tolerance
                    && cx <= handle_x + tolerance
                    && cy >= handle_y - tolerance
                    && cy <= handle_y + tolerance
                {
                    state.is_dragging_resize_handle = true;
                    state.last_mouse_pos = Some((mouse.column, mouse.row));
                    return true;
                }
            }

            if state.floating_editor.is_some() {
                let prev_selected = state.selected_node_id.clone();
                let mut is_inside_editor = false;

                if let Some(id) = &prev_selected {
                    if let Some(node) = state.data.nodes.iter().find(|n| n.id() == id) {
                        let (nx, ny) = node.pos();
                        let (nw, nh) = node.size();
                        let sx = ((nx - state.viewport_x) * state.zoom)
                            + (canvas_area.x as f64 + canvas_area.width as f64 / 2.0);
                        let sy = ((ny - state.viewport_y) * state.zoom)
                            + (canvas_area.y as f64 + canvas_area.height as f64 / 2.0);
                        let sw = nw * state.zoom;
                        let sh = nh * state.zoom;

                        let left = sx.round() as i32;
                        let top = sy.round() as i32;
                        let right = (sx + sw).round() as i32;
                        let bottom = (sy + sh).round() as i32;

                        let expansion_x = 2;
                        let expansion_y = 1;
                        let el = left - expansion_x;
                        let er = right + expansion_x;
                        let et = top - expansion_y;
                        let eb = bottom + expansion_y;

                        let mc = mouse.column as i32;
                        let mr = mouse.row as i32;
                        if mc >= el && mc < er && mr >= et && mr < eb {
                            is_inside_editor = true;
                        }
                    }
                }

                if !is_inside_editor {
                    state.selected_node_id = prev_selected;
                    state.toggle_editor();
                    let hit_node = state.select_node_at(mouse.column, mouse.row, canvas_area);
                    state.selected_node_id = hit_node.clone();

                    if hit_node.is_none() {
                        return true;
                    }
                } else {
                    return true;
                }
            }

            let is_double_click = if let Some((lx, ly, lt)) = state.last_click {
                lx == mouse.column && ly == mouse.row && lt.elapsed().as_millis() < 500
            } else {
                false
            };

            let hit_node = state.node_at(mouse.column, mouse.row, canvas_area);
            let is_already_selected = hit_node.as_ref().map_or(false, |id| {
                state.selected_node_id.as_ref() == Some(id)
                    || state.drag_captured_nodes.contains(id)
            });

            if is_double_click && hit_node.is_some() {
                if !is_already_selected {
                    state.drag_captured_nodes.clear();
                    let _ = state.select_node_at(mouse.column, mouse.row, canvas_area);
                }
                state.toggle_editor();
                state.last_click = None;
            } else if let Some(_) = hit_node {
                if !is_already_selected {
                    state.drag_captured_nodes.clear();
                    let _ = state.select_node_at(mouse.column, mouse.row, canvas_area);
                }
                state.capture_group_children();
                if state.check_mutation_permission() {
                    state.record_undo_state();
                    state.drag_start_pos = Some((cx, cy));
                }
                state.last_click = Some((mouse.column, mouse.row, std::time::Instant::now()));
            } else {
                state.drag_captured_nodes.clear();
                let _ = state.select_node_at(mouse.column, mouse.row, canvas_area);
                state.last_click = Some((mouse.column, mouse.row, std::time::Instant::now()));
            }

            state.last_mouse_pos = Some((mouse.column, mouse.row));
            true
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if state.mouse_selecting && !state.mouse_dragged {
                state.raw_editor.cancel_selection();
            }
            state.mouse_selecting = false;
            state.mouse_dragged = false;

            if state.drag_start_pos.is_some() {
                state.drag_start_pos = None;
                state.drag_group_children.clear();
                let _ = state.save();
            }
            state.last_mouse_pos = None;
            true
        }
        MouseEventKind::Up(MouseButton::Right) => {
            if let (Some(start), Some(end)) = (state.select_rect_start, state.select_rect_end) {
                if (start.0 - end.0).abs() > 5.0 || (start.1 - end.1).abs() > 5.0 {
                    // Significant drag: select nodes in rectangle
                    state.select_nodes_in_rect(start.0, start.1, end.0, end.1);
                    // If an edge was selected (no nodes in rect), show edge context menu
                    if state.selected_edge_id.is_some() && state.selected_node_id.is_none() {
                        state.open_edge_context_menu(mouse.column, mouse.row);
                    }
                } else {
                    // Just a click: show add-node menu
                    state.context_menu_pos = (start.0, start.1);
                    let items = vec!["Add Text Node".to_string(), "Add Group".to_string()];
                    state.context_menu = Some(crate::app::pinstar::state::PinstarContextMenu {
                        x: mouse.column,
                        y: mouse.row,
                        selected: 0,
                        items,
                        menu_type: crate::app::pinstar::state::PinstarMenuType::Canvas,
                    });
                }
            }
            state.select_rect_start = None;
            state.select_rect_end = None;
            state.last_mouse_pos = None;
            true
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if state.mouse_selecting {
                state.mouse_dragged = true;
                if let Some(editor_area) = editor_area {
                    let digits = state.raw_editor.lines().len().max(1).to_string().len() as u16;
                    let gutter_width = digits + 1;
                    let body_inner = ratatui::layout::Rect::new(
                        editor_area.x + gutter_width,
                        editor_area.y + 1,
                        editor_area.width.saturating_sub(gutter_width),
                        editor_area.height.saturating_sub(1),
                    );
                    move_textarea_cursor_to_mouse(
                        &mut state.raw_editor,
                        body_inner,
                        mouse.column,
                        mouse.row,
                    );
                    return true;
                }
            }

            if state.resizing_node_id.is_some()
                && !state.locked
                && let Some((lx, ly)) = state.last_mouse_pos
            {
                let dw = mouse.column as f64 - lx as f64;
                let dh = mouse.row as f64 - ly as f64;
                state.resize_selected_node(dw / state.zoom, dh / state.zoom);
                state.last_mouse_pos = Some((mouse.column, mouse.row));
                return true;
            }

            if let Some(last_pos) = state.drag_start_pos
                && !state.locked
            {
                let (cx, cy) = state.screen_to_canvas(mouse.column, mouse.row, canvas_area);
                let dx = cx - last_pos.0;
                let dy = cy - last_pos.1;
                state.move_selected_node(dx, dy);
                state.drag_start_pos = Some((cx, cy));
                true
            } else if let Some((lx, ly)) = state.last_mouse_pos {
                let dx = mouse.column as f64 - lx as f64;
                let dy = mouse.row as f64 - ly as f64;
                state.pan(-dx, -dy);
                state.last_mouse_pos = Some((mouse.column, mouse.row));
                true
            } else {
                false
            }
        }
        MouseEventKind::Drag(MouseButton::Right) => {
            if state.select_rect_start.is_some() {
                let (cx, cy) = state.screen_to_canvas(mouse.column, mouse.row, canvas_area);
                state.select_rect_end = Some((cx, cy));
                state.last_mouse_pos = Some((mouse.column, mouse.row));
                true
            } else {
                false
            }
        }
        MouseEventKind::ScrollUp => {
            if state.show_editor_pane && mouse.column < canvas_area.x {
                state.raw_editor.scroll((-3, 0));
            } else {
                state.zoom_in();
            }
            true
        }
        MouseEventKind::ScrollDown => {
            if state.show_editor_pane && mouse.column < canvas_area.x {
                state.raw_editor.scroll((3, 0));
            } else {
                state.zoom_out();
            }
            true
        }
        _ => false,
    }
}

fn execute_menu_action(
    state: &mut PinstarState,
    label: &str,
    menu_type: PinstarMenuType,
    menu_x: u16,
    menu_y: u16,
) {
    if menu_type == PinstarMenuType::Editor {
        match label {
            "Copy" => {
                state.raw_editor.copy();
            }
            "Cut" => {
                state.raw_editor.cut();
                let _ = state.sync_from_raw_editor();
            }
            "Paste" => {
                state.raw_editor.paste();
                let _ = state.sync_from_raw_editor();
            }
            "Select All" => {
                state.raw_editor.select_all();
            }
            _ => {}
        }
        return;
    }

    if menu_type == PinstarMenuType::ColorPicker {
        match label {
            "Default" => state.set_node_color(None),
            "Red" => state.set_node_color(Some("#ff5252".to_string())),
            "Orange" => state.set_node_color(Some("#ff9800".to_string())),
            "Yellow" => state.set_node_color(Some("#ffeb3b".to_string())),
            "Green" => state.set_node_color(Some("#4caf50".to_string())),
            "Cyan" => state.set_node_color(Some("#00bcd4".to_string())),
            "Blue" => state.set_node_color(Some("#2196f3".to_string())),
            "Purple" => state.set_node_color(Some("#9c27b0".to_string())),
            "Magenta" => state.set_node_color(Some("#e91e63".to_string())),
            "White" => state.set_node_color(Some("#ffffff".to_string())),
            _ => {}
        }
        state.selected_node_id = None;
        state.selected_edge_id = None;
        return;
    }

    if menu_type == PinstarMenuType::EdgeMenu {
        let items = match label {
            "Set Color..." => vec![
                "Default".to_string(),
                "Red".to_string(),
                "Orange".to_string(),
                "Yellow".to_string(),
                "Green".to_string(),
                "Cyan".to_string(),
                "Blue".to_string(),
                "Purple".to_string(),
                "Magenta".to_string(),
                "White".to_string(),
            ],
            "Set Style..." => vec!["Solid".to_string(), "Dashed".to_string()],
            _ => return,
        };
        let next_type = match label {
            "Set Color..." => PinstarMenuType::EdgeColorPicker,
            "Set Style..." => PinstarMenuType::EdgeStylePicker,
            _ => return,
        };
        state.context_menu = Some(crate::app::pinstar::state::PinstarContextMenu {
            x: menu_x,
            y: menu_y,
            selected: 0,
            items,
            menu_type: next_type,
        });
        return;
    }

    if menu_type == PinstarMenuType::EdgeColorPicker {
        let color = match label {
            "Default" => None,
            "Red" => Some("#ff5252".to_string()),
            "Orange" => Some("#ff9800".to_string()),
            "Yellow" => Some("#ffeb3b".to_string()),
            "Green" => Some("#4caf50".to_string()),
            "Cyan" => Some("#00bcd4".to_string()),
            "Blue" => Some("#2196f3".to_string()),
            "Purple" => Some("#9c27b0".to_string()),
            "Magenta" => Some("#e91e63".to_string()),
            "White" => Some("#ffffff".to_string()),
            _ => None,
        };
        state.set_edge_color(color);
        state.selected_edge_id = None;
        state.selected_node_id = None;
        return;
    }

    if menu_type == PinstarMenuType::EdgeStylePicker {
        let style = match label {
            "Solid" => crate::app::pinstar::data::EdgeStyle::Solid,
            "Dashed" => crate::app::pinstar::data::EdgeStyle::Dashed,
            _ => crate::app::pinstar::data::EdgeStyle::Solid,
        };
        state.set_edge_style(style);
        state.selected_edge_id = None;
        state.selected_node_id = None;
        return;
    }

    let node_id = state.selected_node_id.clone();

    match label {
        "Create Connection" => state.start_connection(),
        "Delete Connection" => state.start_delete_connection(),
        "Rename Node" => {
            if let Some(id) = node_id {
                let mut textarea = TextArea::from(vec![id.clone()]);
                textarea.set_cursor_line_style(ratatui::style::Style::default());
                textarea.set_wrap_mode(WrapMode::WordOrGlyph);
                textarea.set_block(
                    ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .title(" Rename Node (ID) - Enter to confirm, Esc to cancel "),
                );
                state.rename_popup = Some(textarea);
            }
        }
        "Resize Node" => state.start_resize(),
        "Set Color..." => {
            let items = vec![
                "Default".to_string(),
                "Red".to_string(),
                "Orange".to_string(),
                "Yellow".to_string(),
                "Green".to_string(),
                "Cyan".to_string(),
                "Blue".to_string(),
                "Purple".to_string(),
                "Magenta".to_string(),
                "White".to_string(),
            ];
            state.context_menu = Some(crate::app::pinstar::state::PinstarContextMenu {
                x: menu_x,
                y: menu_y,
                selected: 0,
                items,
                menu_type: PinstarMenuType::ColorPicker,
            });
        }
        "Delete All Connections" => state.delete_node_connections(),
        "Delete Node" => state.delete_selected_nodes(),
        "Add Text Node" => state.add_text_node(state.context_menu_pos.0, state.context_menu_pos.1),
        "Add Group" => state.add_group(state.context_menu_pos.0, state.context_menu_pos.1),
        _ => {}
    }
}

pub fn handle_pinstar_key(
    state: &mut PinstarState,
    key: KeyEvent,
    area: ratatui::layout::Rect,
    db: Option<late_core::db::Db>,
) -> bool {
    if state.show_help {
        state.show_help = false;
        return true;
    }

    if let Some(textarea) = &mut state.rename_popup {
        match key.code {
            KeyCode::Esc => {
                state.rename_popup = None;
            }
            KeyCode::Enter => {
                let new_id = textarea.lines().join("");
                state.rename_node(new_id);
                state.rename_popup = None;
            }
            _ => {
                textarea.input(key_event_to_input(key));
            }
        }
        return true;
    }

    let mut menu_action = None;
    let mut close_menu = false;

    if let Some(menu) = &mut state.context_menu {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                close_menu = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                menu.selected = menu.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if menu.selected < menu.items.len() - 1 {
                    menu.selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(label) = menu.items.get(menu.selected) {
                    menu_action = Some((label.clone(), menu.menu_type, menu.x, menu.y));
                }
                close_menu = true;
            }
            KeyCode::Char(c) => {
                let mut found_label = None;
                for label in &menu.items {
                    if let Some(sc) =
                        crate::app::pinstar::helpers::get_menu_shortcut_char(menu.menu_type, label)
                    {
                        if sc == c.to_ascii_lowercase() {
                            found_label = Some(label.clone());
                            break;
                        }
                    }
                }
                if let Some(label) = found_label {
                    menu_action = Some((label, menu.menu_type, menu.x, menu.y));
                    close_menu = true;
                }
            }
            _ => {}
        }
    }

    if close_menu {
        state.context_menu = None;
    }

    if let Some((label, menu_type, mx, my)) = menu_action {
        execute_menu_action(state, &label, menu_type, mx, my);
        return true;
    } else if close_menu {
        return true;
    }

    if state.show_invite_dialog {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.show_invite_dialog = false;
                state.invite_token = None;
                state.invite_error = None;
                state.invite_result_rx = None;
            }
            _ => {}
        }
        return true;
    }

    if state.context_menu.is_some() {
        return true;
    }

    let can_mutate = state.check_mutation_permission();
    if let Some(editor) = &mut state.floating_editor {
        match key.code {
            KeyCode::Esc => {
                state.toggle_editor();
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.toggle_editor();
            }
            _ => {
                if can_mutate {
                    editor.input(key_event_to_input(key));
                    if let Some(node_id) = &state.selected_node_id {
                        let text = editor.lines().join("\n");
                        for node in &mut state.data.nodes {
                            if node.id() == node_id {
                                node.set_text(text);
                                break;
                            }
                        }
                        let _ = state.save();
                    }
                }
            }
        }
        return true;
    }

    if state.resizing_node_id.is_some() {
        match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                state.resizing_node_id = None;
                let _ = state.save();
                return true;
            }
            _ => {}
        }
    }

    if state.editor_focus {
        match key.code {
            KeyCode::Esc => {
                state.editor_focus = false;
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                state.editor_focus = false;
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let _ = state.sync_from_raw_editor();
            }
            _ => {
                if state.check_mutation_permission() {
                    state.raw_editor.input(key_event_to_input(key));
                }
            }
        }
        return true;
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            if state.show_help {
                state.show_help = false;
                return true;
            }
            if state.show_invite_dialog {
                state.show_invite_dialog = false;
                return true;
            }
            if state.connection_source_id.is_some() {
                state.connection_source_id = None;
                return true;
            } else {
                // In late-ssh integration, Esc/q in pinstar is handled by the global input dispatcher.
                // Returning false lets the global handler see it.
                return false;
            }
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            if state.show_editor_pane {
                state.editor_focus = true;
            }
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if state.role() == "owner" {
                let next_mode = match state.lock_mode() {
                    DiagramLockMode::Unlocked => DiagramLockMode::All,
                    DiagramLockMode::All => DiagramLockMode::EditorOnly,
                    DiagramLockMode::EditorOnly => DiagramLockMode::Unlocked,
                };
                state.set_lock_mode(next_mode);
                let _ = state.save();
            }
        }
        KeyCode::Char('?') | KeyCode::Char('/') => {
            state.show_help = true;
        }
        KeyCode::Char('I') => {
            if let Some(db) = db {
                state.show_invite_dialog = true;
                state.generate_invite(db, "editor".to_string());
            }
        }
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.orthogonal_connections = !state.orthogonal_connections;
        }
        KeyCode::Char('z') | KeyCode::Char('Z')
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            let _ = state.redo();
        }
        KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let _ = state.redo();
        }
        KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let _ = state.undo();
        }
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let _ = state.save();
        }
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let _ = state.reload();
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let canvas_area = if state.show_editor_pane {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                    .split(area);
                chunks[1]
            } else {
                area
            };
            state.fit_to_view(canvas_area);
        }
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.zoom_in();
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.zoom_out();
        }
        KeyCode::Left | KeyCode::Char('h') => {
            state.select_node_in_direction(-1.0, 0.0);
            state.center_on_selected();
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.select_node_in_direction(1.0, 0.0);
            state.center_on_selected();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.select_node_in_direction(0.0, -1.0);
            state.center_on_selected();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.select_node_in_direction(0.0, 1.0);
            state.center_on_selected();
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            state.zoom_in();
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            state.zoom_out();
        }
        KeyCode::Char('c') if state.selected_node_id.is_some() => {
            state.start_connection();
        }
        KeyCode::Char('d') if state.selected_node_id.is_some() => {
            state.start_delete_connection();
        }
        KeyCode::Char('r') if state.selected_node_id.is_some() => {
            if let Some(id) = &state.selected_node_id {
                let mut textarea = TextArea::from(vec![id.clone()]);
                textarea.set_cursor_line_style(ratatui::style::Style::default());
                textarea.set_wrap_mode(WrapMode::WordOrGlyph);
                textarea.set_block(
                    ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .title(" Rename Node (ID) - Enter to confirm, Esc to cancel "),
                );
                state.rename_popup = Some(textarea);
            }
        }
        KeyCode::Char('s') if state.selected_node_id.is_some() => {
            state.start_resize();
        }
        KeyCode::Char('o') if state.selected_node_id.is_some() => {
            let items = vec![
                "Default".to_string(),
                "Red".to_string(),
                "Orange".to_string(),
                "Yellow".to_string(),
                "Green".to_string(),
                "Cyan".to_string(),
                "Blue".to_string(),
                "Purple".to_string(),
                "Magenta".to_string(),
                "White".to_string(),
            ];
            let menu_x = (area.width / 2).saturating_sub(16);
            let menu_y = (area.height / 2).saturating_sub(6);
            state.context_menu = Some(crate::app::pinstar::state::PinstarContextMenu {
                x: menu_x,
                y: menu_y,
                selected: 0,
                items,
                menu_type: PinstarMenuType::ColorPicker,
            });
        }
        KeyCode::Char('b') if state.selected_node_id.is_some() => {
            state.delete_node_connections();
        }
        KeyCode::Char('x') if state.selected_node_id.is_some() => {
            state.delete_selected_nodes();
        }
        KeyCode::Char('i') | KeyCode::Enter => {
            let target_id_opt = state.selected_node_id.clone();
            if let Some(target_id) = target_id_opt {
                if state.connection_source_id.is_some() {
                    state.finish_connection(&target_id);
                } else {
                    state.toggle_editor();
                }
            }
        }
        KeyCode::Char('a') => {
            let menu_x = (area.width / 2).saturating_sub(12);
            let menu_y = area.height;

            let cx = state.viewport_x;
            let cy = state.viewport_y;

            if let Some(id) = &state.selected_node_id {
                if state.data.nodes.iter().any(|n| n.id() == id) {
                    state.open_context_menu(menu_x, menu_y, cx, cy);
                }
            } else {
                state.open_context_menu(menu_x, menu_y, cx, cy);
            }
        }
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.show_grid = !state.show_grid;
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.show_editor_pane = !state.show_editor_pane;
            if !state.show_editor_pane {
                state.editor_focus = false;
            }
        }

        _ => return false,
    }

    true
}
