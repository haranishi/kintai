//! AC-02.5: 日付と時点の表記が往復で一致し、不正な表記（2月30日、25時、24:00、60分）を拒否する。
//! AC-02.6: 入力の範囲外の日付（2000年より前、2099年より後）を拒否する。

use kintai_core::{CoreError, Date, Instant};
use proptest::prelude::*;

/// AC-02.5 全日付で、表示した文字列を読むと元に戻り、読んだ文字列を表示すると元に戻る。
#[test]
fn ac02_5_every_date_round_trips() {
    for n in Date::MIN.day_number()..=Date::MAX.day_number() {
        let date = Date::from_day_number(n).unwrap();
        let text = date.to_string();
        assert_eq!(text.len(), 10);
        if date.is_input_range() {
            let parsed: Date = text.parse().unwrap();
            assert_eq!(parsed, date);
            assert_eq!(parsed.to_string(), text);
            assert_eq!(Date::parse(&text), Ok(date));
        } else {
            // 入力の範囲の外は、表記はできるが、入力としては読まない。
            assert_eq!(text.parse::<Date>(), Err(CoreError::OutOfRange), "{text}");
        }
    }
}

proptest! {
    /// AC-02.5 時点の表記が往復で一致する（`T` 区切り）。空白区切りも同じ時点として読む。
    #[test]
    fn ac02_5_instant_round_trips(
        minutes in Instant::INPUT_MIN.minutes()..=Instant::INPUT_MAX.minutes(),
    ) {
        let instant = Instant::from_minutes(minutes).unwrap();
        let text = instant.to_string();
        prop_assert_eq!(text.len(), 16);
        prop_assert_eq!(Instant::parse(&text), Ok(instant));
        let parsed: Instant = text.parse().unwrap();
        prop_assert_eq!(parsed, instant);
        prop_assert_eq!(parsed.to_string(), text.clone());
        let spaced = text.replacen('T', " ", 1);
        prop_assert_eq!(spaced.parse::<Instant>().unwrap(), instant);
    }

    /// AC-02.5 任意の文字列を読んでもパニックせず、読めたものは表示と往復で一致する。
    #[test]
    fn ac02_5_arbitrary_text_never_panics(text in "\\PC{0,20}") {
        if let Ok(date) = text.parse::<Date>() {
            prop_assert_eq!(date.to_string(), text.clone());
        }
        if let Ok(instant) = text.parse::<Instant>() {
            prop_assert_eq!(instant.to_string(), text.replacen(' ', "T", 1));
        }
    }
}

/// AC-02.5 不正な日付の表記を拒否する。
#[test]
fn ac02_5_invalid_dates_are_rejected() {
    let cases = [
        ("2026-02-30", CoreError::InvalidDate),
        ("2026-02-29", CoreError::InvalidDate),
        ("2026-04-31", CoreError::InvalidDate),
        ("2026-13-01", CoreError::InvalidDate),
        ("2026-00-10", CoreError::InvalidDate),
        ("2026-10-00", CoreError::InvalidDate),
        ("2026-10-7", CoreError::InvalidFormat),
        ("2026/10/07", CoreError::InvalidFormat),
        ("20261007", CoreError::InvalidFormat),
        (" 2026-10-07", CoreError::InvalidFormat),
        ("2026-10-07 ", CoreError::InvalidFormat),
        ("2026-1a-07", CoreError::InvalidFormat),
        ("+026-10-07", CoreError::InvalidFormat),
        ("２０２６-10-07", CoreError::InvalidFormat),
        ("", CoreError::InvalidFormat),
    ];
    for (text, error) in cases {
        assert_eq!(text.parse::<Date>(), Err(error), "{text}");
    }
    assert_eq!("2024-02-29".parse::<Date>().unwrap().day(), 29);
}

/// AC-02.5 不正な時点の表記（25時、24:00、60分、秒つき、区切りの誤り）を拒否する。
#[test]
fn ac02_5_invalid_instants_are_rejected() {
    let cases = [
        ("2026-10-07T25:00", CoreError::InvalidTime),
        ("2026-10-07T24:00", CoreError::Hour24),
        ("2026-10-07 24:00", CoreError::Hour24),
        ("2026-10-07T24:30", CoreError::InvalidTime),
        ("2026-10-07T09:60", CoreError::InvalidTime),
        ("2026-10-07T99:99", CoreError::InvalidTime),
        ("2026-02-30T09:00", CoreError::InvalidDate),
        ("2026-10-07T09:00:00", CoreError::InvalidFormat),
        ("2026-10-07t09:00", CoreError::InvalidFormat),
        ("2026-10-07_09:00", CoreError::InvalidFormat),
        ("2026-10-07T9:00", CoreError::InvalidFormat),
        ("2026-10-07T09-00", CoreError::InvalidFormat),
        ("2026-10-07", CoreError::InvalidFormat),
        ("2026-10-07T09:0a", CoreError::InvalidFormat),
        ("2026-10-0７T09:00", CoreError::InvalidFormat),
        ("2026-10-07T09:00Z", CoreError::InvalidFormat),
        ("2026-10-07T09:00+09:00", CoreError::InvalidFormat),
    ];
    for (text, error) in cases {
        assert_eq!(text.parse::<Instant>(), Err(error), "{text}");
    }
    // 24:00 ではなく翌日の 0:00 と書けば読める。
    let midnight: Instant = "2026-10-08T00:00".parse().unwrap();
    assert_eq!(midnight.minute_of_day(), 0);
}

/// AC-02.6 入力の範囲外の日付を含む時点の表記を拒否する（2100-01-01T00:00 も範囲外）。
#[test]
fn ac02_6_out_of_range_instants_are_rejected() {
    for text in [
        "1999-12-31T23:59",
        "2100-01-01T00:00",
        "1970-01-01T00:00",
        "2100-01-01 00:00",
        "2200-12-31T23:59",
    ] {
        assert_eq!(
            text.parse::<Instant>(),
            Err(CoreError::OutOfRange),
            "{text}"
        );
        assert_eq!(Instant::parse(text), Err(CoreError::OutOfRange), "{text}");
    }
    assert_eq!(Instant::parse("2000-01-01T00:00"), Ok(Instant::INPUT_MIN));
    assert_eq!(Instant::parse("2099-12-31T23:59"), Ok(Instant::INPUT_MAX));
    assert!(Instant::INPUT_MIN.is_input_range());
    assert!(Instant::INPUT_MAX.is_input_range());
    let after = Instant::INPUT_MAX.checked_add_minutes(1).unwrap();
    assert!(!after.is_input_range());
    assert_eq!(after.to_string(), "2100-01-01T00:00");
}

/// 誤りの説明文が、すべての種類で空でない。
#[test]
fn error_messages_are_not_empty() {
    for error in [
        CoreError::InvalidFormat,
        CoreError::InvalidDate,
        CoreError::InvalidTime,
        CoreError::Hour24,
        CoreError::OutOfRange,
        CoreError::EmptyInterval,
        CoreError::InvalidMonthStartDay,
    ] {
        assert_ne!(error.to_string(), "");
        let _: &dyn std::error::Error = &error;
    }
}
