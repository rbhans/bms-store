use std::time::{SystemTime, UNIX_EPOCH};

use super::types::{DateSpec, Ordinal};

// ----------------------------------------------------------------
// Time helpers (no chrono dependency)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalTime {
    pub(crate) year: i32,
    pub(crate) month: u8,
    pub(crate) day: u8,
    pub(crate) weekday: u8, // 0=Monday .. 6=Sunday
    pub(crate) hour: u8,
    pub(crate) minute: u8,
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct Tm {
    pub(crate) tm_sec: i32,
    pub(crate) tm_min: i32,
    pub(crate) tm_hour: i32,
    pub(crate) tm_mday: i32,
    pub(crate) tm_mon: i32,
    pub(crate) tm_year: i32,
    pub(crate) tm_wday: i32,
    pub(crate) tm_yday: i32,
    pub(crate) tm_isdst: i32,
    pub(crate) tm_gmtoff: i64,
    pub(crate) tm_zone: *const i8,
}

extern "C" {
    pub(crate) fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
    pub(crate) fn mktime(tm: *mut Tm) -> i64;
}

pub(crate) fn local_time_now() -> LocalTime {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut tm = Tm::default();
    unsafe { localtime_r(&secs, &mut tm) };
    // tm_wday: 0=Sun, 1=Mon, ..., 6=Sat → convert to 0=Mon..6=Sun
    let weekday = if tm.tm_wday == 0 {
        6
    } else {
        (tm.tm_wday - 1) as u8
    };
    LocalTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u8,
        day: tm.tm_mday as u8,
        weekday,
        hour: tm.tm_hour as u8,
        minute: tm.tm_min as u8,
    }
}

/// Get the weekday (0=Mon..6=Sun) for a given date.
pub(crate) fn weekday_of(year: i32, month: u8, day: u8) -> u8 {
    let mut tm = Tm {
        tm_year: year - 1900,
        tm_mon: month as i32 - 1,
        tm_mday: day as i32,
        tm_hour: 12,
        ..Default::default()
    };
    unsafe { mktime(&mut tm) };
    if tm.tm_wday == 0 {
        6
    } else {
        (tm.tm_wday - 1) as u8
    }
}

pub(crate) fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Resolve a DateSpec to (month, day) for the given year. Returns None if not applicable.
pub(crate) fn resolve_date_spec(spec: &DateSpec, year: i32) -> Option<(u8, u8)> {
    match spec {
        DateSpec::Fixed { month, day } => Some((*month, *day)),
        DateSpec::FixedYear {
            year: y,
            month,
            day,
        } => {
            if *y as i32 == year {
                Some((*month, *day))
            } else {
                None
            }
        }
        DateSpec::Relative {
            ordinal,
            weekday,
            month,
        } => {
            let dim = days_in_month(year, *month);
            let target_wd = *weekday; // 0=Mon..6=Sun

            match ordinal {
                Ordinal::Last => {
                    // Search backward from last day of month
                    for d in (1..=dim).rev() {
                        if weekday_of(year, *month, d) == target_wd {
                            return Some((*month, d));
                        }
                    }
                    None
                }
                _ => {
                    let n = match ordinal {
                        Ordinal::First => 1,
                        Ordinal::Second => 2,
                        Ordinal::Third => 3,
                        Ordinal::Fourth => 4,
                        Ordinal::Last => unreachable!(),
                    };
                    let mut count = 0;
                    for d in 1..=dim {
                        if weekday_of(year, *month, d) == target_wd {
                            count += 1;
                            if count == n {
                                return Some((*month, d));
                            }
                        }
                    }
                    None
                }
            }
        }
    }
}
