use crate::config::profile::PointValue;

use super::engine::date_spec_matches_today;
use super::time::{weekday_of, LocalTime, Tm};
use super::types::*;

// ----------------------------------------------------------------
// Preview timeline (Phase 3a)
// ----------------------------------------------------------------

/// A block in the 7-day preview timeline.
#[derive(Debug, Clone)]
pub struct PreviewBlock {
    pub start: TimeOfDay,
    pub end: TimeOfDay,
    pub value: PointValue,
    pub source: String,
}

/// Compute a 7-day preview starting from `start_date`.
/// Returns 7 vectors of blocks (one per day, starting from the given date).
pub fn compute_preview(
    schedule: &Schedule,
    exceptions: &[ScheduleException],
    start_year: i32,
    start_month: u8,
    start_day: u8,
) -> [Vec<PreviewBlock>; 7] {
    let mut result: [Vec<PreviewBlock>; 7] = Default::default();

    for day_offset in 0..7u8 {
        // Compute the actual date for this offset
        let (y, m, d) = add_days(start_year, start_month, start_day, day_offset as i32);
        let wd = weekday_of(y, m, d);

        let fake_now = LocalTime {
            year: y,
            month: m,
            day: d,
            weekday: wd,
            hour: 0,
            minute: 0,
        };

        // Determine which exception matches this day, if any
        let mut exc_match: Option<&ScheduleException> = None;
        for exc in exceptions.iter().rev() {
            if date_spec_matches_today(&exc.date_spec, &fake_now) {
                exc_match = Some(exc);
                break;
            }
        }

        let (slots, source) = if let Some(exc) = exc_match {
            if exc.use_default {
                (
                    &DaySlots(Vec::new()) as *const DaySlots,
                    format!("exception:{}", exc.name),
                )
            } else {
                (
                    &exc.slots as *const DaySlots,
                    format!("exception:{}", exc.name),
                )
            }
        } else {
            (
                &schedule.weekly[wd as usize] as *const DaySlots,
                format!("weekly:{}", day_label(wd)),
            )
        };

        // SAFETY: slots pointer is valid for the duration of this iteration
        let slots_ref = unsafe { &*slots };

        let day_blocks = build_day_blocks(slots_ref, &schedule.default_value, &source);
        result[day_offset as usize] = day_blocks;
    }

    result
}

fn build_day_blocks(
    slots: &DaySlots,
    default_value: &PointValue,
    source: &str,
) -> Vec<PreviewBlock> {
    if slots.0.is_empty() {
        // Whole day is default value
        return vec![PreviewBlock {
            start: TimeOfDay::new(0, 0),
            end: TimeOfDay::new(23, 59),
            value: default_value.clone(),
            source: source.to_string(),
        }];
    }

    let mut blocks = Vec::new();

    // If first slot doesn't start at midnight, add a default block
    if slots.0[0].time.total_minutes() > 0 {
        blocks.push(PreviewBlock {
            start: TimeOfDay::new(0, 0),
            end: TimeOfDay {
                hour: slots.0[0].time.hour,
                minute: if slots.0[0].time.minute > 0 {
                    slots.0[0].time.minute - 1
                } else {
                    59
                },
            },
            value: default_value.clone(),
            source: source.to_string(),
        });
    }

    for (i, slot) in slots.0.iter().enumerate() {
        let end = if i + 1 < slots.0.len() {
            let next = &slots.0[i + 1].time;
            TimeOfDay {
                hour: if next.minute > 0 {
                    next.hour
                } else if next.hour > 0 {
                    next.hour - 1
                } else {
                    0
                },
                minute: if next.minute > 0 { next.minute - 1 } else { 59 },
            }
        } else {
            TimeOfDay::new(23, 59)
        };

        blocks.push(PreviewBlock {
            start: slot.time,
            end,
            value: slot.value.clone(),
            source: source.to_string(),
        });
    }

    blocks
}

fn day_label(weekday: u8) -> &'static str {
    match weekday {
        0 => "Monday",
        1 => "Tuesday",
        2 => "Wednesday",
        3 => "Thursday",
        4 => "Friday",
        5 => "Saturday",
        6 => "Sunday",
        _ => "Unknown",
    }
}

fn add_days(year: i32, month: u8, day: u8, offset: i32) -> (i32, u8, u8) {
    let mut tm = Tm {
        tm_year: year - 1900,
        tm_mon: month as i32 - 1,
        tm_mday: day as i32 + offset,
        tm_hour: 12,
        ..Default::default()
    };
    unsafe { super::time::mktime(&mut tm) };
    (tm.tm_year + 1900, (tm.tm_mon + 1) as u8, tm.tm_mday as u8)
}
