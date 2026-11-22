//! Stored events, editor validation and updates to the day buckets.
use crate::CalendarDraft;
use chrono::{Datelike, NaiveDate, NaiveTime};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub(super) twelve_hour: bool,
    pub(super) time_zone: Option<Tz>,
    events: BTreeMap<String, Vec<Event>>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Event {
    #[serde(default)]
    pub(super) end_date: Option<String>,
    #[serde(default)]
    pub(super) color: u8,
    pub(super) title: String,
    pub(super) start: String,
    pub(super) end: String,
    pub(super) location: String,
    pub(super) description: String,
}

impl Event {
    pub(super) fn last_date(&self, start: NaiveDate) -> NaiveDate {
        let end = self
            .end_date
            .as_deref()
            .and_then(parse_date)
            .unwrap_or(start);
        if end > start && self.end == "00:00" {
            end.pred_opt().unwrap_or(start)
        } else {
            end
        }
    }

    pub(super) fn draft(&self, date: NaiveDate, twelve_hour: bool) -> CalendarDraft {
        CalendarDraft {
            date: date.to_string().into(),
            end_date: self
                .end_date
                .clone()
                .unwrap_or_else(|| date.to_string())
                .into(),
            color: i32::from(self.color),
            title: self.title.clone().into(),
            start: display_time(&self.start, twelve_hour).into(),
            end: display_time(&self.end, twelve_hour).into(),
            location: self.location.clone().into(),
            description: self.description.clone().into(),
        }
    }

    pub(super) fn validate(draft: CalendarDraft) -> Result<(NaiveDate, Self), &'static str> {
        let date = parse_date(draft.date.trim()).ok_or("Enter a start date as YYYY-MM-DD.")?;
        let end_date = if draft.end_date.trim().is_empty() {
            date
        } else {
            parse_date(draft.end_date.trim()).ok_or("Enter an end date as YYYY-MM-DD.")?
        };
        let title = draft.title.trim();
        if title.is_empty() {
            return Err("Enter an event title.");
        }
        let start = parse_time(draft.start.trim())?;
        let end = parse_time(draft.end.trim())?;
        if end_date < date
            || (end_date == date && matches!((start, end), (Some(a), Some(b)) if b <= a))
        {
            return Err("End must be after start.");
        }
        let normalized = |time: Option<NaiveTime>| {
            time.map(|time| time.format("%H:%M").to_string())
                .unwrap_or_default()
        };
        Ok((
            date,
            Self {
                end_date: (end_date != date).then(|| end_date.to_string()),
                color: draft.color.clamp(0, 3) as u8,
                title: title.into(),
                start: normalized(start),
                end: normalized(end),
                location: draft.location.trim().into(),
                description: draft.description.trim().into(),
            },
        ))
    }
}

pub(super) fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .filter(|date| (1..=9999).contains(&date.year()) && date.to_string() == text)
}

fn parse_time(text: &str) -> Result<Option<NaiveTime>, &'static str> {
    if text.is_empty() {
        return Ok(None);
    }
    let text = text.to_ascii_uppercase();
    NaiveTime::parse_from_str(&text, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(&text, "%I:%M %p"))
        .map(Some)
        .map_err(|_| "Enter a time such as 14:30 or 2:30 PM.")
}

pub(super) fn display_time(text: &str, twelve_hour: bool) -> String {
    use chrono::Timelike;
    parse_time(text).ok().flatten().map_or_else(
        || text.into(),
        |time| super::clock_text(time.hour(), time.minute(), twelve_hour),
    )
}

impl Preferences {
    pub(super) fn on_day(&self, date: NaiveDate) -> Vec<(NaiveDate, usize, &Event)> {
        self.events
            .range(..=date.to_string())
            .filter_map(|(key, events)| Some((parse_date(key)?, events)))
            .flat_map(|(source, events)| {
                events
                    .iter()
                    .enumerate()
                    .filter(move |(_, event)| event.last_date(source) >= date)
                    .map(move |(index, event)| (source, index, event))
            })
            .collect()
    }

    pub(super) fn remove_event(
        &mut self,
        source: NaiveDate,
        index: usize,
    ) -> Result<(), &'static str> {
        let key = source.to_string();
        let entries = self
            .events
            .get_mut(&key)
            .filter(|entries| index < entries.len())
            .ok_or("This event is no longer available.")?;
        entries.remove(index);
        if entries.is_empty() {
            self.events.remove(&key);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn events(&self, date: NaiveDate) -> &[Event] {
        self.events
            .get(&date.to_string())
            .map_or(&[], Vec::as_slice)
    }

    pub(super) fn save_event(
        &mut self,
        original: Option<(NaiveDate, usize)>,
        date: NaiveDate,
        event: Event,
    ) -> Result<(), &'static str> {
        if let Some((source, index)) = original {
            if source == date {
                let slot = self
                    .events
                    .get_mut(&source.to_string())
                    .and_then(|events| events.get_mut(index))
                    .ok_or("This event is no longer available.")?;
                *slot = event;
                return Ok(());
            }
            self.remove_event(source, index)?;
        }
        self.events.entry(date.to_string()).or_default().push(event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_legacy_events_in_place_and_moves_them_without_duplication() {
        let mut preferences: Preferences = serde_json::from_str(r#"{"events":{"2026-11-26":[{"title":"First","start":"","end":"","location":"","description":""},{"title":"Second","start":"","end":"","location":"","description":""}]}}"#).unwrap();
        let source = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let target = source.succ_opt().unwrap();
        let mut draft = preferences.events(source)[0].draft(source, false);
        draft.title = "Edited".into();
        let (_, event) = Event::validate(draft).unwrap();
        preferences
            .save_event(Some((source, 0)), source, event.clone())
            .unwrap();
        assert_eq!(preferences.events(source)[0].title, "Edited");
        preferences
            .save_event(Some((source, 0)), target, event)
            .unwrap();
        assert_eq!(preferences.events(source)[0].title, "Second");
        assert_eq!(preferences.events(target).len(), 1);
        assert_eq!(preferences.events(target)[0].title, "Edited");
    }

    #[test]
    fn validates_time_and_date() {
        assert!(parse_time("00:00").is_ok());
        assert!(parse_time("24:00").is_err());
        assert!(parse_time("09:60").is_err());
        let draft = CalendarDraft {
            date: "2026-02-29".into(),
            title: "Event".into(),
            ..Default::default()
        };
        assert!(Event::validate(draft).is_err());
    }

    #[test]
    fn multi_day_events_keep_their_origin_and_can_be_deleted_from_a_later_day() {
        let mut preferences = Preferences::default();
        let draft = CalendarDraft {
            date: "2026-11-30".into(),
            end_date: "2026-12-02".into(),
            title: "Trip".into(),
            start: "3:15 PM".into(),
            end: "00:00".into(),
            color: 2,
            ..Default::default()
        };
        let (source, event) = Event::validate(draft).unwrap();
        assert_eq!(event.start, "15:15");
        preferences.save_event(None, source, event).unwrap();
        let next = source.succ_opt().unwrap();
        let hits = preferences.on_day(next);
        let [(origin, index, event)] = hits.as_slice() else {
            panic!("expected one occurrence")
        };
        assert_eq!(*origin, source);
        assert_eq!(event.color, 2);
        assert!(preferences.on_day(next.succ_opt().unwrap()).is_empty());
        preferences.remove_event(*origin, *index).unwrap();
        assert!(preferences.on_day(next).is_empty());
    }
}
