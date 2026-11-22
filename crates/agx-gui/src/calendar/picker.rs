//! Month and time choices for the event editor.
use super::{MONTHS, clock_text, month_grid, parse_date};
use crate::{AppWindow, Calendar, CalendarDay};
use chrono::{Datelike, NaiveDate};
use slint::{ComponentHandle, ModelRc, VecModel};

pub(super) fn update(
    ui: &AppWindow,
    month_index: &mut i32,
    fallback: i32,
    selected: Option<NaiveDate>,
    text: &str,
    delta: i32,
) {
    if delta == 0 {
        *month_index = parse_date(text)
            .or(selected)
            .map_or(fallback, |date| date.year() * 12 + date.month0() as i32);
    }
    *month_index = (*month_index + delta).clamp(12, 9999 * 12 + 11);
    let year = *month_index / 12;
    let month = (*month_index % 12) as usize;
    let (first, length) = month_grid(year, month);
    let selected = parse_date(text);
    let days = (0..42)
        .map(|index| {
            let number = index - first + 1;
            if (1..=length).contains(&number) {
                CalendarDay {
                    number,
                    today: selected
                        == NaiveDate::from_ymd_opt(year, month as u32 + 1, number as u32),
                    ..Default::default()
                }
            } else {
                CalendarDay::default()
            }
        })
        .collect::<Vec<_>>();
    let calendar = ui.global::<Calendar>();
    calendar.set_picker_days(ModelRc::new(VecModel::from(days)));
    calendar.set_picker_month(format!("{} {year}", MONTHS[month]).into());
    calendar.set_picker_prefix(format!("{year:04}-{:02}-", month + 1).into());
}

pub(super) fn time_choices(twelve_hour: bool) -> ModelRc<slint::SharedString> {
    let values = (0..96)
        .map(|quarter| clock_text(quarter / 4, (quarter % 4) * 15, twelve_hour).into())
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(values))
}
