use agentmark_rt::{run_once, Schedule, Weekday};
use chrono::{Duration, NaiveDate, NaiveDateTime};

fn dt(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
}

#[test]
fn every_adds_interval() {
    let s = Schedule::Every(Duration::minutes(30));
    assert_eq!(s.next_fire(dt("2026-10-13 10:00:00")), dt("2026-10-13 10:30:00"));
}

#[test]
fn every_day_at() {
    let s = Schedule::EveryDayAt { hour: 9, minute: 0 };
    assert_eq!(s.next_fire(dt("2026-10-13 08:59:59")), dt("2026-10-13 09:00:00"));
    // exactly at fire time -> strictly after, so tomorrow
    assert_eq!(s.next_fire(dt("2026-10-13 09:00:00")), dt("2026-10-14 09:00:00"));
    assert_eq!(s.next_fire(dt("2026-10-13 23:00:00")), dt("2026-10-14 09:00:00"));
    // month / year rollover
    assert_eq!(s.next_fire(dt("2026-12-31 12:00:00")), dt("2027-01-01 09:00:00"));
}

#[test]
fn weekly() {
    // 2026-10-13 is a Tuesday
    assert_eq!(NaiveDate::from_ymd_opt(2026, 10, 13).unwrap().format("%A").to_string(), "Tuesday");
    let s = Schedule::Weekly { weekday: Weekday::Mon, hour: 8, minute: 30 };
    assert_eq!(s.next_fire(dt("2026-10-13 10:00:00")), dt("2026-10-19 08:30:00"));
    let s = Schedule::Weekly { weekday: Weekday::Tue, hour: 11, minute: 0 };
    assert_eq!(s.next_fire(dt("2026-10-13 10:00:00")), dt("2026-10-13 11:00:00"));
    // same weekday but time has passed -> next week
    assert_eq!(s.next_fire(dt("2026-10-13 11:00:00")), dt("2026-10-20 11:00:00"));
    let s = Schedule::Weekly { weekday: Weekday::Wed, hour: 0, minute: 0 };
    assert_eq!(s.next_fire(dt("2026-10-13 23:59:00")), dt("2026-10-14 00:00:00"));
}

#[tokio::test]
async fn run_once_runs_handler_exactly_once() {
    let n = std::sync::atomic::AtomicUsize::new(0);
    let out = run_once(|| async {
        n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        7
    })
    .await;
    assert_eq!(out, 7);
    assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn run_schedule_fires_repeatedly() {
    use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = n.clone();
    let h = tokio::spawn(agentmark_rt::run_schedule(Schedule::Every(Duration::seconds(10)), move || {
        let n = n2.clone();
        async move {
            n.fetch_add(1, Ordering::SeqCst);
        }
    }));
    tokio::time::sleep(std::time::Duration::from_secs(35)).await;
    h.abort();
    assert_eq!(n.load(Ordering::SeqCst), 3);
}
