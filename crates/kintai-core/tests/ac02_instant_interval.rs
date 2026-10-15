//! AC-02.2: 時点の分の加減と、日付＋分への分解・合成が往復で一致する。
//! AC-02.3: 区間の交差と重なりの判定が、境界（端が接する、1分重なる）で正しい。
//! AC-02.4: 区間を暦日の0:00で分けると、各小片が1つの暦日に収まり、合計の長さが元と一致する。

use kintai_core::{CoreError, Date, Instant, Interval, MINUTES_PER_DAY};
use proptest::prelude::*;

fn at(text: &str) -> Instant {
    text.parse().unwrap()
}

fn span(start: &str, end: &str) -> Interval {
    Interval::new(at(start), at(end)).unwrap()
}

fn any_instant() -> impl Strategy<Value = Instant> {
    (Instant::MIN.minutes()..=Instant::MAX.minutes())
        .prop_map(|m| Instant::from_minutes(m).unwrap())
}

proptest! {
    /// AC-02.2 日付＋その日の何分目かへの分解と、そこからの合成が往復で一致する。
    #[test]
    fn ac02_2_decompose_and_compose_round_trip(instant in any_instant()) {
        let rebuilt = Instant::new(instant.date(), instant.minute_of_day()).unwrap();
        prop_assert_eq!(rebuilt, instant);
        let by_hm = Instant::from_hm(instant.date(), instant.hour(), instant.minute()).unwrap();
        prop_assert_eq!(by_hm, instant);
        prop_assert!(instant.minute_of_day() < 1440);
        prop_assert_eq!(instant.hour() * 60 + instant.minute(), instant.minute_of_day());
        prop_assert_eq!(Instant::from_minutes(instant.minutes()).unwrap(), instant);
    }

    /// AC-02.2 分の加減が往復で一致し、範囲を出るときは拒否する。
    #[test]
    fn ac02_2_add_minutes_round_trip(instant in any_instant(), delta in -3_000_000_i64..3_000_000) {
        match instant.checked_add_minutes(delta) {
            Ok(moved) => {
                prop_assert_eq!(moved.minutes_since(instant), delta);
                prop_assert_eq!(moved.checked_add_minutes(-delta).unwrap(), instant);
            }
            Err(error) => {
                prop_assert_eq!(error, CoreError::OutOfRange);
                let target = instant.minutes() + delta;
                prop_assert!(target < Instant::MIN.minutes() || target > Instant::MAX.minutes());
            }
        }
    }

    /// AC-02.4 0:00 で分けた小片は1つの暦日に収まり、隙間なく並び、合計の長さが元と一致する。
    #[test]
    fn ac02_4_split_pieces_fit_one_day_and_sum_up(
        start in Instant::MIN.minutes()..Instant::MAX.minutes() - 10_000,
        len in 1_i64..10_000,
    ) {
        let start = Instant::from_minutes(start).unwrap();
        let end = start.checked_add_minutes(len).unwrap();
        let interval = Interval::new(start, end).unwrap();
        let pieces = interval.split_at_midnight();
        prop_assert!(!pieces.is_empty());
        prop_assert_eq!(pieces.iter().map(|p| p.len_minutes()).sum::<i64>(), len);
        prop_assert_eq!(pieces[0].start(), start);
        prop_assert_eq!(pieces[pieces.len() - 1].end(), end);
        for pair in pieces.windows(2) {
            prop_assert_eq!(pair[0].end(), pair[1].start());
            prop_assert_eq!(pair[1].start().minute_of_day(), 0);
        }
        for piece in &pieces {
            // 最後の分（終了の1分前）が、開始と同じ暦日にある。
            let last = piece.end().checked_add_minutes(-1).unwrap();
            prop_assert_eq!(last.date(), piece.start().date());
        }
    }

    /// AC-02.3 重なり・交差の判定が、分の集合としての交わりと一致する。
    #[test]
    fn ac02_3_overlap_matches_minute_sets(
        a_start in 0_i64..200, a_len in 1_i64..100, b_start in 0_i64..200, b_len in 1_i64..100,
    ) {
        let base = Instant::MIN.minutes();
        let make = |s: i64, l: i64| Interval::new(
            Instant::from_minutes(base + s).unwrap(),
            Instant::from_minutes(base + s + l).unwrap(),
        ).unwrap();
        let a = make(a_start, a_len);
        let b = make(b_start, b_len);
        let common = (a_start..a_start + a_len).filter(|m| (b_start..b_start + b_len).contains(m)).count();
        let common = i64::try_from(common).unwrap();
        prop_assert_eq!(a.overlaps(b), common > 0);
        prop_assert_eq!(a.overlaps(b), b.overlaps(a));
        prop_assert_eq!(a.intersection(b).map_or(0, Interval::len_minutes), common);
        prop_assert_eq!(a.intersection(b), b.intersection(a));
    }
}

/// AC-02.3 端が接する区間は重ならず、1分重なる区間は重なる。
#[test]
fn ac02_3_touching_and_one_minute_overlap() {
    let morning = span("2026-10-07T09:00", "2026-10-07T12:00");
    let touching = span("2026-10-07T12:00", "2026-10-07T13:00");
    let one_minute = span("2026-10-07T11:59", "2026-10-07T13:00");
    assert!(!morning.overlaps(touching));
    assert!(!touching.overlaps(morning));
    assert_eq!(morning.intersection(touching), None);
    assert!(morning.overlaps(one_minute));
    assert_eq!(
        morning.intersection(one_minute),
        Some(span("2026-10-07T11:59", "2026-10-07T12:00"))
    );
    assert_eq!(morning.intersection(morning), Some(morning));
    // 包含
    let inner = span("2026-10-07T10:00", "2026-10-07T10:01");
    assert_eq!(morning.intersection(inner), Some(inner));
    assert!(morning.contains(at("2026-10-07T09:00")));
    assert!(morning.contains(at("2026-10-07T11:59")));
    assert!(!morning.contains(at("2026-10-07T12:00")));
    assert!(!morning.contains(at("2026-10-07T08:59")));
}

/// AC-02.3 開始 < 終了でない区間は作れない。長さは分。
#[test]
fn ac02_3_empty_interval_is_rejected() {
    let t = at("2026-10-07T09:00");
    assert_eq!(Interval::new(t, t), Err(CoreError::EmptyInterval));
    assert_eq!(
        Interval::new(at("2026-10-07T10:00"), t),
        Err(CoreError::EmptyInterval)
    );
    let one = Interval::new(t, at("2026-10-07T09:01")).unwrap();
    assert_eq!(one.len_minutes(), 1);
    assert_eq!(one.start(), t);
    assert_eq!(one.end(), at("2026-10-07T09:01"));
}

/// AC-02.4 24:00（翌日の0:00）に終わる区間は分けない。0:00 をまたぐ区間は2つに分ける。
#[test]
fn ac02_4_split_examples() {
    let to_midnight = span("2026-10-04T09:00", "2026-10-05T00:00");
    assert_eq!(to_midnight.split_at_midnight(), vec![to_midnight]);
    let overnight = span("2026-10-07T22:00", "2026-10-08T05:00");
    assert_eq!(
        overnight.split_at_midnight(),
        vec![
            span("2026-10-07T22:00", "2026-10-08T00:00"),
            span("2026-10-08T00:00", "2026-10-08T05:00"),
        ]
    );
    let three_days = span("2026-12-31T23:00", "2027-01-02T01:00");
    let pieces = three_days.split_at_midnight();
    assert_eq!(pieces.len(), 3);
    assert_eq!(pieces[1], span("2027-01-01T00:00", "2027-01-02T00:00"));
    assert_eq!(pieces[1].len_minutes(), MINUTES_PER_DAY);
    // 入力の範囲の最後の分まで
    let last = Interval::new(at("2099-12-31T23:00"), Instant::INPUT_MAX).unwrap();
    assert_eq!(last.split_at_midnight(), vec![last]);
    // 型の範囲の最後の分まで
    let very_last =
        Interval::new(Instant::MAX.checked_add_minutes(-60).unwrap(), Instant::MAX).unwrap();
    assert_eq!(very_last.split_at_midnight(), vec![very_last]);
}

/// AC-02.2 範囲の端と、作れない時点。
#[test]
fn ac02_2_instant_edges() {
    assert_eq!(Instant::MIN.to_string(), "1900-01-01T00:00");
    assert_eq!(Instant::MAX.to_string(), "2200-12-31T23:59");
    assert_eq!(Instant::INPUT_MIN.to_string(), "2000-01-01T00:00");
    assert_eq!(Instant::INPUT_MAX.to_string(), "2099-12-31T23:59");
    assert_eq!(Instant::INPUT_MIN.date(), Date::INPUT_MIN);
    assert_eq!(Instant::INPUT_MAX.date(), Date::INPUT_MAX);
    assert_eq!(Instant::MIN.date(), Date::MIN);
    assert_eq!(Instant::MAX.date(), Date::MAX);
    assert_eq!(Instant::MAX.minute_of_day(), 1439);
    assert_eq!(
        Instant::MAX.checked_add_minutes(1),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(
        Instant::MIN.checked_add_minutes(-1),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(
        Instant::MIN.checked_add_minutes(i64::MIN),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(
        Instant::from_minutes(Instant::MAX.minutes() + 1),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(Instant::new(Date::MIN, 1440), Err(CoreError::InvalidTime));
    assert_eq!(Instant::from_hm(Date::MIN, 24, 0), Err(CoreError::Hour24));
    assert_eq!(
        Instant::from_hm(Date::MIN, 24, 1),
        Err(CoreError::InvalidTime)
    );
    assert_eq!(
        Instant::from_hm(Date::MIN, 25, 0),
        Err(CoreError::InvalidTime)
    );
    assert_eq!(
        Instant::from_hm(Date::MIN, 9, 60),
        Err(CoreError::InvalidTime)
    );
    let t = Instant::from_hm(Date::from_ymd(2026, 10, 7).unwrap(), 22, 5).unwrap();
    assert_eq!((t.hour(), t.minute(), t.minute_of_day()), (22, 5, 1325));
    assert_eq!(format!("{t:?}"), "Instant(2026-10-07T22:05)");
    assert_eq!(
        t.minutes(),
        Date::from_ymd(2026, 10, 7).unwrap().day_number() * 1440 + 1325
    );
}
