//! Bounded iCalendar exchange. Imported events become new, permission-checked drafts.
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use chrono_tz::Tz;
use icalendar::parser::{self, Component, Property};
use late_core::models::calendar::{
    CalendarEvent, EventDraft, EventTiming, Occurrence, local_instant,
};

pub(super) const MAX_BYTES: usize = 1024 * 1024;
const MAX_EVENTS: usize = 1000;

#[derive(Debug)]
pub struct Candidate {
    pub title: String,
    pub draft: Result<EventDraft, String>,
}

pub(super) fn import_url(input: &str) -> Result<Option<String>> {
    let input = input.trim().trim_start_matches('\u{feff}');
    if input.starts_with("BEGIN:") || input.contains('\n') {
        return Ok(None);
    }
    let normalized = if input
        .get(..9)
        .is_some_and(|s| s.eq_ignore_ascii_case("webcal://"))
    {
        format!("https://{}", &input[9..])
    } else {
        input.to_owned()
    };
    let url = reqwest::Url::parse(&normalized)
        .context("Paste iCalendar content or an http, https or webcal URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "Use an http, https or webcal URL"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URLs with embedded credentials are not supported"
    );
    Ok(Some(url.to_string()))
}

pub(super) fn parse(input: &str, tz: Tz) -> Result<Vec<Candidate>> {
    ensure!(input.len() <= MAX_BYTES, "iCalendar is limited to 1 MiB");
    let input = input.trim().trim_start_matches('\u{feff}');
    let input = parser::unfold(input);
    let mut depth = 0usize;
    for line in input.lines() {
        if line
            .get(..6)
            .is_some_and(|s| s.eq_ignore_ascii_case("BEGIN:"))
        {
            depth += 1;
            ensure!(depth <= 16, "iCalendar components are nested too deeply");
        } else if line
            .get(..4)
            .is_some_and(|s| s.eq_ignore_ascii_case("END:"))
        {
            depth = depth.saturating_sub(1);
        }
    }
    let roots = parser::read_components(&input)
        .map_err(|_| anyhow::anyhow!("Invalid iCalendar content"))?;
    let mut events = Vec::new();
    for root in roots {
        let components = if root.name.as_str().eq_ignore_ascii_case("VCALENDAR") {
            root.components
        } else {
            vec![root]
        };
        for event in components
            .into_iter()
            .filter(|c| c.name.as_str().eq_ignore_ascii_case("VEVENT"))
        {
            ensure!(
                events.len() < MAX_EVENTS,
                "iCalendar is limited to 1000 events"
            );
            let title = property(&event.properties, "SUMMARY")
                .map(|p| p.val.as_str())
                .and_then(|s| text(s).ok())
                .unwrap_or_else(|| "Untitled event".into())
                .replace(['\n', '\t'], " ");
            let draft = event_draft(&event, tz).map_err(|e| e.to_string());
            events.push(Candidate { title, draft });
        }
    }
    ensure!(
        !events.is_empty(),
        "No VEVENT events found in this iCalendar"
    );
    Ok(events)
}

fn property<'a, 'b>(props: &'a [Property<'b>], name: &str) -> Option<&'a Property<'b>> {
    props
        .iter()
        .find(|p| p.name.as_str().eq_ignore_ascii_case(name))
}

fn parameter<'a>(p: &'a Property<'_>, name: &str) -> Option<&'a str> {
    p.params
        .iter()
        .find(|p| p.key.as_str().eq_ignore_ascii_case(name))?
        .val
        .as_ref()
        .map(|s| s.as_str())
}

fn value<'a>(p: &'a Property<'_>) -> &'a str {
    p.val.as_str()
}

fn text(input: &str) -> Result<String> {
    // The component parser unfolds and unescapes TEXT properties already.
    ensure!(
        !input
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t')),
        "iCalendar text contains control characters"
    );
    Ok(input.to_owned())
}

enum When {
    Date(NaiveDate),
    Instant(DateTime<Utc>),
}

fn when(p: &Property, tz: Tz) -> Result<When> {
    let v = value(p);
    if parameter(p, "VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE")) {
        return Ok(When::Date(
            NaiveDate::parse_from_str(v, "%Y%m%d")
                .with_context(|| format!("Invalid {} date", p.name))?,
        ));
    }
    ensure!(
        parameter(p, "VALUE").is_none_or(|v| v.eq_ignore_ascii_case("DATE-TIME")),
        "{} must be DATE or DATE-TIME",
        p.name
    );
    let utc = v.ends_with('Z');
    let date = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
        .with_context(|| format!("Invalid {} date/time", p.name))?;
    if utc {
        ensure!(
            parameter(p, "TZID").is_none(),
            "UTC times cannot also have TZID"
        );
        Ok(When::Instant(date.and_utc()))
    } else {
        let zone = parameter(p, "TZID")
            .map(|name| {
                name.parse::<Tz>().with_context(|| {
                    format!("Unsupported timezone: {name}; use an IANA timezone or UTC")
                })
            })
            .transpose()?
            .unwrap_or(tz);
        // RFC 5545 specifies the first occurrence of a repeated wall time.
        Ok(When::Instant(local_instant(
            date,
            zone,
            Some(Occurrence::Earlier),
        )?))
    }
}

#[derive(Clone, Copy)]
struct IcalDuration {
    days: i64,
    seconds: i64,
}

impl IcalDuration {
    fn total(self) -> Result<Duration> {
        let seconds = self
            .days
            .checked_mul(86400)
            .and_then(|n| n.checked_add(self.seconds))
            .context("Duration out of range")?;
        Duration::try_seconds(seconds).context("Duration out of range")
    }
}

fn duration(input: &str) -> Result<IcalDuration> {
    let mut chars = input.strip_prefix('+').unwrap_or(input).chars();
    ensure!(chars.next() == Some('P'), "Invalid iCalendar duration");
    let mut time = false;
    let mut digits = String::new();
    let mut total = 0i64;
    let mut days = 0i64;
    let mut last = 0;
    let mut week = false;
    for c in chars {
        if c == 'T' {
            ensure!(
                !time && digits.is_empty() && !week,
                "Invalid iCalendar duration"
            );
            time = true;
            continue;
        }
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        let (order, multiplier) = match (c, time) {
            ('W', false) => {
                week = true;
                (1, 604800)
            }
            ('D', false) => (2, 86400),
            ('H', true) => (3, 3600),
            ('M', true) => (4, 60),
            ('S', true) => (5, 1),
            _ => bail!("Invalid iCalendar duration"),
        };
        ensure!(
            order > last && (!week || last == 0),
            "Invalid iCalendar duration"
        );
        let n = digits
            .parse::<i64>()
            .context("Invalid iCalendar duration")?;
        digits.clear();
        let amount = n.checked_mul(multiplier).context("Duration out of range")?;
        if order <= 2 {
            days = days
                .checked_add(amount / 86400)
                .context("Duration out of range")?;
        } else {
            total = total.checked_add(amount).context("Duration out of range")?;
        }
        last = order;
    }
    ensure!(
        digits.is_empty() && last > 0 && !(time && last <= 2),
        "Invalid iCalendar duration"
    );
    // Preserve nominal days: a day across a DST transition need not be 24 hours.
    let d = IcalDuration {
        days,
        seconds: total,
    };
    d.total()?;
    Ok(d)
}

fn event_draft(event: &Component<'_>, tz: Tz) -> Result<EventDraft> {
    let props = &event.properties;
    for name in ["DTSTART", "DTEND", "DURATION", "SUMMARY", "DESCRIPTION"] {
        ensure!(
            props
                .iter()
                .filter(|p| p.name.as_str().eq_ignore_ascii_case(name))
                .count()
                <= 1,
            "Event has more than one {name}"
        );
    }
    ensure!(
        !["RRULE", "RDATE", "EXRULE", "EXDATE"]
            .iter()
            .any(|n| property(props, n).is_some()),
        "Recurring series are not supported; paste a single occurrence instead"
    );
    ensure!(
        !property(props, "STATUS")
            .map(|p| p.val.as_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("CANCELLED")),
        "This event is cancelled"
    );
    let start_property = property(props, "DTSTART").context("Event is missing DTSTART")?;
    let end = property(props, "DTEND");
    let length = property(props, "DURATION");
    ensure!(
        end.is_none() || length.is_none(),
        "Use DTEND or DURATION, not both"
    );
    let length = length.map(|p| duration(value(p))).transpose()?;
    if let Some(d) = length {
        ensure!(
            d.total()? > Duration::zero(),
            "Event duration must be positive"
        );
    }
    let timing = match when(start_property, tz)? {
        When::Date(start) => {
            let end_exclusive = if let Some(end) = end {
                let When::Date(end) = when(end, tz)? else {
                    bail!("All-day DTEND must be a DATE");
                };
                end
            } else if let Some(d) = length {
                ensure!(
                    d.seconds == 0,
                    "All-day duration must be whole days or weeks"
                );
                start
                    .checked_add_signed(d.total()?)
                    .context("End date out of range")?
            } else {
                start.succ_opt().context("End date out of range")?
            };
            EventTiming::AllDay {
                start,
                end_exclusive,
            }
        }
        When::Instant(start) => {
            let end = if let Some(end) = end {
                let When::Instant(end) = when(end, tz)? else {
                    bail!("Timed DTEND must be a DATE-TIME");
                };
                Some(end)
            } else {
                length
                    .map(|d| {
                        let mut end = start;
                        if d.days != 0 {
                            let zone = if value(start_property).ends_with('Z') {
                                chrono_tz::UTC
                            } else {
                                parameter(start_property, "TZID")
                                    .map(str::parse::<Tz>)
                                    .transpose()?
                                    .unwrap_or(tz)
                            };
                            let days =
                                Duration::try_days(d.days).context("Duration out of range")?;
                            let local = start
                                .with_timezone(&zone)
                                .naive_local()
                                .checked_add_signed(days)
                                .context("End time out of range")?;
                            end = local_instant(local, zone, Some(Occurrence::Earlier))?;
                        }
                        end.checked_add_signed(
                            Duration::try_seconds(d.seconds).context("Duration out of range")?,
                        )
                        .context("End time out of range")
                    })
                    .transpose()?
            };
            EventTiming::Timed { start, end }
        }
    };
    let title = property(props, "SUMMARY")
        .map(|p| text(value(p)))
        .transpose()?
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Untitled event".into())
        .replace(['\n', '\t'], " ");
    let mut description = property(props, "DESCRIPTION")
        .map(|p| text(value(p)))
        .transpose()?
        .unwrap_or_default();
    for (name, label) in [("LOCATION", "Location"), ("URL", "URL")] {
        if let Some(p) = property(props, name) {
            let v = if name == "URL" {
                value(p).to_owned()
            } else {
                text(value(p))?
            };
            ensure!(
                !v.chars().any(|c| c.is_control() && c != '\n' && c != '\t'),
                "Invalid {name} text"
            );
            if !v.is_empty() {
                if !description.is_empty() {
                    description.push_str("\n\n");
                }
                description.push_str(&format!("{label}: {v}"));
            }
        }
    }
    let notice_lead_seconds = event
        .components
        .iter()
        .filter(|c| c.name.as_str().eq_ignore_ascii_case("VALARM"))
        .find_map(|alarm| {
            let action = value(property(&alarm.properties, "ACTION")?);
            let trigger = property(&alarm.properties, "TRIGGER")?;
            if !action.eq_ignore_ascii_case("DISPLAY")
                || parameter(trigger, "RELATED").is_some_and(|s| !s.eq_ignore_ascii_case("START"))
                || parameter(trigger, "VALUE").is_some_and(|s| !s.eq_ignore_ascii_case("DURATION"))
                || property(&alarm.properties, "REPEAT").is_some()
            {
                return None;
            }
            let v = value(trigger);
            let lead = duration(v.strip_prefix('-')?)
                .ok()?
                .total()
                .ok()?
                .num_seconds();
            (lead <= 315360000).then_some(lead)
        });
    let draft = EventDraft {
        title,
        description,
        timing,
        notice_lead_seconds,
        mod_editable: false,
    };
    draft.validate()?;
    Ok(draft)
}

fn escaped(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
}

fn line(output: &mut String, value: &str) {
    let mut remaining = value;
    let mut limit = 75;
    while remaining.len() > limit {
        let mut end = limit;
        while !remaining.is_char_boundary(end) {
            end -= 1;
        }
        output.push_str(&remaining[..end]);
        output.push_str("\r\n ");
        remaining = &remaining[end..];
        limit = 74; // The continuation space counts toward the 75-octet limit.
    }
    output.push_str(remaining);
    output.push_str("\r\n");
}

pub(super) fn export(event: &CalendarEvent) -> String {
    let mut output = String::new();
    for value in [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//late.sh//Calendars//EN",
        "CALSCALE:GREGORIAN",
        "BEGIN:VEVENT",
    ] {
        line(&mut output, value);
    }
    line(&mut output, &format!("UID:{}@late.sh", event.id));
    line(
        &mut output,
        &format!("DTSTAMP:{}", Utc::now().format("%Y%m%dT%H%M%SZ")),
    );
    match &event.timing {
        EventTiming::AllDay {
            start,
            end_exclusive,
        } => {
            line(
                &mut output,
                &format!("DTSTART;VALUE=DATE:{}", start.format("%Y%m%d")),
            );
            line(
                &mut output,
                &format!("DTEND;VALUE=DATE:{}", end_exclusive.format("%Y%m%d")),
            );
        }
        EventTiming::Timed { start, end } => {
            line(
                &mut output,
                &format!("DTSTART:{}", start.format("%Y%m%dT%H%M%SZ")),
            );
            let end = end.unwrap_or_else(|| *start + Duration::hours(1));
            line(
                &mut output,
                &format!("DTEND:{}", end.format("%Y%m%dT%H%M%SZ")),
            );
        }
    }
    line(&mut output, &format!("SUMMARY:{}", escaped(&event.title)));
    if !event.description.is_empty() {
        line(
            &mut output,
            &format!("DESCRIPTION:{}", escaped(&event.description)),
        );
    }
    line(
        &mut output,
        &format!("SEQUENCE:{}", event.revision.saturating_sub(1).max(0)),
    );
    if let Some(lead) = event.notice_lead_seconds {
        for value in [
            "BEGIN:VALARM".to_owned(),
            "ACTION:DISPLAY".into(),
            format!("TRIGGER:-PT{lead}S"),
            format!("DESCRIPTION:{}", escaped(&event.title)),
            "END:VALARM".into(),
        ] {
            line(&mut output, &value);
        }
    }
    line(&mut output, "END:VEVENT");
    line(&mut output, "END:VCALENDAR");
    output
}
