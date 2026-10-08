//! Builds the customizable status bar and reports where each segment landed.
//!
//! Three passes, and no component ever knows its own x:
//!
//! 1. [`build_segments`] turns a component list plus this frame's [`StatusData`]
//!    into [`Segment`]s. Disabled components, and auto-hiding components with
//!    nothing to say, produce nothing. The top bar supplies a fixed list (pot
//!    and chips); the bottom bar supplies the user's persisted list.
//! 2. [`fit`] keeps the segments that fit beside the other title sharing the
//!    border row, in list order, and drops the rest whole. Nothing is ever
//!    shortened: what a user sees is exactly what they configured, or nothing.
//! 3. [`lay_out`] joins the survivors with dividers, measures, and converts
//!    accumulated widths into click rects.
//!
//! Splitting it this way is what lets the bar be reordered and resized freely:
//! widths are measured with ratatui's own [`Span::width`], the same function
//! that decides which cells a span occupies when it paints, so a rect can
//! never disagree with what the user sees.

use late_core::models::statusline::{LabelMode, StatusComponent, StatusComponentSetting};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::data::{StatusData, clock_icon};
use crate::app::common::theme;

/// Which row the bar is painted on, and therefore which end of it collides
/// with the other title on that row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Placement {
    /// Right-aligned on the top border, sharing the row with the page tabs on
    /// the left. The corner end has first claim on the room.
    TopRight,
    /// Left-aligned on the bottom border, sharing the row with the sponsor
    /// line on the right. The corner end has first claim on the room.
    BottomLeft,
    /// Left-aligned on Zen's own bottom row, which it has to itself. Zen
    /// has no frame, so there is no border to run on: no corners, and the
    /// dividers are dots.
    ZenRow,
}

impl Placement {
    /// Segment indices in the order they claim room: from the frame corner
    /// the bar is anchored to, inward toward the other title.
    fn claim_order(self, len: usize) -> Vec<usize> {
        match self {
            Self::TopRight => (0..len).rev().collect(),
            Self::BottomLeft | Self::ZenRow => (0..len).collect(),
        }
    }

    /// Cells of `area` that are not frame corners.
    fn usable_cols(self, area: Rect) -> u16 {
        match self {
            Self::TopRight | Self::BottomLeft => area.width.saturating_sub(2),
            Self::ZenRow => area.width,
        }
    }
}

/// One component's contribution to this frame's bar.
///
/// Carries its own spans and nothing about position: turning these into
/// screen rects is [`lay_out`]'s job, which is why reordering or resizing the
/// bar cannot leave a stale hit target behind.
#[derive(Clone, Debug)]
pub(crate) struct Segment {
    component: StatusComponent,
    spans: Vec<Span<'static>>,
}

impl Segment {
    fn width(&self) -> u16 {
        span_width(&self.spans)
    }
}

fn span_width(spans: &[Span<'static>]) -> u16 {
    spans.iter().map(Span::width).sum::<usize>() as u16
}

/// The finished bar: what to paint, and where each clickable segment landed.
pub(crate) struct StatusBar {
    pub line: Line<'static>,
    /// Every component that survived the fit, in paint order.
    pub painted: Vec<StatusComponent>,
    /// Screen rects for the segments a click can act on, in paint order,
    /// with what the click does. Components with no click action are absent.
    pub hits: Vec<(StatusClick, Rect)>,
}

/// The top-right bar is fixed UI policy, not part of the user's saved
/// arrangement: the pot and the chip balance, the two ambient readings that
/// belong in a corner that never moves. Expressing it as component settings
/// lets both bars share formatting, fitting, and hit-testing. The chips sit
/// in the corner, so they claim room before the pot does.
pub(crate) fn fixed_topbar_components() -> [StatusComponentSetting; 2] {
    [StatusComponent::Pot, StatusComponent::Chips].map(|component| StatusComponentSetting {
        enabled: true,
        ..StatusComponentSetting::new(component)
    })
}

/// Build the fixed top-right bar. A component the bottom bar painted this
/// frame is skipped, so placing the pot or the chips on the bottom moves the
/// reading there instead of showing it twice. Painted, not merely enabled: a
/// segment the bottom bar had to drop stays up here.
pub(crate) fn build_top_status_bar(
    data: &StatusData<'_>,
    area: Rect,
    title_width: u16,
    painted_on_bottom: &[StatusComponent],
) -> Option<StatusBar> {
    let components: Vec<StatusComponentSetting> = fixed_topbar_components()
        .into_iter()
        .filter(|setting| !painted_on_bottom.contains(&setting.component))
        .collect();
    build_status_bar(&components, data, Placement::TopRight, area, title_width)
}

/// Zen's bottom row for one frame: the user's bar on the left, the page's
/// own keys on the right.
pub(crate) struct ZenStatusRow {
    /// `None` while every enabled component is auto-hidden, or all are off:
    /// the bar's part of the row stays blank.
    pub bar: Option<Line<'static>>,
    /// `None` on a row too narrow to hold them beside a cell of gap.
    pub keys: Option<Line<'static>>,
    pub hits: Vec<(StatusClick, Rect)>,
}

/// The layout keys worth knowing by heart, on the right end of Zen's row:
/// the guide, split, flip, close, and zoom. The rest are in the guide.
pub(crate) const ZEN_ROW_KEYS: [(&str, &str); 5] = [
    ("?", "help"),
    ("S", "split"),
    ("F", "flip"),
    ("X", "close"),
    ("Z", "zoom"),
];

/// The keys as one compact line: each key amber, its word dim, two spaces
/// between pairs and nothing around them.
pub(crate) fn zen_keys_line() -> Line<'static> {
    let key = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let word = Style::default().fg(theme::TEXT_DIM());
    let mut spans = Vec::with_capacity(ZEN_ROW_KEYS.len() * 2);
    for (index, (k, w)) in ZEN_ROW_KEYS.iter().enumerate() {
        let gap = if index == 0 { "" } else { "  " };
        spans.push(Span::styled(format!("{gap}{k}"), key));
        spans.push(Span::styled(format!(" {w}"), word));
    }
    Line::from(spans)
}

/// Build Zen's bottom row. The keys claim the right end first, a cell of gap
/// before them, and the bar fits into what is left; a row narrower than the
/// keys gives the bar all of it.
pub(crate) fn build_zen_status_row(
    components: &[StatusComponentSetting],
    data: &StatusData<'_>,
    row: Rect,
) -> ZenStatusRow {
    let keys = zen_keys_line();
    let reserved = keys.width() as u16 + 1;
    let (keys, reserved) = match reserved <= row.width {
        true => (Some(keys), reserved),
        false => (None, 0),
    };
    match build_status_bar(components, data, Placement::ZenRow, row, reserved) {
        Some(bar) => ZenStatusRow {
            bar: Some(bar.line),
            keys,
            hits: bar.hits,
        },
        None => ZenStatusRow {
            bar: None,
            keys,
            hits: Vec::new(),
        },
    }
}

/// The keyboard hint was the original bottom-left frame title. It stays its
/// own styled component rather than flattening into the generic value/label
/// treatment so its key names keep their emphasis. It has one rendering per
/// setting: caret notation, or the brief glyph form.
fn shortcut_spans(brief: bool) -> Vec<Span<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let sep_style = Style::default().fg(theme::TEXT_FAINT());
    let (separator, hints): (&str, &[(&str, &str)]) = match brief {
        false => (
            "  ",
            &[
                ("Settings", "^O"),
                ("Lobby", "^G"),
                ("Zen", "^F"),
                ("Shop", "^S"),
                ("Guide", "?"),
                ("Exit", "qq"),
            ],
        ),
        true => (" · ", &[("⚙", "^o"), ("⚄", "^g"), ("◉", "^s")]),
    };

    let mut spans = Vec::new();
    for (idx, &(label, key_text)) in hints.iter().enumerate() {
        if idx == 0 {
            spans.push(Span::styled(" ", dim));
        } else {
            spans.push(Span::styled(separator, sep_style));
        }
        spans.push(Span::styled(format!("{label} "), dim));
        spans.push(Span::styled(key_text, key));
    }
    spans.push(Span::styled(" ", dim));
    spans
}

/// Build the bar for one frame.
///
/// `area` is the full bordered frame (corners included), or Zen's row itself
/// for `ZenRow`, and `title_width` is the width of the other title sharing
/// the row. Both arrive raw rather than pre-subtracted so the fitting math is
/// covered by tests instead of living uncovered at the call site.
pub(crate) fn build_status_bar(
    components: &[StatusComponentSetting],
    data: &StatusData<'_>,
    placement: Placement,
    area: Rect,
    title_width: u16,
) -> Option<StatusBar> {
    let spare_cols = placement.usable_cols(area).saturating_sub(title_width);
    let segments = fit(build_segments(components, data), spare_cols, placement);
    lay_out(segments, placement, area)
}

/// Turn a component list into this frame's segments, in paint order.
pub(crate) fn build_segments(
    components: &[StatusComponentSetting],
    data: &StatusData<'_>,
) -> Vec<Segment> {
    components
        .iter()
        .filter(|setting| setting.enabled)
        .filter_map(|setting| build_segment(setting, data))
        .collect()
}

fn build_segment(setting: &StatusComponentSetting, data: &StatusData<'_>) -> Option<Segment> {
    let component = setting.component;
    if component == StatusComponent::Shortcuts {
        return Some(Segment {
            component,
            spans: shortcut_spans(setting.brief),
        });
    }
    let value = match data.value(component, setting.variant) {
        Some(value) => value,
        // Inactive: hide the segment, or show the resting reading.
        None if setting.auto_hide => return None,
        None => resting_value(component),
    };

    Some(Segment {
        component,
        spans: segment_spans(setting, data, &value),
    })
}

/// What an inactive component paints when the user has turned auto-hide off:
/// the zero it is counting, or a resting marker for the ones that have no
/// count at all.
fn resting_value(component: StatusComponent) -> String {
    match component {
        StatusComponent::Voice | StatusComponent::Live => "-".to_string(),
        StatusComponent::Pot => "closed".to_string(),
        StatusComponent::Mentions
        | StatusComponent::Turns
        | StatusComponent::Quests
        | StatusComponent::Care => "0".to_string(),
        // `StatusComponent::can_auto_hide` is false for these because they
        // always have a reading, and Keyhints never reaches the value path.
        StatusComponent::Shortcuts
        | StatusComponent::Time
        | StatusComponent::Date
        | StatusComponent::Chips
        | StatusComponent::Users
        | StatusComponent::Station => {
            unreachable!("{} always has a reading", component.as_str())
        }
    }
}

/// Assemble one segment: the value always paints, and `LabelMode` picks what
/// sits before it. Text labels lead values (`unread 3`), as do icons (`📩 3`).
/// Leading icons also keep the glyph off the segment's right edge, where a
/// terminal painting it a cell narrower than
/// measured would drag the following divider with it.
fn segment_spans(
    setting: &StatusComponentSetting,
    data: &StatusData<'_>,
    value: &str,
) -> Vec<Span<'static>> {
    let component = setting.component;
    let value_style = Style::default()
        .fg(accent(component))
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(theme::TEXT_MUTED());

    let lead = match setting.label {
        LabelMode::Text => component.text_label(),
        // The clock's face follows the hour, so it has no static icon.
        LabelMode::Icon if component == StatusComponent::Time => clock_icon(data.hour),
        LabelMode::Icon => component.icon(),
        LabelMode::None => "",
    };
    // A component with no word to add (the clock under `Text`) paints the
    // bare value rather than a stray space.
    if lead.is_empty() {
        return vec![Span::styled(format!(" {value} "), value_style)];
    }
    vec![
        Span::styled(format!(" {lead} "), label_style),
        Span::styled(format!("{value} "), value_style),
    ]
}

fn accent(component: StatusComponent) -> ratatui::style::Color {
    match component {
        StatusComponent::Shortcuts => theme::TEXT_DIM(),
        StatusComponent::Mentions => theme::MENTION(),
        StatusComponent::Chips | StatusComponent::Pot => theme::AMBER(),
        StatusComponent::Voice => theme::SUCCESS(),
        StatusComponent::Turns
        | StatusComponent::Quests
        | StatusComponent::Care
        | StatusComponent::Live => theme::AMBER_GLOW(),
        StatusComponent::Users | StatusComponent::Station | StatusComponent::Date => theme::TEXT(),
        StatusComponent::Time => theme::TEXT_BRIGHT(),
    }
}

/// Keep the segments that fit in `spare_cols` and drop the rest.
///
/// Segments claim room in `Placement::claim_order`, each with the one
/// separator it brings. A segment with no room left for it is dropped whole;
/// a narrower one after it may still fit. Nothing is shortened and nothing
/// outranks the list order, so getting a bar that fits a small terminal is up
/// to the arrangement the user saved.
pub(crate) fn fit(segments: Vec<Segment>, spare_cols: u16, placement: Placement) -> Vec<Segment> {
    let mut used: u16 = 0;
    let mut kept = vec![false; segments.len()];
    for idx in placement.claim_order(segments.len()) {
        let cost = segments[idx].width().saturating_add(1);
        if used.saturating_add(cost) <= spare_cols {
            used = used.saturating_add(cost);
            kept[idx] = true;
        }
    }
    segments
        .into_iter()
        .zip(kept)
        .filter_map(|(segment, kept)| kept.then_some(segment))
        .collect()
}

/// The frame bars are painted *over* the frame's border row, so their
/// separators are border glyphs rather than pipes: the line reads as the
/// border running on through the gaps between components.
///
/// ```text
/// ───── 3 ─ 12:04 ─ 1204 ─┐
/// ```
///
/// Zen's row has no border to continue, so it is dotted like the page's
/// other hint rows.
fn separator(placement: Placement) -> Span<'static> {
    match placement {
        Placement::TopRight | Placement::BottomLeft => {
            Span::styled("─", Style::default().fg(theme::BORDER_ACTIVE()))
        }
        Placement::ZenRow => Span::styled("·", Style::default().fg(theme::TEXT_FAINT())),
    }
}

/// Painted width of the whole bar: every segment and a separator between
/// each adjacent pair. A frame bar adds one more at the corner end so it
/// meets the frame corner through a border glyph instead of a blank cell.
/// `fit` budgets that corner glyph on every placement, so Zen's row keeps
/// the cell as air at its right end.
fn total_width(segments: &[Segment]) -> u16 {
    let segments_width: u16 = segments.iter().map(Segment::width).sum();
    segments_width + (segments.len() as u16)
}

/// Join the survivors, then walk the joined line converting accumulated widths
/// into screen rects.
///
/// Dividers belong to this pass, not to the builders: a builder that emits its
/// own divider has to know whether a neighbour survived the fit, which is
/// exactly the coupling that makes reorderable segments awkward.
pub(crate) fn lay_out(
    segments: Vec<Segment>,
    placement: Placement,
    area: Rect,
) -> Option<StatusBar> {
    if segments.is_empty() {
        return None;
    }

    let total = total_width(&segments);
    // Both titles sit inside the frame corners.
    let start_x = match placement {
        Placement::TopRight => area.right().saturating_sub(total).saturating_sub(1),
        Placement::BottomLeft => area.x.saturating_add(1),
        Placement::ZenRow => area.x,
    };
    let y = match placement {
        Placement::TopRight | Placement::ZenRow => area.y,
        Placement::BottomLeft => area.bottom().saturating_sub(1),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut painted = Vec::new();
    let mut hits = Vec::new();
    let mut cursor = start_x;
    if placement == Placement::BottomLeft {
        // The bar starts at the corner, so its own edge glyph leads.
        spans.push(separator(placement));
        cursor = cursor.saturating_add(1);
    }
    for (idx, segment) in segments.into_iter().enumerate() {
        if idx > 0 {
            spans.push(separator(placement));
            cursor = cursor.saturating_add(1);
        }
        let width = segment.width();
        let component = segment.component;
        painted.push(component);
        if let Some(action) = click_action(component) {
            hits.push((
                action,
                Rect {
                    x: cursor,
                    y,
                    width,
                    height: 1,
                },
            ));
        }
        cursor = cursor.saturating_add(width);
        spans.extend(segment.spans);
    }
    if placement == Placement::TopRight {
        // The bar ends at the corner, so its edge glyph trails.
        spans.push(separator(placement));
    }

    let line = match placement {
        Placement::TopRight => Line::from(spans).right_aligned(),
        Placement::BottomLeft | Placement::ZenRow => Line::from(spans).left_aligned(),
    };
    Some(StatusBar {
        line,
        painted,
        hits,
    })
}

/// What clicking a segment does. Components absent from this list paint but
/// do not respond, and never get a hit rect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StatusClick {
    /// Home, with the notifications feed selected.
    Mentions,
    Shop,
    Lobby,
    Booth,
    Arcade,
    Profiles,
    /// Zen, where the bonsai, the tank, and the pet all live.
    Zen,
    /// What the live strip shows, as `o` on the #lounge card opens it.
    Live,
}

pub(crate) fn click_action(component: StatusComponent) -> Option<StatusClick> {
    match component {
        StatusComponent::Shortcuts => None,
        StatusComponent::Mentions => Some(StatusClick::Mentions),
        StatusComponent::Chips => Some(StatusClick::Shop),
        StatusComponent::Turns => Some(StatusClick::Lobby),
        StatusComponent::Care => Some(StatusClick::Zen),
        StatusComponent::Station => Some(StatusClick::Booth),
        StatusComponent::Quests => Some(StatusClick::Arcade),
        StatusComponent::Users => Some(StatusClick::Profiles),
        StatusComponent::Live => Some(StatusClick::Live),
        // The clock, the date, the pot and the mic badge are readouts: there
        // is no screen a click on them obviously means.
        StatusComponent::Time
        | StatusComponent::Date
        | StatusComponent::Voice
        | StatusComponent::Pot => None,
    }
}
