use crate::config::profile::PointValue;

use super::types::*;

// ----------------------------------------------------------------
// Schedule templates
// ----------------------------------------------------------------

/// M-F 06:00-18:00 — standard office hours.
pub fn template_office_hours(on: PointValue, off: PointValue) -> WeeklySchedule {
    let mut weekly = empty_weekly();
    for slot in weekly.iter_mut().take(5) {
        // Mon-Fri
        *slot = DaySlots(vec![
            TimeSlot {
                time: TimeOfDay::new(6, 0),
                value: on.clone(),
            },
            TimeSlot {
                time: TimeOfDay::new(18, 0),
                value: off.clone(),
            },
        ]);
    }
    weekly
}

/// M-F 05:00-22:00 — extended hours.
pub fn template_extended_hours(on: PointValue, off: PointValue) -> WeeklySchedule {
    let mut weekly = empty_weekly();
    for slot in weekly.iter_mut().take(5) {
        *slot = DaySlots(vec![
            TimeSlot {
                time: TimeOfDay::new(5, 0),
                value: on.clone(),
            },
            TimeSlot {
                time: TimeOfDay::new(22, 0),
                value: off.clone(),
            },
        ]);
    }
    weekly
}

/// 24/7 — always on (every day starts with the "on" value at midnight).
pub fn template_24_7(on: PointValue) -> WeeklySchedule {
    let mut weekly = empty_weekly();
    for slot in &mut weekly {
        *slot = DaySlots(vec![TimeSlot {
            time: TimeOfDay::new(0, 0),
            value: on.clone(),
        }]);
    }
    weekly
}

/// M-Sat 08:00-21:00, Sun 10:00-18:00 — retail hours.
pub fn template_retail(on: PointValue, off: PointValue) -> WeeklySchedule {
    let mut weekly = empty_weekly();
    for slot in weekly.iter_mut().take(6) {
        // Mon-Sat
        *slot = DaySlots(vec![
            TimeSlot {
                time: TimeOfDay::new(8, 0),
                value: on.clone(),
            },
            TimeSlot {
                time: TimeOfDay::new(21, 0),
                value: off.clone(),
            },
        ]);
    }
    // Sunday
    weekly[6] = DaySlots(vec![
        TimeSlot {
            time: TimeOfDay::new(10, 0),
            value: on.clone(),
        },
        TimeSlot {
            time: TimeOfDay::new(18, 0),
            value: off.clone(),
        },
    ]);
    weekly
}

/// M-F 06:00-16:00 — school hours.
pub fn template_school(on: PointValue, off: PointValue) -> WeeklySchedule {
    let mut weekly = empty_weekly();
    for slot in weekly.iter_mut().take(5) {
        *slot = DaySlots(vec![
            TimeSlot {
                time: TimeOfDay::new(6, 0),
                value: on.clone(),
            },
            TimeSlot {
                time: TimeOfDay::new(16, 0),
                value: off.clone(),
            },
        ]);
    }
    weekly
}

/// M-F 05:00-17:00 — warehouse hours.
pub fn template_warehouse(on: PointValue, off: PointValue) -> WeeklySchedule {
    let mut weekly = empty_weekly();
    for slot in weekly.iter_mut().take(5) {
        *slot = DaySlots(vec![
            TimeSlot {
                time: TimeOfDay::new(5, 0),
                value: on.clone(),
            },
            TimeSlot {
                time: TimeOfDay::new(17, 0),
                value: off.clone(),
            },
        ]);
    }
    weekly
}

// ----------------------------------------------------------------
// Pre-built exception groups (holiday templates)
// ----------------------------------------------------------------

/// US Federal Holidays as DateSpec entries.
pub fn us_federal_holidays() -> Vec<DateSpec> {
    vec![
        // New Year's Day
        DateSpec::Fixed { month: 1, day: 1 },
        // MLK Day — 3rd Monday in January
        DateSpec::Relative {
            ordinal: Ordinal::Third,
            weekday: 0,
            month: 1,
        },
        // Presidents' Day — 3rd Monday in February
        DateSpec::Relative {
            ordinal: Ordinal::Third,
            weekday: 0,
            month: 2,
        },
        // Memorial Day — last Monday in May
        DateSpec::Relative {
            ordinal: Ordinal::Last,
            weekday: 0,
            month: 5,
        },
        // Juneteenth
        DateSpec::Fixed { month: 6, day: 19 },
        // Independence Day
        DateSpec::Fixed { month: 7, day: 4 },
        // Labor Day — 1st Monday in September
        DateSpec::Relative {
            ordinal: Ordinal::First,
            weekday: 0,
            month: 9,
        },
        // Columbus Day — 2nd Monday in October
        DateSpec::Relative {
            ordinal: Ordinal::Second,
            weekday: 0,
            month: 10,
        },
        // Veterans Day
        DateSpec::Fixed { month: 11, day: 11 },
        // Thanksgiving — 4th Thursday in November
        DateSpec::Relative {
            ordinal: Ordinal::Fourth,
            weekday: 3,
            month: 11,
        },
        // Christmas
        DateSpec::Fixed { month: 12, day: 25 },
    ]
}

/// UK Bank Holidays as DateSpec entries (approximation — some are fixed by proclamation).
pub fn uk_bank_holidays() -> Vec<DateSpec> {
    vec![
        // New Year's Day
        DateSpec::Fixed { month: 1, day: 1 },
        // Good Friday — not easily computed without Easter algorithm; skip for now
        // Easter Monday — same issue
        // Early May bank holiday — 1st Monday in May
        DateSpec::Relative {
            ordinal: Ordinal::First,
            weekday: 0,
            month: 5,
        },
        // Spring bank holiday — last Monday in May
        DateSpec::Relative {
            ordinal: Ordinal::Last,
            weekday: 0,
            month: 5,
        },
        // Summer bank holiday — last Monday in August
        DateSpec::Relative {
            ordinal: Ordinal::Last,
            weekday: 0,
            month: 8,
        },
        // Christmas Day
        DateSpec::Fixed { month: 12, day: 25 },
        // Boxing Day
        DateSpec::Fixed { month: 12, day: 26 },
    ]
}
