use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::{
    creature::{CreatureDef, Entity, School, Variant},
    state::{AquariumState, RuntimeMode, TankState, WaterBand},
    world::ReefWorld,
};

const MODAL_BORDER: Color = Color::LightCyan;
const REEFS_SOURCE_URL: &str = "https://github.com/mevanlc/reefs";

pub(crate) fn modal_inner_area(area: Rect) -> Rect {
    let popup = modal_outer_area(area);
    Rect::new(
        popup.x.saturating_add(1),
        popup.y.saturating_add(1),
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    )
}

pub fn draw_modal(frame: &mut Frame<'_>, area: Rect, state: &AquariumState) {
    let popup = modal_outer_area(area);
    frame.render_widget(Clear, popup);

    let block = Block::new()
        .title(format!(" Aquarium ({REEFS_SOURCE_URL}) "))
        .title_bottom(" Esc/q close ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(MODAL_BORDER))
        .style(Style::new().bg(Color::Black));
    let inner = modal_inner_area(area);
    frame.render_widget(block, popup);
    draw(frame, inner, state);
}

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &AquariumState) {
    match &app.mode {
        RuntimeMode::Tank(tank) => render_tank(frame, area, app, tank),
        RuntimeMode::Reef(reef) => {
            if area.height < reef.min_height {
                render_size_warning(frame, area, reef.min_height);
            } else {
                render_reef(frame, area, app, &reef.world);
            }
        }
    }
}

fn render_tank(frame: &mut Frame<'_>, area: Rect, app: &AquariumState, tank_state: &TankState) {
    if area.width < tank_state.width || area.height < tank_state.height {
        let message = Paragraph::new(vec![
            Line::from(format!(
                "Aquarium needs a {}x{} terminal.",
                tank_state.width, tank_state.height
            )),
            Line::from(format!("Current size: {}x{}", area.width, area.height)),
            Line::from("Resize the terminal, or press q / Esc to quit."),
        ])
        .style(Style::new().fg(Color::LightCyan));
        frame.render_widget(message, area);
        return;
    }

    let tank = centered_rect(area, tank_state.width, tank_state.height);
    let water = Rect::new(tank.x + 1, tank.y + 1, tank.width - 2, tank.height - 2);
    let block = Block::new()
        .title(" Aquarium ")
        .title_bottom(format!(" {} creatures ", app.entities.len()))
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .style(Style::new().bg(Color::Black));
    frame.render_widget(block, tank);

    if app.show_background {
        render_water(frame, water, app.tick);
    }
    render_creatures(
        frame,
        water,
        &app.definitions,
        &app.entities,
        app.tick,
        0,
        app.show_creature_names,
    );
}

fn render_reef(frame: &mut Frame<'_>, area: Rect, app: &AquariumState, world: &ReefWorld) {
    if app.show_background {
        let band = WaterBand::for_reef(world, area.height);
        let water = Rect::new(
            area.x,
            area.y + band.top.max(0) as u16,
            area.width,
            (band.bottom - band.top).max(0) as u16,
        );
        render_water(frame, water, app.tick);
    }

    render_layer(frame, area, world, LayerPosition::Surface);
    render_surface_overlay(frame, area);
    render_layer(frame, area, world, LayerPosition::Floor);
    render_creatures(
        frame,
        area,
        &app.definitions,
        &app.entities,
        app.tick,
        world.viewport_x,
        app.show_creature_names,
    );
}

fn render_surface_overlay(frame: &mut Frame<'_>, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let buffer = frame.buffer_mut();
    let label_style = Style::new().fg(Color::LightCyan);
    render_surface_text(buffer, area, 2, " aquarium ", label_style);
}

fn render_surface_text(buffer: &mut Buffer, area: Rect, offset: u16, text: &str, style: Style) {
    if offset >= area.width {
        return;
    }

    let x = area.x + offset;
    let width = area.right().saturating_sub(x) as usize;
    buffer.set_stringn(x, area.y, text, width, style);
}

fn render_size_warning(frame: &mut Frame<'_>, area: Rect, min_height: u16) {
    let message = Paragraph::new(vec![
        Line::from("Aquarium reef mode needs more rows."),
        Line::from(format!("Minimum rows: {min_height}")),
        Line::from(format!("Current rows: {}", area.height)),
        Line::from("Resize the terminal, or press q / Esc to quit."),
    ])
    .style(Style::new().fg(Color::LightCyan));
    frame.render_widget(message, area);
}

#[derive(Debug, Clone, Copy)]
enum LayerPosition {
    Surface,
    Floor,
}

fn render_layer(frame: &mut Frame<'_>, area: Rect, world: &ReefWorld, position: LayerPosition) {
    let (layer, start_y) = match position {
        LayerPosition::Surface => (&world.surface, area.y),
        LayerPosition::Floor => (
            &world.floor,
            area.bottom().saturating_sub(world.floor.height),
        ),
    };
    let style = Style::new().fg(layer.color);
    let buffer = frame.buffer_mut();

    for row in 0..layer.height {
        let y = start_y + row;
        if y >= area.bottom() {
            continue;
        }

        for x in 0..area.width {
            if let Some(symbol) = layer.cell_at(world.viewport_x + x as i32, row)
                && let Some(cell) = buffer.cell_mut((area.x + x, y))
            {
                let mut encoded = [0; 4];
                cell.set_symbol(symbol.encode_utf8(&mut encoded))
                    .set_style(style);
            }
        }
    }
}

fn render_water(frame: &mut Frame<'_>, area: Rect, tick: u64) {
    let buffer = frame.buffer_mut();
    let water_style = Style::new().fg(Color::DarkGray);
    for y in 0..area.height {
        for x in 0..area.width {
            let ripple = match (x as u64 + y as u64 * 3 + tick / 2) % 23 {
                0 => "~",
                7 => ".",
                _ => " ",
            };
            if ripple != " "
                && let Some(cell) = buffer.cell_mut((area.x + x, area.y + y))
            {
                cell.set_symbol(ripple).set_style(water_style);
            }
        }
    }
}

fn render_creatures(
    frame: &mut Frame<'_>,
    area: Rect,
    definitions: &[CreatureDef],
    entities: &[Entity],
    tick: u64,
    viewport_x: i32,
    show_names: bool,
) {
    let buffer = frame.buffer_mut();

    for entity in entities {
        if !entity.is_active() {
            continue;
        }

        let def = &definitions[entity.def];
        let variant = def.best_variant_for(
            entity.pose_dx_for(def),
            entity.pose_intent,
            entity.animation_tick_for(def, tick),
            entity.phase,
        );
        let style = Style::new().fg(entity.color).add_modifier(if def.brownian {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });

        if let Some(school) = &variant.school {
            render_school(buffer, area, entity, variant, school, viewport_x, style);
        } else {
            render_static_art(buffer, area, entity, variant, viewport_x, style);
        }

        if show_names {
            render_creature_name(
                buffer,
                area,
                entity,
                variant.width,
                variant.height,
                &def.name,
                viewport_x,
            );
        }
    }
}

fn render_static_art(
    buffer: &mut Buffer,
    area: Rect,
    entity: &Entity,
    variant: &Variant,
    viewport_x: i32,
    style: Style,
) {
    for (line_index, line) in variant.art.iter().enumerate() {
        let y = area.y as i32 + entity.y + line_index as i32;
        if y < area.y as i32 || y >= area.bottom() as i32 {
            continue;
        }

        let raw_x = area.x as i32 + entity.x - viewport_x;
        if raw_x >= area.right() as i32 {
            continue;
        }

        let (x, text) = if raw_x < area.x as i32 {
            let skip = (area.x as i32 - raw_x) as usize;
            let clipped = line.chars().skip(skip).collect::<String>();
            (area.x, clipped)
        } else {
            (raw_x as u16, line.clone())
        };

        if text.is_empty() || x >= area.right() {
            continue;
        }

        let width = area.right().saturating_sub(x) as usize;
        buffer.set_stringn(x, y as u16, text, width, style);
    }
}

fn render_school(
    buffer: &mut Buffer,
    area: Rect,
    entity: &Entity,
    variant: &Variant,
    school: &School,
    viewport_x: i32,
    style: Style,
) {
    let unit_width = school.unit.chars().count().max(1) as u16;
    let max_x = variant.width.saturating_sub(unit_width) as u64;
    let max_y = variant.height.saturating_sub(1) as u64;

    for (index, unit) in school.units.iter().enumerate() {
        let local_x = brownian_coordinate(
            unit.x as u64,
            max_x,
            entity.school_rearrangements,
            entity.phase,
            index,
            0,
        );
        let local_y = brownian_coordinate(
            unit.y as u64,
            max_y,
            entity.school_rearrangements,
            entity.phase,
            index,
            1,
        );
        let raw_x = area.x as i32 + entity.x - viewport_x + local_x as i32;
        let y = area.y as i32 + entity.y + local_y as i32;
        if y < area.y as i32 || y >= area.bottom() as i32 {
            continue;
        }

        render_clipped_text(buffer, area, raw_x, y as u16, &school.unit, style);
    }
}

fn brownian_coordinate(
    origin: u64,
    max: u64,
    rearrangements: u64,
    phase: usize,
    unit_index: usize,
    axis: u64,
) -> u64 {
    if max == 0 || rearrangements == 0 {
        return origin.min(max);
    }

    let seed = rearrangements
        .wrapping_add((phase as u64).wrapping_mul(0x9e37_79b9))
        .wrapping_add((unit_index as u64).wrapping_mul(0x85eb_ca6b))
        .wrapping_add(axis.wrapping_mul(0xc2b2_ae35));
    let drift = stable_hash(seed) % (max + 1);

    origin.wrapping_add(drift).wrapping_rem(max + 1)
}

fn stable_hash(mut value: u64) -> u64 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^ (value >> 33)
}

fn render_clipped_text(
    buffer: &mut Buffer,
    area: Rect,
    raw_x: i32,
    y: u16,
    text: &str,
    style: Style,
) {
    let text_width = text.chars().count() as i32;
    if text_width == 0 || raw_x >= area.right() as i32 || raw_x + text_width <= area.x as i32 {
        return;
    }

    let (x, text) = if raw_x < area.x as i32 {
        let skip = (area.x as i32 - raw_x) as usize;
        (area.x, text.chars().skip(skip).collect::<String>())
    } else {
        (raw_x as u16, text.to_string())
    };

    if text.is_empty() || x >= area.right() {
        return;
    }

    let width = area.right().saturating_sub(x) as usize;
    buffer.set_stringn(x, y, text, width, style);
}

fn render_creature_name(
    buffer: &mut Buffer,
    area: Rect,
    entity: &Entity,
    creature_width: u16,
    creature_height: u16,
    name: &str,
    viewport_x: i32,
) {
    let name_width = name.chars().count() as i32;
    if name_width == 0 {
        return;
    }

    let y = area.y as i32 + entity.y + creature_height as i32;
    if y < area.y as i32 || y >= area.bottom() as i32 {
        return;
    }

    let creature_center = area.x as i32 + entity.x - viewport_x + creature_width as i32 / 2;
    let raw_x = creature_center - name_width / 2;
    if raw_x >= area.right() as i32 || raw_x + name_width <= area.x as i32 {
        return;
    }

    let (x, text) = if raw_x < area.x as i32 {
        let skip = (area.x as i32 - raw_x) as usize;
        (area.x, name.chars().skip(skip).collect::<String>())
    } else {
        (raw_x as u16, name.to_string())
    };

    if text.is_empty() || x >= area.right() {
        return;
    }

    let width = area.right().saturating_sub(x) as usize;
    let style = Style::new().fg(Color::LightCyan);
    buffer.set_stringn(x, y as u16, text, width, style);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width.min(area.width),
        height.min(area.height),
    )
}

fn modal_outer_area(area: Rect) -> Rect {
    centered_rect(area, area.width.min(126), area.height.min(42))
}
