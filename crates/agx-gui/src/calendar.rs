//! Local calendar dates, per-account event storage and the desktop clock.
mod events;
mod holidays;
mod picker;

use self::events::{Event, Preferences, display_time, parse_date};
use crate::{
    AppWindow, Calendar, CalendarDay, CalendarDraft, CalendarEntry, CalendarEvent, CalendarPage,
    settings,
};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Timelike, Utc};
use chrono_tz::{TZ_VARIANTS, Tz};
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use std::{
    cell::RefCell,
    fs, io,
    path::PathBuf,
    rc::Rc,
    time::{Duration, SystemTime},
};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const SYSTEM_ZONE: &str = "System time zone";

struct CalendarState {
    path: Option<PathBuf>,
    preferences: Preferences,
    month: i32,
    selected: Option<NaiveDate>,
    editing: Option<(NaiveDate, usize)>,
    picker_month: i32,
}

fn local_time(zone: Option<Tz>) -> Option<NaiveDateTime> {
    if let Some(zone) = zone {
        return Some(
            DateTime::<Utc>::from(SystemTime::now())
                .with_timezone(&zone)
                .naive_local(),
        );
    }
    // SAFETY: localtime_r initializes the supplied tm, and time accepts a null output pointer.
    let result = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut result = std::mem::zeroed();
        (!libc::localtime_r(&now, &mut result).is_null()).then_some(result)?
    };
    NaiveDate::from_ymd_opt(
        result.tm_year + 1900,
        result.tm_mon as u32 + 1,
        result.tm_mday as u32,
    )?
    .and_hms_opt(
        result.tm_hour as u32,
        result.tm_min as u32,
        result.tm_sec as u32,
    )
}

fn month_of(time: NaiveDateTime) -> i32 {
    time.year() * 12 + time.month0() as i32
}

fn clock_text(hour: u32, minute: u32, twelve_hour: bool) -> String {
    if twelve_hour {
        let suffix = if hour < 12 { "AM" } else { "PM" };
        let hour = (hour + 11) % 12 + 1;
        format!("{hour}:{minute:02} {suffix}")
    } else {
        format!("{hour:02}:{minute:02}")
    }
}

fn update_clock(ui: &AppWindow, preferences: &Preferences) {
    if let Some(time) = local_time(preferences.time_zone) {
        ui.set_clock(clock_text(time.hour(), time.minute(), preferences.twelve_hour).into());
        ui.set_date(
            format!(
                "{} {} {}",
                DAYS[time.weekday().num_days_from_sunday() as usize],
                time.day(),
                &MONTHS[time.month0() as usize][..3]
            )
            .into(),
        );
    }
}

fn zone_choices(query: &str) -> Vec<slint::SharedString> {
    let query = query.to_lowercase();
    let matches = |name: &str| {
        let name = name.to_ascii_lowercase();
        query.split_whitespace().all(|part| name.contains(part))
    };
    let mut names: Vec<_> = TZ_VARIANTS
        .iter()
        .copied()
        .map(Tz::name)
        .filter(|name| matches(name))
        .collect();
    names.sort_unstable();
    if matches(SYSTEM_ZONE) {
        names.insert(0, SYSTEM_ZONE);
    }
    names.into_iter().map(Into::into).collect()
}

// Gregorian weekday of the first, with Sunday at column zero.
fn month_grid(year: i32, month: usize) -> (i32, i32) {
    let first = NaiveDate::from_ymd_opt(year, month as u32 + 1, 1).unwrap();
    let length = first
        .iter_days()
        .take_while(|date| date.month() == first.month())
        .count();
    (first.weekday().num_days_from_sunday() as i32, length as i32)
}

fn load(path: &PathBuf) -> io::Result<Preferences> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(serde_json::from_str(&contents)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(error) => Err(error),
    }
}

impl CalendarState {
    fn year_month(&self) -> (i32, usize) {
        (self.month / 12, (self.month % 12) as usize)
    }

    fn render_month(&self, ui: &AppWindow) {
        let calendar = ui.global::<Calendar>();
        let (year, month) = self.year_month();
        let (first, length) = month_grid(year, month);
        let today = local_time(self.preferences.time_zone).map(|time| time.date());
        let days: Vec<_> = (0..42)
            .map(|index| {
                let day = index - first + 1;
                let Some(date) = NaiveDate::from_ymd_opt(year, month as u32 + 1, day as u32) else {
                    return CalendarDay::default();
                };
                let holiday = holidays::on(date);
                let events = self.preferences.on_day(date);
                let mut entries = Vec::with_capacity(events.len() + usize::from(holiday.is_some()));
                if let Some(holiday) = &holiday {
                    entries.push(CalendarEntry {
                        title: holiday.as_str().into(),
                        holiday: true,
                        ..Default::default()
                    });
                }
                let mut ordered: Vec<_> = events.iter().copied().collect();
                ordered.sort_by_key(|(source, index, event)| {
                    (
                        event.last_date(*source) == *source,
                        *source,
                        &event.start,
                        *index,
                    )
                });
                entries.extend(ordered.into_iter().map(|(source, _, event)| CalendarEntry {
                    title: event.title.as_str().into(),
                    time: if source == date {
                        display_time(&event.start, self.preferences.twelve_hour).into()
                    } else {
                        "".into()
                    },
                    holiday: false,
                    color: i32::from(event.color),
                    continues_before: source < date,
                    continues_after: event.last_date(source) > date,
                }));
                CalendarDay {
                    number: day,
                    today: today == Some(date),
                    has_events: !events.is_empty(),
                    holiday: holiday.unwrap_or_default().into(),
                    entries: ModelRc::new(VecModel::from(entries)),
                }
            })
            .collect();
        calendar.set_month(format!("{} {year}", MONTHS[month]).into());
        calendar.set_weeks((first + length + 6) / 7);
        calendar.set_days(ModelRc::new(VecModel::from(days)));
    }

    fn render_agenda(&self, ui: &AppWindow, date: NaiveDate) {
        let calendar = ui.global::<Calendar>();
        let events: Vec<_> = self
            .preferences
            .on_day(date)
            .into_iter()
            .map(|(source, _, event)| {
                let start = display_time(&event.start, self.preferences.twelve_hour);
                let end = display_time(&event.end, self.preferences.twelve_hour);
                let times = match (start.is_empty(), end.is_empty()) {
                    (false, false) => format!("{start}–{end}"),
                    (false, true) => start,
                    (true, false) => format!("Ends {end}"),
                    (true, true) => String::new(),
                };
                let dates = event
                    .end_date
                    .as_ref()
                    .map(|end| format!("{source} → {end}"))
                    .unwrap_or_default();
                CalendarEvent {
                    title: event.title.as_str().into(),
                    details: [
                        dates.as_str(),
                        times.as_str(),
                        &event.location,
                        &event.description,
                    ]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ")
                    .into(),
                }
            })
            .collect();
        calendar.set_events(ModelRc::new(VecModel::from(events)));
        calendar.set_holiday(holidays::on(date).unwrap_or_default().into());
        calendar.set_selected_date(
            format!(
                "{} {}, {}",
                MONTHS[date.month0() as usize],
                date.day(),
                date.year()
            )
            .into(),
        );
    }

    fn select(&mut self, ui: &AppWindow, day: i32) {
        let (year, month) = self.year_month();
        let Some(date) = NaiveDate::from_ymd_opt(year, month as u32 + 1, day as u32) else {
            return;
        };
        self.selected = Some(date);
        self.render_agenda(ui, date);
        let calendar = ui.global::<Calendar>();
        calendar.set_error("".into());
        if !self.preferences.on_day(date).is_empty() || holidays::on(date).is_some() {
            calendar.set_editor_back(CalendarPage::Agenda);
            calendar.set_page(CalendarPage::Agenda);
        } else {
            self.open_editor(ui, None, CalendarPage::Month);
        }
    }

    fn open_editor(&mut self, ui: &AppWindow, index: Option<usize>, back: CalendarPage) {
        let Some(date) = self.selected else { return };
        let (draft, editing) = if let Some(index) = index {
            let occurrences = self.preferences.on_day(date);
            let Some(&(source, index, event)) = occurrences.get(index) else {
                return;
            };
            (
                event.draft(source, self.preferences.twelve_hour),
                Some((source, index)),
            )
        } else {
            (
                CalendarDraft {
                    date: date.to_string().into(),
                    end_date: date.to_string().into(),
                    ..Default::default()
                },
                None,
            )
        };
        self.editing = editing;
        let calendar = ui.global::<Calendar>();
        calendar.set_draft(draft);
        calendar.set_editing_saved(editing.is_some());
        calendar.set_editor_back(back);
        calendar.set_error("".into());
        calendar.set_page(CalendarPage::Event);
    }

    fn save_event(&mut self, ui: &AppWindow, draft: CalendarDraft) {
        let calendar = ui.global::<Calendar>();
        let Some(_) = self
            .selected
            .filter(|_| calendar.get_page() == CalendarPage::Event)
        else {
            return;
        };
        let result = Event::validate(draft).and_then(|(date, event)| {
            let mut preferences = self.preferences.clone();
            preferences.save_event(self.editing, date, event)?;
            Ok((date, preferences))
        });
        let (date, preferences) = match result {
            Ok(value) => value,
            Err(error) => {
                calendar.set_error(error.into());
                return;
            }
        };
        if self.save(ui, preferences) {
            self.selected = Some(date);
            self.month = date.year() * 12 + date.month0() as i32;
            self.render_month(ui);
            if self.editing.take().is_some() {
                self.render_agenda(ui, date);
                calendar.set_page(CalendarPage::Agenda);
            } else {
                calendar.set_page(CalendarPage::Month);
            }
        }
    }

    fn delete_event(&mut self, ui: &AppWindow) {
        let calendar = ui.global::<Calendar>();
        let (Some((source, index)), Some(date)) = (self.editing, self.selected) else {
            return;
        };
        if calendar.get_page() != CalendarPage::Event {
            return;
        }
        let mut preferences = self.preferences.clone();
        if let Err(error) = preferences.remove_event(source, index) {
            calendar.set_error(error.into());
        } else if self.save(ui, preferences) {
            self.editing = None;
            self.render_month(ui);
            self.render_agenda(ui, date);
            calendar.set_page(CalendarPage::Agenda);
        }
    }

    fn save(&mut self, ui: &AppWindow, preferences: Preferences) -> bool {
        let result = self
            .path
            .as_ref()
            .ok_or_else(|| io::Error::other("Calendar storage unavailable"))
            .and_then(|path| settings::write_private(path, &serde_json::to_string(&preferences)?));
        let calendar = ui.global::<Calendar>();
        if let Err(error) = result {
            eprintln!("agx-gui: calendar: {error}");
            calendar.set_error("Could not save calendar data.".into());
            return false;
        }
        self.preferences = preferences;
        calendar.set_error("".into());
        true
    }
}

pub(super) fn install(ui: &AppWindow) -> Timer {
    let mut path = settings::config_file("calendar.json");
    let preferences = match path.as_ref().map(load).transpose() {
        Ok(preferences) => preferences.unwrap_or_default(),
        Err(error) => {
            eprintln!("agx-gui: calendar: {error}");
            path = None;
            Preferences::default()
        }
    };
    let state = Rc::new(RefCell::new(CalendarState {
        path,
        month: local_time(preferences.time_zone).map_or(1970 * 12, month_of),
        preferences,
        selected: None,
        editing: None,
        picker_month: 0,
    }));
    let calendar = ui.global::<Calendar>();
    calendar.set_twelve_hour(state.borrow().preferences.twelve_hour);
    calendar.set_zone(
        state
            .borrow()
            .preferences
            .time_zone
            .map_or(SYSTEM_ZONE, |zone| zone.name())
            .into(),
    );
    calendar.set_times_12(picker::time_choices(true));
    calendar.set_times_24(picker::time_choices(false));
    calendar.set_zones(ModelRc::new(VecModel::from(zone_choices(""))));
    macro_rules! hook {
        ($callback:ident, |$state:ident, $ui:ident $(, $arg:ident)*| $body:block) => {{
            let weak = ui.as_weak();
            let state = state.clone();
            calendar.$callback(move |$($arg),*| {
                if let Some($ui) = weak.upgrade() {
                    let mut $state = state.borrow_mut();
                    $body
                }
            });
        }};
    }
    hook!(on_opened, |state, ui| {
        state.month = local_time(state.preferences.time_zone).map_or(state.month, month_of);
        state.render_month(&ui);
        let calendar = ui.global::<Calendar>();
        calendar.set_page(CalendarPage::Month);
        calendar.set_error("".into());
        calendar.set_open(true);
    });
    hook!(on_navigate, |state, ui, delta| {
        state.month = (state.month + delta.clamp(-1, 1)).clamp(12, 9999 * 12 + 11);
        state.render_month(&ui);
    });
    hook!(on_select_day, |state, ui, day| {
        if ui.global::<Calendar>().get_page() == CalendarPage::Month {
            state.select(&ui, day);
        }
    });
    hook!(on_open_editor, |state, ui, index| {
        if ui.global::<Calendar>().get_page() == CalendarPage::Agenda {
            state.open_editor(&ui, usize::try_from(index).ok(), CalendarPage::Agenda);
        }
    });
    hook!(on_time_format, |state, ui, twelve_hour| {
        let mut preferences = state.preferences.clone();
        preferences.twelve_hour = twelve_hour;
        if state.save(&ui, preferences) {
            ui.global::<Calendar>().set_twelve_hour(twelve_hour);
            state.render_month(&ui);
            update_clock(&ui, &state.preferences);
        }
    });
    let weak = ui.as_weak();
    calendar.on_search_zones(move |query| {
        if let Some(ui) = weak.upgrade() {
            ui.global::<Calendar>()
                .set_zones(ModelRc::new(VecModel::from(zone_choices(&query))));
        }
    });
    hook!(on_choose_zone, |state, ui, zone| {
        let time_zone = if zone == SYSTEM_ZONE {
            None
        } else if let Ok(time_zone) = zone.parse::<Tz>() {
            Some(time_zone)
        } else {
            return;
        };
        let mut preferences = state.preferences.clone();
        preferences.time_zone = time_zone;
        let old_month = local_time(state.preferences.time_zone).map_or(state.month, month_of);
        if state.save(&ui, preferences) {
            let calendar = ui.global::<Calendar>();
            calendar.set_zone(zone);
            calendar.set_page(CalendarPage::Month);
            if state.month == old_month {
                state.month = local_time(time_zone).map_or(old_month, month_of);
            }
            state.render_month(&ui);
            update_clock(&ui, &state.preferences);
        }
    });
    hook!(on_save_event, |state, ui, draft| {
        state.save_event(&ui, draft);
    });

    hook!(on_delete_event, |state, ui| {
        state.delete_event(&ui);
    });
    hook!(on_pick_month, |state, ui, text, delta| {
        let CalendarState {
            picker_month,
            month,
            selected,
            ..
        } = &mut *state;
        picker::update(
            &ui,
            picker_month,
            *month,
            *selected,
            &text,
            delta.clamp(-1, 1),
        );
    });

    let timer = Timer::default();
    let weak = ui.as_weak();
    let tick = move || {
        if let Some(ui) = weak.upgrade() {
            update_clock(&ui, &state.borrow().preferences);
        }
    };
    tick();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), tick);
    timer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_handles_leap_days_and_clock_boundaries() {
        assert_eq!(month_grid(2024, 1), (4, 29));
        assert_eq!(month_grid(2100, 1).1, 28);
        assert_eq!(clock_text(0, 5, true), "12:05 AM");
        assert_eq!(clock_text(12, 0, true), "12:00 PM");
        assert_eq!(clock_text(23, 59, false), "23:59");
    }

    #[test]
    fn zone_picker_matches_regions_and_cities_and_uses_dst() {
        assert!(zone_choices("America New York").contains(&"America/New_York".into()));
        assert_eq!(
            zone_choices("system zone"),
            [slint::SharedString::from(SYSTEM_ZONE)]
        );
        let ny = chrono_tz::America::New_York;
        let local_hour = |date: &str| {
            DateTime::parse_from_rfc3339(date)
                .unwrap()
                .with_timezone(&ny)
                .hour()
        };
        assert_eq!(local_hour("2026-01-15T15:00:00Z"), 10);
        assert_eq!(local_hour("2026-07-15T15:00:00Z"), 11);
    }
}
