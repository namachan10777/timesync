use std::{fmt::Debug, ops};

use chrono::{DateTime, TimeZone};

pub mod client;
pub mod server;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TimeOffset {
    Later(chrono::Duration),
    Earlier(chrono::Duration),
}

impl TimeOffset {
    pub fn diff<Tz: TimeZone>(base: DateTime<Tz>, sample: DateTime<Tz>) -> Self {
        if sample < base {
            TimeOffset::Later(base - sample)
        } else {
            TimeOffset::Earlier(sample - base)
        }
    }

    pub fn correct<Tz: TimeZone>(&self, time: DateTime<Tz>) -> DateTime<Tz> {
        match self {
            Self::Earlier(offset) => time + *offset,
            Self::Later(offset) => time - *offset,
        }
    }
}

impl Debug for TimeOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Earlier(t) => {
                if let Ok(dur) = t.to_std() {
                    f.write_fmt(format_args!("Early({})", humantime::format_duration(dur)))
                } else {
                    f.write_fmt(format_args!("Early({:?})", t))
                }
            }
            Self::Later(t) => {
                if let Ok(dur) = t.to_std() {
                    f.write_fmt(format_args!("Later({})", humantime::format_duration(dur)))
                } else {
                    f.write_fmt(format_args!("Later({:?})", t))
                }
            }
        }
    }
}

impl ops::Add for TimeOffset {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (TimeOffset::Earlier(t1), TimeOffset::Earlier(t2)) => TimeOffset::Earlier(t1 + t2),
            (TimeOffset::Earlier(t1), TimeOffset::Later(t2)) if t1 > t2 => {
                TimeOffset::Earlier(t1 - t2)
            }
            (TimeOffset::Earlier(t1), TimeOffset::Later(t2)) => TimeOffset::Later(t2 - t1),
            (TimeOffset::Later(t1), TimeOffset::Earlier(t2)) if t1 > t2 => {
                TimeOffset::Later(t1 - t2)
            }
            (TimeOffset::Later(t1), TimeOffset::Earlier(t2)) => TimeOffset::Earlier(t2 - t1),
            (TimeOffset::Later(t1), TimeOffset::Later(t2)) => TimeOffset::Later(t1 + t2),
        }
    }
}

impl ops::Neg for TimeOffset {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            TimeOffset::Earlier(t) => TimeOffset::Later(t),
            TimeOffset::Later(t) => TimeOffset::Earlier(t),
        }
    }
}

impl ops::Sub for TimeOffset {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        ops::Add::add(self, ops::Neg::neg(rhs))
    }
}

impl ops::AddAssign for TimeOffset {
    fn add_assign(&mut self, rhs: Self) {
        *self = ops::Add::add(*self, rhs);
    }
}

impl ops::SubAssign for TimeOffset {
    fn sub_assign(&mut self, rhs: Self) {
        *self = ops::Sub::sub(*self, rhs);
    }
}

impl ops::Div<i32> for TimeOffset {
    type Output = Self;
    fn div(self, rhs: i32) -> Self::Output {
        let is_neg = rhs < 0;
        let rhs_abs = rhs.abs();
        let ret = match self {
            TimeOffset::Earlier(t) => TimeOffset::Earlier(t / rhs_abs),
            TimeOffset::Later(t) => TimeOffset::Later(t / rhs_abs),
        };
        if is_neg {
            ops::Neg::neg(ret)
        } else {
            ret
        }
    }
}

impl Default for TimeOffset {
    fn default() -> Self {
        Self::Later(chrono::Duration::zero())
    }
}
