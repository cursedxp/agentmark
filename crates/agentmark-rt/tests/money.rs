use agentmark_rt::{no_amounts_changed, extract_money, Currency, Money, MoneyError};
use std::cmp::Ordering;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

#[test]
fn parses_and_displays() {
    let a = m("1200.00 EUR");
    assert_eq!(a.minor, 120000);
    assert_eq!(a.currency, Currency::EUR);
    assert_eq!(a.to_string(), "1200.00 EUR");
    assert_eq!(m("0 EUR").to_string(), "0.00 EUR");
    assert_eq!(m("0 EUR").minor, 0);
    assert_eq!(m("12.5 USD").minor, 1250);
    assert_eq!(m("-5.05 GBP").to_string(), "-5.05 GBP");
    assert_eq!(m("-0.05 CHF").to_string(), "-0.05 CHF");
    assert_eq!(m("1500 JPY").to_string(), "1500 JPY");
    assert_eq!(m("1500 JPY").minor, 1500);
}

#[test]
fn rejects_bad_input() {
    for bad in [
        "", "EUR", "12", "12 ", "12 XYZ", "12.345 EUR", "1.5 JPY", "abc EUR", "1200. EUR", ".5 EUR",
        "1,200.00 EUR", "12.00EUR", "12.00 eur", "--1 EUR", "1 2 EUR", "12.00 EUR extra",
        "99999999999999999999 EUR",
    ] {
        assert!(bad.parse::<Money>().is_err(), "should reject {bad:?}");
    }
}

#[test]
fn serde_as_display_string() {
    let a = m("1200.00 EUR");
    assert_eq!(serde_json::to_string(&a).unwrap(), "\"1200.00 EUR\"");
    let b: Money = serde_json::from_str("\"450.00 EUR\"").unwrap();
    assert_eq!(b.minor, 45000);
    assert!(serde_json::from_str::<Money>("450").is_err());
    assert!(serde_json::from_str::<Money>("\"nonsense\"").is_err());
}

#[test]
fn add_sub_checked() {
    assert_eq!(m("1.10 EUR").checked_add(m("2.25 EUR")).unwrap(), m("3.35 EUR"));
    assert_eq!(m("1.10 EUR").checked_sub(m("2.25 EUR")).unwrap(), m("-1.15 EUR"));
    assert_eq!(
        m("1 EUR").checked_add(m("1 USD")),
        Err(MoneyError::CurrencyMismatch { left: Currency::EUR, right: Currency::USD })
    );
    assert!(m("1 EUR").checked_sub(m("1 USD")).is_err());
    let max = Money::new(i64::MAX, Currency::EUR);
    assert_eq!(max.checked_add(m("0.01 EUR")), Err(MoneyError::Overflow));
}

#[test]
fn comparison_only_within_currency() {
    assert_eq!(m("1 EUR").try_cmp(&m("2 EUR")).unwrap(), Ordering::Less);
    assert_eq!(m("2 EUR").try_cmp(&m("2.00 EUR")).unwrap(), Ordering::Equal);
    assert!(m("1 EUR").try_cmp(&m("1 USD")).is_err());
    assert_eq!(m("1 EUR").partial_cmp(&m("1 USD")), None);
    assert!(m("3 EUR") > m("2 EUR"));
    assert!(!(m("3 EUR") > m("2 USD")));
    assert!(!(m("3 EUR") < m("2 USD")));
    assert_ne!(m("1 EUR"), m("1 USD"));
}

#[test]
fn mul_int_and_negatives() {
    assert_eq!(m("12.34 EUR").mul_int(3).unwrap(), m("37.02 EUR"));
    assert_eq!(m("12.34 EUR").mul_int(-2).unwrap(), m("-24.68 EUR"));
    assert_eq!(Money::new(i64::MAX, Currency::EUR).mul_int(2), Err(MoneyError::Overflow));
}

#[test]
fn div_int_rounds_half_away_from_zero() {
    // 1.00 / 3 = 0.333.. -> 0.33 ; 2.00 / 3 = 0.666.. -> 0.67
    assert_eq!(m("1.00 EUR").div_int(3).unwrap(), m("0.33 EUR"));
    assert_eq!(m("2.00 EUR").div_int(3).unwrap(), m("0.67 EUR"));
    // exact halves: 0.05 / 2 = 0.025 -> 0.03 (away from zero), 0.01/2 -> 0.01
    assert_eq!(m("0.05 EUR").div_int(2).unwrap(), m("0.03 EUR"));
    assert_eq!(m("0.01 EUR").div_int(2).unwrap(), m("0.01 EUR"));
    // negatives round away from zero too
    assert_eq!(m("-0.05 EUR").div_int(2).unwrap(), m("-0.03 EUR"));
    assert_eq!(m("0.05 EUR").div_int(-2).unwrap(), m("-0.03 EUR"));
    assert_eq!(m("-1.00 EUR").div_int(3).unwrap(), m("-0.33 EUR"));
    assert_eq!(m("-2.00 EUR").div_int(3).unwrap(), m("-0.67 EUR"));
    assert_eq!(m("-0.05 EUR").div_int(-2).unwrap(), m("0.03 EUR"));
    // just below half rounds down
    assert_eq!(m("0.04 EUR").div_int(3).unwrap(), m("0.01 EUR"));
    // exact division stays exact
    assert_eq!(m("10.00 EUR").div_int(4).unwrap(), m("2.50 EUR"));
    assert_eq!(m("1.00 EUR").div_int(0), Err(MoneyError::DivisionByZero));
}

#[test]
fn mul_div_percentage_without_intermediate_overflow() {
    // 1200.00 * 19 / 100 = 228.00
    assert_eq!(m("1200.00 EUR").mul_div(19, 100).unwrap(), m("228.00 EUR"));
    // 0.05 * 1 / 2 rounds away from zero
    assert_eq!(m("0.05 EUR").mul_div(1, 2).unwrap(), m("0.03 EUR"));
    // intermediate would overflow i64 but result fits
    let big = Money::new(i64::MAX / 2, Currency::EUR);
    assert_eq!(big.mul_div(4, 4).unwrap(), big);
    assert_eq!(m("1 EUR").mul_div(1, 0), Err(MoneyError::DivisionByZero));
}

#[test]
fn extracts_money_from_text() {
    let got = extract_money("Invoice for 1200.00 EUR due; pay 50 USD or -3.5 GBP. Date 2026-09-01. 12 EURO no. 1,5 EUR");
    // "1,5 EUR" is ambiguous (decimal comma?) so it is not extracted; the guard fails closed on it
    assert_eq!(got, vec![m("1200.00 EUR"), m("50 USD"), m("-3.50 GBP")]);
    let got = extract_money("EUR 1300, \u{20ac}1,300.00 and $5 and 1.300,00 EUR and 7 \u{a3}");
    assert_eq!(got, vec![m("1300 EUR"), m("1300.00 EUR"), m("5 USD"), m("7 GBP")]);
}

#[test]
fn guard_no_amounts_changed() {
    let input = serde_json::json!({
        "id": "INV-1", "amount": "1200.00 EUR", "nested": [{"fee": "5 EUR"}]
    });
    assert!(no_amounts_changed("Your invoice for 1200.00 EUR is due.", &input));
    assert!(no_amounts_changed("Amount 1200 EUR plus fee 5.00 EUR.", &input));
    assert!(no_amounts_changed("No amounts at all.", &input));
    assert!(!no_amounts_changed("Your invoice for 1000.00 EUR is due.", &input));
    assert!(!no_amounts_changed("Pay 1200.00 USD", &input));
}

#[test]
fn guard_fails_closed() {
    let input = serde_json::json!({"amount": "1200.00 EUR", "small": "300.00 EUR"});
    assert!(!no_amounts_changed("Please pay \u{20ac}1,300.00 soon", &input));
    assert!(!no_amounts_changed("Please pay EUR 1300 soon", &input));
    assert!(!no_amounts_changed("Please pay 1,300.00 EUR soon", &input)); // one token 1300.00, not 300.00
    assert!(!no_amounts_changed("Please pay 1200.005 EUR soon", &input));
    assert!(!no_amounts_changed("Please pay 1,5 EUR soon", &input)); // ambiguous separator
    assert!(!no_amounts_changed("Please pay 1.300,00 EUR soon", &input));
    assert!(!no_amounts_changed("Please pay $1200", &input));
    assert!(no_amounts_changed("Please pay \u{20ac}1,200.00 and EUR 300", &input));
    // ids and dates are not amounts
    assert!(no_amounts_changed("Invoice INV-1 due 2026-09-01, ref 12345.", &input));
    let fixture = "Dear M\u{fc}ller GmbH, this is a friendly reminder that invoice INV-1 for 1200.00 EUR was due on 2026-09-01. We would appreciate payment at your earliest convenience.";
    assert!(no_amounts_changed(fixture, &serde_json::json!({"id":"INV-1","amount":"1200.00 EUR","due":"2026-09-01"})));
}
