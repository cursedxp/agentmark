use agentmark_rt::{run_once, Schedule, Weekday};
use chrono::{Duration, NaiveDate, NaiveDateTime};

fn dt(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
}

#[test]
fn every_adds_interval() {
    let s = Schedule::every(Duration::minutes(30)).unwrap();
    assert_eq!(s.next_fire(dt("2026-10-13 10:00:00")), dt("2026-10-13 10:30:00"));
}

#[test]
fn every_day_at() {
    let s = Schedule::every_day_at(9, 0).unwrap();
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
    let s = Schedule::weekly(Weekday::Mon, 8, 30).unwrap();
    assert_eq!(s.next_fire(dt("2026-10-13 10:00:00")), dt("2026-10-19 08:30:00"));
    let s = Schedule::weekly(Weekday::Tue, 11, 0).unwrap();
    assert_eq!(s.next_fire(dt("2026-10-13 10:00:00")), dt("2026-10-13 11:00:00"));
    // same weekday but time has passed -> next week
    assert_eq!(s.next_fire(dt("2026-10-13 11:00:00")), dt("2026-10-20 11:00:00"));
    let s = Schedule::weekly(Weekday::Wed, 0, 0).unwrap();
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

// Real time: the schedule reads the wall clock, so tokio's paused clock cannot drive it.
#[tokio::test]
async fn run_schedule_fires_repeatedly() {
    use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = n.clone();
    let h = tokio::spawn(agentmark_rt::run_schedule(Schedule::every(Duration::milliseconds(300)).unwrap(), move || {
        let n = n2.clone();
        async move {
            n.fetch_add(1, Ordering::SeqCst);
        }
    }));
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    h.abort();
    let fired = n.load(Ordering::SeqCst);
    assert!((2..=4).contains(&fired), "fired {fired} times");
}

#[test]
fn constructors_validate() {
    assert!(Schedule::every(Duration::zero()).is_err());
    assert!(Schedule::every(Duration::seconds(-5)).is_err());
    assert!(Schedule::every_day_at(24, 0).is_err());
    assert!(Schedule::every_day_at(9, 60).is_err());
    assert!(Schedule::weekly(Weekday::Mon, 8, 99).is_err());
    assert!(Schedule::every_day_at(23, 59).is_ok());
}

#[test]
fn every_counts_from_previous_fire_not_from_now() {
    let s = Schedule::every(Duration::minutes(10)).unwrap();
    let t1 = s.next_fire(dt("2026-10-13 10:00:00"));
    assert_eq!(s.next_fire(t1), dt("2026-10-13 10:20:00"));
}
