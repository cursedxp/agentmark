use chrono::{Datelike, Duration, Local, NaiveDateTime, Weekday};
use std::future::Future;

/// When an entry fires (local time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schedule {
    /// Every fixed interval (`Every 30 minutes`).
    Every(Duration),
    /// Daily at a local time (`Every day at 09:00`).
    EveryDayAt {
        /// Hour 0-23.
        hour: u32,
        /// Minute 0-59.
        minute: u32,
    },
    /// Weekly on a weekday at a local time (`Every Monday at 08:30`).
    Weekly {
        /// Day of week.
        weekday: Weekday,
        /// Hour 0-23.
        hour: u32,
        /// Minute 0-59.
        minute: u32,
    },
}

impl Schedule {
    /// The first fire time strictly after `now` (pure function of its input, local time).
    pub fn next_fire(&self, now: NaiveDateTime) -> NaiveDateTime {
        match *self {
            Schedule::Every(d) => now + d,
            Schedule::EveryDayAt { hour, minute } => {
                let today = now.date().and_hms_opt(hour, minute, 0).expect("valid time");
                if today > now { today } else { today + Duration::days(1) }
            }
            Schedule::Weekly { weekday, hour, minute } => {
                let ahead = (7 + weekday.num_days_from_monday() as i64 - now.weekday().num_days_from_monday() as i64) % 7;
                let t = (now.date() + Duration::days(ahead)).and_hms_opt(hour, minute, 0).expect("valid time");
                if t > now { t } else { t + Duration::days(7) }
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

/// Run the handler on the schedule forever (sleeps on tokio until each next fire time).
pub async fn run_schedule<F, Fut>(schedule: Schedule, mut handler: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    loop {
        let now = Local::now().naive_local();
        let wait = (schedule.next_fire(now) - now).to_std().unwrap_or_default();
        tokio::time::sleep(wait).await;
        handler().await;
    }
}
