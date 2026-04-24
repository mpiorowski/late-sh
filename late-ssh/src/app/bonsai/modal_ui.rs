use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    bonsai::{
        care::{BonsaiCareState, CareMode, branch_targets_for},
        state::BonsaiState,
        ui::{TreeOverlay, render_tree_art_lines, tree_ascii},
    },
    common::theme,
};

const MODAL_WIDTH: u16 = 72;
const MODAL_HEIGHT: u16 = 26;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    bonsai: &BonsaiState,
    care: &BonsaiCareState,
    beat: f32,
) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Bonsai Care ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(15),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_header(frame, layout[0], bonsai, care);
    draw_tree(frame, layout[1], bonsai, care, beat);
    draw_status(frame, layout[2], bonsai, care);
    draw_controls(frame, layout[3], care);
}

fn draw_header(frame: &mut Frame, area: Rect, bonsai: &BonsaiState, care: &BonsaiCareState) {
    let water = if care.watered {
        Span::styled("watered", Style::default().fg(theme::SUCCESS()))
    } else {
        Span::styled("needs water", Style::default().fg(theme::AMBER()))
    };
    let branches = Span::styled(
        format!("{}/{}", care.branches_done(), care.branch_goal),
        Style::default()
            .fg(if care.all_branches_cut() {
                theme::SUCCESS()
            } else {
                theme::AMBER()
            })
            .add_modifier(Modifier::BOLD),
    );
    let line = Line::from(vec![
        Span::raw("  UTC "),
        Span::styled(
            care.date.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled("  ·  ", Style::default().fg(theme::BORDER_DIM())),
        Span::styled(
            bonsai.stage().label(),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled("  ·  ", Style::default().fg(theme::BORDER_DIM())),
        water,
        Span::styled("  · branches ", Style::default().fg(theme::TEXT_DIM())),
        branches,
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_tree(
    frame: &mut Frame,
    area: Rect,
    bonsai: &BonsaiState,
    care: &BonsaiCareState,
    beat: f32,
) {
    let stage = bonsai.stage();
    let art = tree_ascii(stage, bonsai.seed, bonsai.is_wilting());
    let targets = branch_targets_for(stage, bonsai.seed, care.date, &art, care.branch_goal);
    let selected_id = targets
        .get(care.cursor.min(targets.len().saturating_sub(1)))
        .map(|target| target.id);

    let mut tree_lines = render_tree_art_lines(
        stage,
        bonsai.seed,
        bonsai.is_wilting(),
        area.width as usize,
        beat,
        Some(TreeOverlay {
            targets: &targets,
            cut_branch_ids: &care.cut_branch_ids,
            selected_id,
            show_selection: care.mode == CareMode::Prune,
        }),
    );

    let mut lines = Vec::new();
    let top_pad = area.height.saturating_sub(tree_lines.len() as u16) as usize;
    for _ in 0..top_pad {
        lines.push(Line::from(""));
    }
    lines.append(&mut tree_lines);

    if care.water_animation_ticks > 0 {
        if let Some(line) = lines.last_mut() {
            line.spans.push(Span::styled(
                "  drip",
                Style::default()
                    .fg(theme::SUCCESS())
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_status(frame: &mut Frame, area: Rect, bonsai: &BonsaiState, care: &BonsaiCareState) {
    let penalty = if care.watered && care.all_branches_cut() {
        ("Daily care complete", theme::SUCCESS())
    } else if !care.watered && !care.all_branches_cut() {
        ("Missed water: -20%, missed pruning: -10%", theme::AMBER())
    } else if !care.watered {
        (
            "Water before UTC midnight or lose 20% growth",
            theme::AMBER(),
        )
    } else {
        (
            "Trim all marked branches or lose 10% growth",
            theme::AMBER(),
        )
    };

    let message = care.message.as_deref().unwrap_or(if bonsai.is_alive {
        penalty.0
    } else {
        "Plant anew with w"
    });
    let lines = vec![
        Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(penalty.1),
        ))
        .centered(),
        Line::from(vec![
            Span::styled("Growth ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                bonsai.growth_points.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" pts", Style::default().fg(theme::TEXT_DIM())),
        ])
        .centered(),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_controls(frame: &mut Frame, area: Rect, care: &BonsaiCareState) {
    let mode = match care.mode {
        CareMode::Water => "care",
        CareMode::Prune => "prune",
    };
    let lines = vec![
        Line::from(vec![
            key("w"),
            text(" water  "),
            key("p"),
            text(" reshape -1 stage  "),
            key("←/→ hjkl"),
            text(" move  "),
            key("x"),
            text(" cut"),
        ])
        .centered(),
        Line::from(vec![
            Span::styled(
                format!("mode: {mode}  "),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            key("Esc/q"),
            text(" close"),
        ])
        .centered(),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn key(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )
}

fn text(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM()))
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
