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

use super::commentary::{self, CommentRoom};
use super::data;
use super::model::{self, Character, Specialty};
use super::state::{BankOp, FoeKind, Mode, PvpVenue, State};

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
                c.dragon_attack_bonus, c.dragon_defense_bonus, c.dragon_hp_bonus, c.dragon_ff_bonus
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
                    Style::default().fg(if dead {
                        theme::TEXT_FAINT()
                    } else {
                        theme::ERROR()
                    }),
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

    // A commentary room: the venue framing, then the window's lines oldest
    // to newest (they arrive newest first), then the talk line being typed.
    if let Mode::Commentary(room) = state.mode() {
        let dim = Style::default().fg(theme::TEXT_DIM());
        let intro = match room {
            CommentRoom::Village => "Villagers trade the day's gossip around you.",
            CommentRoom::Inn => "You lean in at the long table and follow the talk.",
            CommentRoom::DarkHorse => "Names and boasts are scratched deep into the tabletop.",
            CommentRoom::Gardens => "Voices drift low between the hedges.",
            CommentRoom::Veterans => "Beyond the stone door, old scars trade stories.",
            CommentRoom::ShadeGypsy => "Through the trance, the dead press close to be heard.",
            CommentRoom::ShadeGrave => "Nearby, the lost souls give voice to their grief.",
            CommentRoom::Waiting => {
                "Plush leather chairs, potted bushes, and tinny muzak from a fake rock."
            }
            CommentRoom::ClanHall(_) => {
                "The secret levers give, the lock disengages, and your clan mates look up."
            }
        };
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(intro, dim)));
        let page_no = state.commentary_page_no();
        if page_no > 0 {
            lines.push(Line::from(Span::styled(
                format!(
                    "You leaf back through older talk ({page_no} page{} deep).",
                    if page_no == 1 { "" } else { "s" }
                ),
                dim,
            )));
        }
        let seen_day = state.comments_seen_day();
        match state.commentary_page() {
            None => lines.push(Line::from(Span::styled("You lean in to listen...", dim))),
            Some([]) if page_no > 0 => lines.push(Line::from(Span::styled(
                "The talk goes no further back than this.",
                dim,
            ))),
            Some([]) => lines.push(Line::from(Span::styled(
                "It is quiet. No one has spoken here in an age.",
                dim,
            ))),
            Some(items) => {
                for item in items.iter().rev() {
                    let style = if item.name.is_empty() {
                        dim
                    } else {
                        Style::default().fg(theme::TEXT())
                    };
                    let composed = commentary::compose_line(&item.name, &item.body);
                    // The new-post marker (upstream's `new.gif` on rows past
                    // the reader's `recentcomments` watermark).
                    if item.day >= seen_day {
                        lines.push(Line::from(vec![
                            Span::styled("• ", Style::default().fg(theme::TEXT_BRIGHT())),
                            Span::styled(composed, style),
                        ]));
                    } else {
                        lines.push(Line::from(Span::styled(composed, style)));
                    }
                }
            }
        }
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("say> {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The warrior list and the Hall of Fame: a heading, a column header,
    // the built rows, and any footer lines (fuzz note, your percentile).
    if matches!(
        state.mode(),
        Mode::WarriorList | Mode::HallOfFame | Mode::BountyList | Mode::ClanDetail
    ) {
        let dim = Style::default().fg(theme::TEXT_DIM());
        let page = match state.mode() {
            Mode::WarriorList => state.warrior_page(),
            Mode::BountyList => state.bounty_page_view(),
            Mode::ClanDetail => state.clan_detail_page(),
            _ => state.hall_of_fame_page(),
        };
        lines.push(Line::raw(""));
        match page {
            None => lines.push(Line::from(Span::styled(
                "The herald thumbs through his ledger...",
                dim,
            ))),
            Some(page) => {
                lines.push(Line::from(Span::styled(
                    page.heading.clone(),
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                )));
                if let Some(header) = &page.header {
                    lines.push(Line::from(Span::styled(header.clone(), dim)));
                }
                if page.rows.is_empty() {
                    lines.push(Line::from(Span::styled("No one at all.", dim)));
                }
                for row in &page.rows {
                    // Your own Hall of Fame row is marked with a star.
                    let style = if row.starts_with('*') {
                        Style::default().fg(theme::AMBER())
                    } else {
                        Style::default().fg(theme::TEXT())
                    };
                    lines.push(Line::from(Span::styled(row.clone(), style)));
                }
                for foot in &page.foot {
                    lines.push(Line::from(Span::styled(foot.clone(), dim)));
                }
            }
        }
        if state.mode() == Mode::WarriorList
            && let Some(input) = state.talk_line()
        {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("whose name? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The PvP target lists: how many attacks remain, and a rumor of the
    // sleepers you can't reach from here (upstream's location counts).
    if let Mode::PvpList(venue) = state.mode() {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            match venue {
                PvpVenue::Fields => "Out in the dark fields, unwitting warriors sleep off the day.",
                PvpVenue::Inn => "The keys clink onto the counter, one for each room upstairs.",
            },
            dim,
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "You have {} PvP attack{} left today.",
                c.player_fights,
                if c.player_fights == 1 { "" } else { "s" }
            ),
            dim,
        )));
        let elsewhere = state.pvp_elsewhere();
        if elsewhere > 0 {
            lines.push(Line::from(Span::styled(
                match venue {
                    PvpVenue::Fields => format!(
                        "Talk around town says {elsewhere} more sleep{} behind the inn's locked doors.",
                        if elsewhere == 1 { "s" } else { "" }
                    ),
                    PvpVenue::Inn => format!(
                        "{elsewhere} more sleep{} rough out in the fields.",
                        if elsewhere == 1 { "s" } else { "" }
                    ),
                },
                dim,
            )));
        }
    }

    // The bounty broker's booth: his greeting and the price on your head.
    if state.mode() == Mode::DagTable {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "{} sulks in the inn's darkest booth, a pipe clamped in his teeth. \
                 He deals in one commodity: other people's deaths.",
                data::BOUNTY_BROKER
            ),
            dim,
        )));
        lines.push(Line::from(Span::styled(
            match state.bounty_on_my_head() {
                None => "He looks you over slowly, saying nothing yet.".to_string(),
                Some(0) => {
                    "\"No price on your head just now. I'd be keeping it that way.\"".to_string()
                }
                Some(gold) => format!(
                    "\"There's {gold} gold riding on your head. I'd be watching \
                     my back, were I you.\""
                ),
            },
            dim,
        )));
    }

    // Picking a contract's target: the broker's terms, and the name line.
    if state.mode() == Mode::BountyTarget {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "\"Who's it to be? They must be level {} at the least, past the \
                 realm's protection, and not carrying too much contract already. \
                 My listing fee is {}%, paid when the ink dries.\"",
                model::BOUNTY_MIN_TARGET_LEVEL,
                model::BOUNTY_FEE_PCT
            ),
            dim,
        )));
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("whose head? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // Naming the price: the floor, the ceiling, and the fee.
    if state.mode() == Mode::BountyAmount {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        if let Some((name, level)) = state.bounty_target_info() {
            let min = model::BOUNTY_MIN_PER_LEVEL * level as u64;
            let cap = model::BOUNTY_MAX_PER_LEVEL * level as u64;
            lines.push(Line::from(Span::styled(
                format!(
                    "\"{name}, then. I'll take no less than {min} gold, and the \
                     total on that head stops at {cap}. My {}% comes off the top.\"",
                    model::BOUNTY_FEE_PCT
                ),
                dim,
            )));
        }
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("how much gold? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The teller's counter: the holdings in question, and the amount line.
    if let Mode::BankAmount(op) = state.mode() {
        let dim = Style::default().fg(theme::TEXT_DIM());
        if let Some(c) = state.character() {
            let terms = match op {
                BankOp::Deposit => format!(
                    "\"How much goes in? You carry {} gold. Leave the line \
                     blank to deposit it all.\"",
                    c.gold
                ),
                BankOp::Withdraw => format!(
                    "\"How much comes out? Your balance is {} gold. Leave the \
                     line blank to withdraw it all.\"",
                    c.gold_in_bank.max(0)
                ),
                BankOp::Borrow => format!(
                    "\"How much do you need in hand? Balance and credit \
                     together, the bank can give you {} gold. Anything past \
                     your balance is a loan, and debt gathers interest daily.\"",
                    c.borrow_available()
                ),
            };
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(terms, dim)));
        }
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("how much gold? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The vault's transfer window: the terms of the house, and the name line.
    if state.mode() == Mode::BankTransferTarget {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "The banker slides the transfer ledger across the counter. \
                 \"You may send {} more gold today, and no account takes more \
                 than {} per its holder's level in one note.\"",
                state.transfer_out_left(),
                model::TRANSFER_PER_LEVEL
            ),
            dim,
        )));
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("to whom? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // Writing the sum: the recipient's cap, the day's allowance, the minimum.
    if state.mode() == Mode::BankTransferAmount {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        if let Some((name, level)) = state.transfer_target_info() {
            let cap = model::TRANSFER_PER_LEVEL * level as u64;
            lines.push(Line::from(Span::styled(
                format!(
                    "\"{name}, then. Their account takes up to {cap} gold in one \
                     note; you may still send {} today, and nothing smaller \
                     than your level.\"",
                    state.transfer_out_left()
                ),
                dim,
            )));
        }
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("how much gold? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The barman's counter: his greeting and the price of a word.
    if state.mode() == Mode::TavernBartender {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "The barman is a dried stick of a man, all knuckles and squint. \
             He knows every warrior who ever drank here, and plenty who never did.",
            dim,
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "\"A hundred gold buys everything I know about a name. You carry {}.\"",
                c.gold
            ),
            dim,
        )));
    }

    // Picking whose name to buy: the terms, and the name line.
    if state.mode() == Mode::IntelTarget {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "\"Who do you want to know about? {} gold a name, friend or foe — \
                 I don't ask which.\"",
                model::INTEL_COST
            ),
            dim,
        )));
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("about whom? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The barman's rundown (or his mock sheet), line by line.
    if state.mode() == Mode::IntelSheet {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        match state.intel_sheet_lines() {
            Some(sheet) => {
                for line in sheet {
                    lines.push(Line::from(Span::styled(line.clone(), dim)));
                }
            }
            None => {
                lines.push(Line::from(Span::styled(
                    "The barman pours slowly, gathering what he knows...",
                    dim,
                )));
            }
        }
    }

    // The haunt: the warden's leave, your favor, and the name line.
    if state.mode() == Mode::Haunt {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "{} parts the veil a finger's width: the mortal world, asleep and \
                 unguarded. One haunting costs {} favor, roll the dice as it may.",
                data::DEATH_OVERLORD,
                model::HAUNT_FAVOR_THRESHOLD
            ),
            dim,
        )));
        lines.push(Line::from(Span::styled(
            format!("You hold {} favor.", c.favor),
            dim,
        )));
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("whose dreams? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The clan lobby: the registrar's marble hall, and any pending
    // application's status (applicant.php).
    if state.mode() == Mode::ClanLobby {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "A marble lobby ringed with intricately locked doors, one per clan \
                 hall. Behind a polished desk sits {}, the clan registrar.",
                data::CLAN_REGISTRAR
            ),
            dim,
        )));
        if c.clan_id.is_some() {
            lines.push(Line::from(Span::styled(
                match state.clan_view() {
                    Some((clan, _)) => format!(
                        "\"Your application to {} hasn't been accepted yet,\" she says. \
                         \"Perhaps the waiting area?\"",
                        clan.name
                    ),
                    None => "She checks her files for word on your application...".to_string(),
                },
                dim,
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "You are not a member of any clan.",
                dim,
            )));
        }
    }

    // The two clan pickers share their flavor line.
    if matches!(state.mode(), Mode::ClanList | Mode::ClanApply) {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            match state.mode() {
                Mode::ClanApply => format!(
                    "{} pulls out a form with two lines: your name, and the clan's.",
                    data::CLAN_REGISTRAR
                ),
                _ => format!(
                    "{} points you to a marquee board near the entrance.",
                    data::CLAN_REGISTRAR
                ),
            },
            dim,
        )));
    }

    // The founding form: the fees and the lines already inked.
    if state.mode() == Mode::ClanFoundForm {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "\"Three things,\" says {}: \"a full name, a short banner, and the \
                 fees - {} gold and {} gems - to tailor your door's locks.\"",
                data::CLAN_REGISTRAR,
                model::CLAN_START_GOLD,
                model::CLAN_START_GEMS
            ),
            dim,
        )));
        if let Some(name) = state.clan_found_name() {
            lines.push(Line::from(Span::styled(
                format!("The name line reads: {name}"),
                dim,
            )));
        }
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                if state.clan_found_name().is_none() {
                    format!("clan name? {input}_")
                } else {
                    format!("banner letters? {input}_")
                },
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The hall: the boards, the counts, and the clan's tally
    // (clan_default.php's page body).
    if matches!(state.mode(), Mode::ClanHall | Mode::ClanMembership) {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        match state.clan_view() {
            None => lines.push(Line::from(Span::styled(
                "You work the secret levers and knobs of your hall's lock...",
                dim,
            ))),
            Some((clan, members)) => {
                lines.push(Line::from(Span::styled(
                    format!("The hall of {} <{}>.", clan.name, clan.tag),
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                )));
                if !clan.motd.trim().is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("MOTD (by {}): {}", clan.motd_author, clan.motd),
                        Style::default().fg(theme::TEXT()),
                    )));
                }
                if !clan.description.trim().is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("Charter (by {}): {}", clan.desc_author, clan.description),
                        dim,
                    )));
                }
                // Membership counts per rank, highest first (the hall's
                // "Membership Details" block).
                let mut counts: Vec<(u8, usize)> = Vec::new();
                for m in members {
                    match counts.iter_mut().find(|(r, _)| *r == m.rank) {
                        Some((_, n)) => *n += 1,
                        None => counts.push((m.rank, 1)),
                    }
                }
                counts.sort_by_key(|b| std::cmp::Reverse(b.0));
                let detail = counts
                    .iter()
                    .map(|(r, n)| format!("{}: {n}", model::clan_rank_name(*r)))
                    .collect::<Vec<_>>()
                    .join("   ");
                lines.push(Line::from(Span::styled(detail, dim)));
                let total_dks: u64 = members.iter().map(|m| m.dragon_kills as u64).sum();
                lines.push(Line::from(Span::styled(
                    format!(
                        "This clan counts {total_dks} dragon kill{} all told.",
                        if total_dks == 1 { "" } else { "s" }
                    ),
                    dim,
                )));
            }
        }
    }

    // One member on the desk (the ledger's operations page).
    if state.mode() == Mode::ClanMemberOps {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        match state.clan_member_target() {
            None => lines.push(Line::from(Span::styled("The page is blank.", dim))),
            Some(m) => {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{} - {} - level {}, {} dragon kill{}.",
                        m.name,
                        model::clan_rank_name(m.rank),
                        m.level,
                        m.dragon_kills,
                        if m.dragon_kills == 1 { "" } else { "s" }
                    ),
                    Style::default().fg(theme::TEXT()),
                )));
            }
        }
    }

    // The boards editor: what stands on them now, and the line being typed.
    if state.mode() == Mode::ClanEdit {
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::raw(""));
        if let Some((clan, _)) = state.clan_view() {
            lines.push(Line::from(Span::styled(
                format!(
                    "MOTD: {}",
                    if clan.motd.trim().is_empty() {
                        "(blank)"
                    } else {
                        &clan.motd
                    }
                ),
                dim,
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "Charter: {}",
                    if clan.description.trim().is_empty() {
                        "(blank)"
                    } else {
                        &clan.description
                    }
                ),
                dim,
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "Talk verb: {}",
                    if clan.custom_verb.trim().is_empty() {
                        "says"
                    } else {
                        &clan.custom_verb
                    }
                ),
                dim,
            )));
        }
        if let Some(input) = state.talk_line() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("new wording? {input}_"),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
        }
    }

    // The withdraw confirmation (clan_start.php's withdrawconfirm).
    if state.mode() == Mode::ClanWithdraw {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Are you sure you want to withdraw from your clan? A solitary \
             leader's mantle passes on - or, with no one left, the clan is \
             struck from the rolls.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
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
            "A new day stirs old memories. Whose blood runs in your veins? The choice holds for this life, and each people carries its own gift.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    if state.mode() == Mode::ChooseSpecialty {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Choose the craft you'll hone against the forest this life. You'll spend daily \"uses\" on its skills mid-fight.",
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
    let hint = if state.is_typing() {
        match state.mode() {
            Mode::WarriorList => "type a name   Enter ask around   Esc never mind",
            Mode::BountyTarget => "type a name   Enter check his book   Esc never mind",
            Mode::IntelTarget => "type a name   Enter ask him   Esc never mind",
            Mode::BountyAmount => "type an amount   Enter slide the coins over   Esc never mind",
            Mode::BankAmount(_) => "type an amount   Enter settle it   Esc never mind",
            Mode::BankTransferTarget => "type a name   Enter check the ledger   Esc never mind",
            Mode::BankTransferAmount => "type an amount   Enter send the note   Esc never mind",
            Mode::Haunt => "type a name   Enter whisper it   Esc never mind",
            _ => "type your line   Enter say it   Esc think better of it",
        }
    } else {
        controls_hint(state.mode())
    };
    lines.push(Line::from(Span::styled(
        hint,
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
        Mode::DragonApproach => "The Dragon's Cave",
        Mode::Fight => "Battle!",
        Mode::WeaponShop => "Ironroost Weapons",
        Mode::ArmorShop => "Duskmail Armoury",
        Mode::Healer => "The Mendery",
        Mode::Bank => "The Coinvault",
        Mode::BankAmount(_) => "The Teller's Counter",
        Mode::BankTransferTarget => "The Transfer Ledger",
        Mode::BankTransferAmount => "Writing the Note",
        Mode::Training => "The Proving Yard",
        Mode::Event => "A Forest Happening",
        Mode::ChooseStyle => "A Manner of Address",
        Mode::ChooseRace => "Remember Your Blood",
        Mode::ChooseSpecialty => "Choose Your Path",
        Mode::Graveyard => "The Graveyard",
        Mode::SpendDragonPoints => "Dragon Points",
        Mode::News => "The Daily News",
        Mode::Stables => "The Stables",
        Mode::MercCamp => "The Mercenary Camp",
        Mode::Inn => data::INN_NAME,
        Mode::InnRoom => "A Room for the Night",
        Mode::Barkeep => "The Barkeep's Counter",
        Mode::SwitchSpecialty => "A Quiet Word",
        Mode::Potions => "The Back Shelf",
        Mode::Drinks => "The Taps",
        Mode::Romance => "The Corner Table",
        Mode::Outhouse => "The Outhouse",
        Mode::OuthouseWash(_) => "The Rain Barrel",
        Mode::Tavern => "The Dark Horse Tavern",
        Mode::TavernBartender => "The Barman's Counter",
        Mode::IntelTarget => "Naming an Enemy",
        Mode::IntelSheet => "The Barman's Word",
        Mode::Commentary(room) => match room {
            CommentRoom::Village => "The Town Square",
            CommentRoom::Inn => "The Long Table",
            CommentRoom::DarkHorse => "The Table Etchings",
            CommentRoom::Gardens => "The Gardens",
            CommentRoom::Veterans => "The Veterans' Rock",
            CommentRoom::ShadeGypsy => "A Deep Trance",
            CommentRoom::ShadeGrave => "The Lost Souls",
            CommentRoom::Waiting => "The Waiting Area",
            CommentRoom::ClanHall(_) => "The Clan Hearth",
        },
        Mode::WarriorList => "The Warriors of the Realm",
        Mode::HallOfFame => "The Hall of Fame",
        Mode::BarkeepEar => "A Quiet Word",
        Mode::PvpList(PvpVenue::Fields) => "The Sleeping Fields",
        Mode::PvpList(PvpVenue::Inn) => "The Rooms Upstairs",
        Mode::DagTable => "The Shadowed Booth",
        Mode::BountyList => "The Wanted List",
        Mode::BountyTarget => "Naming a Head",
        Mode::BountyAmount => "Naming a Price",
        Mode::Haunt => "Across the Veil",
        Mode::ClanLobby => "The Clan Halls",
        Mode::ClanList => "The Marquee Board",
        Mode::ClanDetail => "A Clan's Roll",
        Mode::ClanApply => "A Membership Form",
        Mode::ClanFoundForm => "A New Clan's Filing",
        Mode::ClanHall => "Your Clan Hall",
        Mode::ClanMembership => "The Clan Ledger",
        Mode::ClanMemberOps => "A Word About a Member",
        Mode::ClanEdit => "The Hall's Boards",
        Mode::ClanWithdraw => "Leaving the Clan",
    }
}

fn controls_hint(mode: Mode) -> &'static str {
    match mode {
        Mode::Fight => "up/down select   Enter act   Esc try to flee",
        Mode::BarkeepEar => "up/down move   Enter choose   Esc back to the inn",
        Mode::PvpList(PvpVenue::Inn) => "up/down move   Enter attack   Esc back to the barkeep",
        Mode::PvpList(PvpVenue::Fields) => "up/down move   Enter attack   Esc back to village",
        Mode::Village | Mode::Graveyard => "up/down move   Enter choose   Esc leave the game",
        Mode::SpendDragonPoints => "up/down move   Enter spend   Esc leave the game",
        Mode::ChooseStyle | Mode::ChooseRace => "up/down move   Enter choose   Esc leave the game",
        Mode::InnRoom
        | Mode::Barkeep
        | Mode::SwitchSpecialty
        | Mode::Potions
        | Mode::Drinks
        | Mode::Romance => "up/down move   Enter choose   Esc back to the inn",
        Mode::Outhouse | Mode::Tavern => "up/down move   Enter choose   Esc back to the forest",
        Mode::TavernBartender => "up/down move   Enter choose   Esc back to the taproom",
        Mode::IntelTarget | Mode::IntelSheet => {
            "up/down move   Enter choose   Esc back to the counter"
        }
        Mode::OuthouseWash(_) => "up/down move   Enter choose   Esc slips out unwashed",
        Mode::Commentary(_) => "up/down move   Enter choose   Esc step away",
        Mode::DagTable => "up/down move   Enter choose   Esc back to the inn",
        Mode::BountyList | Mode::BountyTarget | Mode::BountyAmount => {
            "up/down move   Enter choose   Esc back to the booth"
        }
        Mode::BankTransferTarget | Mode::BankTransferAmount => {
            "up/down move   Enter choose   Esc back to the counter"
        }
        Mode::Haunt => "up/down move   Enter choose   Esc back to the graves",
        Mode::ClanList | Mode::ClanApply | Mode::ClanFoundForm => {
            "up/down move   Enter choose   Esc back to the lobby"
        }
        Mode::ClanMembership | Mode::ClanEdit | Mode::ClanWithdraw => {
            "up/down move   Enter choose   Esc back to the hall"
        }
        Mode::ClanMemberOps => "up/down move   Enter choose   Esc back to the ledger",
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
