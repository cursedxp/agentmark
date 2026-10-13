use crate::error::RtError;
use chrono::{Datelike, Duration, Local, NaiveDateTime, Weekday};
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Every(Duration),
    EveryDayAt { hour: u32, minute: u32 },
    Weekly { weekday: Weekday, hour: u32, minute: u32 },
}

/// When an entry fires (local time). Build with the validating constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule(Kind);

fn check_time(hour: u32, minute: u32) -> Result<(), RtError> {
    if hour < 24 && minute < 60 {
        Ok(())
    } else {
        Err(RtError::Schedule(format!("invalid time {hour:02}:{minute:02}")))
    }
}

impl Schedule {
    /// Every fixed interval (`Every 30 minutes`); the interval must be positive.
    pub fn every(interval: Duration) -> Result<Schedule, RtError> {
        if interval > Duration::zero() {
            Ok(Schedule(Kind::Every(interval)))
        } else {
            Err(RtError::Schedule("interval must be greater than zero".into()))
        }
    }

    /// Daily at a local time (`Every day at 09:00`); hour < 24, minute < 60.
    pub fn every_day_at(hour: u32, minute: u32) -> Result<Schedule, RtError> {
        check_time(hour, minute)?;
        Ok(Schedule(Kind::EveryDayAt { hour, minute }))
    }

    /// Weekly on a weekday at a local time (`Every Monday at 08:30`); hour < 24, minute < 60.
    pub fn weekly(weekday: Weekday, hour: u32, minute: u32) -> Result<Schedule, RtError> {
        check_time(hour, minute)?;
        Ok(Schedule(Kind::Weekly { weekday, hour, minute }))
    }

    /// The first fire time strictly after `after` (pass the previous fire time; `Every` adds the interval to it).
    pub fn next_fire(&self, after: NaiveDateTime) -> NaiveDateTime {
        match self.0 {
            Kind::Every(d) => after + d,
            Kind::EveryDayAt { hour, minute } => {
                let today = after.date().and_hms_opt(hour, minute, 0).expect("validated time");
                if today > after { today } else { today + Duration::days(1) }
            }
            Kind::Weekly { weekday, hour, minute } => {
                let ahead = (7 + weekday.num_days_from_monday() as i64 - after.weekday().num_days_from_monday() as i64) % 7;
                let t = (after.date() + Duration::days(ahead)).and_hms_opt(hour, minute, 0).expect("validated time");
                if t > after { t } else { t + Duration::days(7) }
            }
        }
    }
}

/// `--once` mode: run the handler exactly one time and return its output.
pub async fn run_once<F, Fut>(mut handler: F) -> Fut::Output
where
    F: FnMut() -> Fut,
    Fut: Future,
{
    handler().await
}

/// Run the handler on the schedule forever; each tick is computed from the previous fire time, skipping missed ticks.
pub async fn run_schedule<F, Fut>(schedule: Schedule, mut handler: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    let mut next = schedule.next_fire(Local::now().naive_local());
    loop {
        let wait = (next - Local::now().naive_local()).to_std().unwrap_or_default();
        tokio::time::sleep(wait).await;
        handler().await;
        let now = Local::now().naive_local();
        next = schedule.next_fire(next);
        while next <= now {
            next = schedule.next_fire(next);
        }
    }
}
