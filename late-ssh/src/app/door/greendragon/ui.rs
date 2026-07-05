//! Rendering for the Green Dragon door: the live game page and the Games-hub
//! landing card. Pure presentation — everything is read off [`State`] getters
//! and the [`Character`]; no game logic lives here.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::common::theme;
use crate::app::door::landing;

use super::data;
use super::model::{self, Character, Specialty};
use super::state::{FoeKind, Mode, State};

/// Draw the live Green Dragon game (called when a character is loaded).
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SUCCESS()))
        .title(Span::styled(
            " Legend of the Green Dragon ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 30 || inner.height < 10 {
        frame.render_widget(
            Paragraph::new("Terminal too small for Legend of the Green Dragon"),
            inner,
        );
        return;
    }

    let Some(c) = state.character() else {
        frame.render_widget(
            Paragraph::new("Loading your character from the realm...").alignment(Alignment::Center),
            inner,
        );
        return;
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(0)])
        .split(inner);

    draw_stats(frame, cols[0], c);
    draw_main(frame, cols[1], state, c);
}

fn draw_stats(frame: &mut Frame, area: Rect, c: &Character) {
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let gold = Style::default().fg(theme::BADGE_GOLD());

    let stat = |label: &str, value: String, value_style: Style| {
        Line::from(vec![
            Span::styled(format!("{label:<9}"), dim),
            Span::styled(value, value_style),
        ])
    };

    let exp_target = c.exp_for_next_level();
    let mut lines = vec![
        // The dragon-kill title precedes the name (LoGD renders "Farmboy Name").
        Line::from(Span::styled(
            c.titled_name(),
            bright.add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        stat("Level", c.level.to_string(), bright),
        stat("Race", c.race.name().to_string(), bright),
        stat(
            "HP",
            format!("{}/{}", c.hitpoints, c.max_hitpoints()),
            Style::default().fg(theme::SUCCESS()),
        ),
        stat("Attack", c.attack().to_string(), bright),
        stat("Defense", c.defense().to_string(), bright),
        Line::raw(""),
        stat(
            "Weapon",
            data::weapon_name(c.weapon_tier).to_string(),
            bright,
        ),
        stat("Armor", data::armor_name(c.armor_tier).to_string(), bright),
        Line::raw(""),
        stat("Gold", c.gold.to_string(), gold),
        stat("Bank", c.gold_in_bank.to_string(), gold),
        stat("Gems", c.gems.to_string(), gold),
        stat(
            "Exp",
            if c.level >= data::MAX_LEVEL {
                format!("{}", c.experience)
            } else {
                format!("{}/{}", c.experience, exp_target)
            },
            dim,
        ),
        stat("Turns", c.turns.to_string(), bright),
        stat("Dragons", c.dragon_kills.to_string(), gold),
        stat(
            "DK pts",
            format!("{} to spend", c.dragon_points_unspent),
            if c.dragon_points_unspent > 0 {
                gold
            } else {
                dim
            },
        ),
        stat(
            "Boons",
            format!(
                "{}a {}d {}hp {}ff",
                c.dragon_attack_bonus,
                c.dragon_defense_bonus,
                c.dragon_hp_bonus,
                c.dragon_ff_bonus
            ),
            dim,
        ),
        stat("Charm", c.charm.to_string(), bright),
        stat(
            "Soul",
            format!("{}/{}", c.soulpoints, c.max_soulpoints()),
            bright,
        ),
        stat("Favor", c.favor.to_string(), bright),
    ];

    // Living companions (e.g. a Bonecall skeleton), if any are at your side.
    if !c.companions.is_empty() {
        lines.push(Line::raw(""));
        for comp in &c.companions {
            lines.push(stat(
                "Ally",
                format!(
                    "{} ({}/{} HP)",
                    comp.name, comp.hitpoints, comp.max_hitpoints
                ),
                dim,
            ));
        }
    }

    // Specialty (once chosen): the path, and today's spendable skill uses.
    if c.specialty != Specialty::None {
        lines.push(Line::raw(""));
        lines.push(stat("Path", c.specialty.name().to_string(), bright));
        lines.push(stat(
            "Focus",
            format!("{} uses (skill {})", c.specialty_uses, c.specialty_skill),
            dim,
        ));
    }

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme::TEXT_FAINT()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_main(frame: &mut Frame, area: Rect, state: &State, c: &Character) {
    // Reserve the bottom for the message log; the rest is the active panel.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(9)])
        .split(area);

    draw_panel(frame, rows[0], state, c);
    draw_log(frame, rows[1], state);
}

fn draw_panel(frame: &mut Frame, area: Rect, state: &State, c: &Character) {
    let mut lines = vec![Line::from(Span::styled(
        panel_title(state.mode()),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))];

    // Fight panels get a foe banner above the action list. Multi-fights list
    // every foe; the first living one is the player's current target.
    if state.mode() == Mode::Fight
        && let Some(enc) = state.encounter()
    {
        lines.push(Line::raw(""));
        let target = enc.target();
        for (i, foe) in enc.foes.iter().enumerate() {
            let dead = foe.hp == 0;
            let name_style = if dead {
                Style::default().fg(theme::TEXT_FAINT())
            } else {
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD)
            };
            let mut spans = vec![
                Span::styled(foe.name.clone(), name_style),
                Span::styled(
                    format!("  wields {}", foe.weapon),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(
                    if dead {
                        "slain".to_string()
                    } else {
                        format!("{}/{} HP", foe.hp, foe.max_hp)
                    },
                    Style::default().fg(if dead { theme::TEXT_FAINT() } else { theme::ERROR() }),
                ),
            ];
            if target == Some(i) && enc.foes.len() > 1 {
                spans.push(Span::styled(
                    "  < target",
                    Style::default().fg(theme::AMBER()),
                ));
            }
            lines.push(Line::from(spans));
        }
        // Torment fights run on the soul pool, not the body's hitpoints.
        let (label, max) = if enc.kind == FoeKind::Torment {
            ("Your soul ", c.max_soulpoints())
        } else {
            ("Your HP ", c.max_hitpoints())
        };
        lines.push(Line::from(vec![
            Span::styled(label, Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}/{}", c.hitpoints, max),
                Style::default().fg(theme::SUCCESS()),
            ),
        ]));
    }

    if state.mode() == Mode::Graveyard {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "You are dead. Broken tombstones crowd a weed-choked yard, and at its heart looms the mausoleum of {}, warden of the dead.",
                data::DEATH_OVERLORD
            ),
            dim,
        )));
        lines.push(Line::from(Span::styled(
            "Your soul is your strength here; torment the lost to earn the warden's favor, or rest until a new day returns you to the living.",
            dim,
        )));
        let c_favor = c.favor;
        let tier = if c_favor >= model::RESURRECTION_FAVOR_COST {
            format!(
                "{} is impressed indeed. He will barter your life back for {} favor.",
                data::DEATH_OVERLORD,
                model::RESURRECTION_FAVOR_COST
            )
        } else if c_favor >= model::HAUNT_FAVOR_THRESHOLD {
            format!(
                "{} is moderately impressed. At {} favor he will barter your life back.",
                data::DEATH_OVERLORD,
                model::RESURRECTION_FAVOR_COST
            )
        } else {
            format!(
                "{} is not yet impressed with your efforts ({} favor stirs his interest; {} buys your life back).",
                data::DEATH_OVERLORD,
                model::HAUNT_FAVOR_THRESHOLD,
                model::RESURRECTION_FAVOR_COST
            )
        };
        lines.push(Line::from(Span::styled(tier, dim)));
    }

    // A forest event shows its framing narration above the accept/decline rows.
    if state.mode() == Mode::Event
        && let Some(event) = state.pending_event()
    {
        lines.push(Line::raw(""));
        for line in event.present(c).intro {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(theme::TEXT_DIM()),
            )));
        }
    }

    // The daily news: one day per page, newest first (news.php).
    if state.mode() == Mode::News {
        let dim = Style::default().fg(theme::TEXT_DIM());
        let (days_back, lines_opt) = state.news_page();
        let day_label = match days_back {
            0 => "Today in Duskmere".to_string(),
            1 => "Yesterday in Duskmere".to_string(),
            n => format!("Duskmere, {n} days ago"),
        };
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            day_label,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )));
        match lines_opt {
            None => lines.push(Line::from(Span::styled(
                "The crier is still clearing his throat...",
                dim,
            ))),
            Some([]) => lines.push(Line::from(Span::styled(
                "Nothing of note happened this day.",
                dim,
            ))),
            Some(items) => {
                for item in items {
                    lines.push(Line::from(Span::styled(format!("- {item}"), dim)));
                }
            }
        }
    }

    if state.mode() == Mode::ChooseStyle {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "How shall the realm address you? The choice colors your titles, and whose eye you catch at the inn. Pick the style that suits you; it is yours for good.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    if state.mode() == Mode::ChooseRace {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "A new day stirs old memories. Whose blood runs in your veins? The choice is permanent, and each people carries its own gift.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    if state.mode() == Mode::ChooseSpecialty {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Choose the craft you'll hone against the forest. The choice is permanent; you'll spend daily \"uses\" on its skills mid-fight.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    if state.mode() == Mode::SpendDragonPoints {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "The dragon's fall left you changed. Spend your {} unspent dragon point{} before returning to the village; each buys a permanent boon.",
                c.dragon_points_unspent,
                if c.dragon_points_unspent == 1 { "" } else { "s" }
            ),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    lines.push(Line::raw(""));
    for (i, (label, enabled)) in state.menu().into_iter().enumerate() {
        let selected = i == state.cursor();
        let style = match (selected, enabled) {
            (true, true) => Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            (true, false) => Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::REVERSED),
            (false, true) => Style::default().fg(theme::TEXT_BRIGHT()),
            (false, false) => Style::default().fg(theme::TEXT_FAINT()),
        };
        let marker = if selected { "> " } else { "  " };
        lines.push(Line::from(Span::styled(format!("{marker}{label}"), style)));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        controls_hint(state.mode()),
        Style::default().fg(theme::TEXT_FAINT()),
    )));

    let block = Block::default().borders(Borders::NONE);
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(block.inner(area))[1];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_log(frame: &mut Frame, area: Rect, state: &State) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::TEXT_FAINT()))
        .title(Span::styled(
            " Recent events ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = state
        .log_lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(theme::TEXT()),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn panel_title(mode: Mode) -> &'static str {
    match mode {
        Mode::Loading => "Entering the realm...",
        Mode::Village => "The village of Duskmere",
        Mode::Forest => "The Forest",
        Mode::Fight => "Battle!",
        Mode::WeaponShop => "Ironroost Weapons",
        Mode::ArmorShop => "Duskmail Armoury",
        Mode::Healer => "The Mendery",
        Mode::Bank => "The Coinvault",
        Mode::Training => "The Proving Yard",
        Mode::Event => "A Forest Happening",
        Mode::ChooseStyle => "A Manner of Address",
        Mode::ChooseRace => "Remember Your Blood",
        Mode::ChooseSpecialty => "Choose Your Path",
        Mode::Graveyard => "The Graveyard",
        Mode::SpendDragonPoints => "Dragon Points",
        Mode::News => "The Daily News",
    }
}

fn controls_hint(mode: Mode) -> &'static str {
    match mode {
        Mode::Fight => "up/down select   Enter act   Esc try to flee",
        Mode::Village | Mode::Graveyard => "up/down move   Enter choose   Esc leave the game",
        Mode::SpendDragonPoints => "up/down move   Enter spend   Esc leave the game",
        Mode::ChooseStyle | Mode::ChooseRace => "up/down move   Enter choose   Esc leave the game",
        _ => "up/down move   Enter choose   Esc back to village",
    }
}

/// Two-column Green Dragon landing card for the Games hub.
pub fn draw_landing(frame: &mut Frame, area: Rect, delete_confirm: bool) {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let mut lines = vec![Line::raw("")];
    lines.extend(title_art());
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            "An open-source remake of LORD ",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "(Legend of the Green Dragon)",
            Style::default().fg(theme::AMBER_DIM()),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "Hunt the forest, train against the masters, gear up, and slay the Green Dragon. Your character persists.",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));
    lines.push(landing::heading("The Loop"));
    lines.push(landing::stat(
        "Forest",
        "fight creatures for gold and experience",
        10,
    ));
    lines.push(landing::stat(
        "Masters",
        "beat your level master to advance",
        10,
    ));
    lines.push(landing::stat(
        "Dragon",
        "reach level 15, then end the run in glory",
        10,
    ));
    lines.push(Line::raw(""));
    lines.push(landing::heading("Enter"));
    lines.push(landing::action(
        ">",
        "Enter",
        "step into the village",
        theme::SUCCESS(),
    ));
    lines.push(landing::action(
        " ",
        "d",
        "reset your character",
        theme::ERROR(),
    ));
    lines.push(Line::raw(""));
    lines.push(landing::heading("Once Inside"));
    lines.push(landing::hint("up/down", "move the menu cursor", 10));
    lines.push(landing::hint("Enter", "choose", 10));
    lines.push(landing::hint("Esc", "back out / leave", 10));

    if delete_confirm {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Delete your Green Dragon character?",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled("Enter/Y", Style::default().fg(theme::ERROR())),
            Span::styled(" confirm  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("N/Esc", Style::default().fg(theme::AMBER())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]));
    } else {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Esc leaves the game back to this gate.",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn title_art() -> Vec<Line<'static>> {
    [
        "  ___                      ___                         ",
        " / __|_ _ ___ ___ _ _    |   \\ _ _ __ _ __ _ ___ _ _  ",
        "| (_ | '_/ -_) -_) ' \\   | |) | '_/ _` / _` / _ \\ ' \\ ",
        " \\___|_| \\___\\___|_||_|  |___/|_| \\__,_\\__, \\___/_||_|",
        "                                       |___/          ",
    ]
    .into_iter()
    .map(|line| {
        Line::from(Span::styled(
            line,
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ))
    })
    .collect()
}
