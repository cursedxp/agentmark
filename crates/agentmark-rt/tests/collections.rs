use agentmark_rt::collections::{extreme, sorted, sum_money, unique};
use agentmark_rt::Money;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

#[test]
fn sorting_is_stable_in_both_directions() {
    let keyed = vec![(2, "a"), (1, "b"), (2, "c"), (1, "d")];
    assert_eq!(sorted(keyed.clone(), false).unwrap(), vec!["b", "d", "a", "c"]);
    // equal keys keep their original order also when descending
    assert_eq!(sorted(keyed, true).unwrap(), vec!["a", "c", "b", "d"]);
}

#[test]
fn text_dates_and_durations_have_an_order() {
    assert_eq!(sorted(vec![("b".to_string(), 1), ("a".to_string(), 2)], false).unwrap(), vec![2, 1]);
    let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    assert_eq!(sorted(vec![(d("2026-10-02"), 1), (d("2026-10-01"), 2)], false).unwrap(), vec![2, 1]);
    assert_eq!(sorted(vec![(chrono::Duration::days(2), 1), (chrono::Duration::days(1), 2)], false).unwrap(), vec![2, 1]);
}

#[test]
fn money_in_one_currency_is_ordered_and_summed() {
    assert_eq!(sorted(vec![(m("5.00 EUR"), "b"), (m("1.00 EUR"), "a")], false).unwrap(), vec!["a", "b"]);
    assert_eq!(sum_money(vec![m("1.50 EUR"), m("2.25 EUR")]).unwrap(), m("3.75 EUR"));
}

#[test]
fn an_empty_money_sum_is_zero_euro() {
    assert_eq!(sum_money(vec![]).unwrap(), m("0 EUR"));
}

#[test]
fn two_currencies_are_an_error_not_a_guess() {
    let e = sum_money(vec![m("1.00 EUR"), m("1.00 USD")]).unwrap_err();
    assert!(e.to_string().contains("currency mismatch"), "{e}");
    assert!(sorted(vec![(m("1.00 EUR"), 1), (m("1.00 USD"), 2)], false).is_err());
    assert!(extreme(vec![m("1.00 EUR"), m("2.00 GBP")], true).is_err());
}

#[test]
fn smallest_and_largest_pick_the_first_of_equals() {
    assert_eq!(extreme(vec![3, 1, 2, 1], false).unwrap(), Some(1));
    assert_eq!(extreme(vec![3, 1, 3, 2], true).unwrap(), Some(3));
    assert_eq!(extreme(Vec::<i64>::new(), true).unwrap(), None);
}

#[test]
fn unique_keeps_the_first_occurrence() {
    assert_eq!(unique(vec!["b", "a", "b", "c", "a"]), vec!["b", "a", "c"]);
    assert_eq!(unique(Vec::<i64>::new()), Vec::<i64>::new());
}
