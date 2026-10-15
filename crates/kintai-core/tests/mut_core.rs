//! 変異試験（`tools/mutations/core.json`）で生き残った誤りを赤くする試験。
//!
//! AC-02.2・AC-02.6: 時点の型の両端（1900-01-01T00:00 と 2200-12-31T23:59）は、分からも、
//! 分の加減からも作れる。それまでの試験は、端の1分外が作れないことだけを見ていた。

use kintai_core::{CoreError, Date, Instant};

/// AC-02.6 型の最後の分（2200-12-31T23:59）と最初の分（1900-01-01T00:00）は作れて、その外は作れない。
#[test]
fn ac02_6_instant_type_edges_are_inside_the_range() {
    assert_eq!(
        Instant::from_minutes(Instant::MAX.minutes()),
        Ok(Instant::MAX)
    );
    assert_eq!(
        Instant::from_minutes(Instant::MIN.minutes()),
        Ok(Instant::MIN)
    );
    assert_eq!(
        Instant::from_minutes(Instant::MAX.minutes() + 1),
        Err(CoreError::OutOfRange)
    );
    assert_eq!(
        Instant::from_minutes(Instant::MIN.minutes() - 1),
        Err(CoreError::OutOfRange)
    );
    // 加減で端にちょうど届く。
    let before_max = Instant::new(Date::MAX, 1438).unwrap();
    assert_eq!(before_max.checked_add_minutes(1), Ok(Instant::MAX));
    assert_eq!(Instant::MAX.checked_add_minutes(-1), Ok(before_max));
    let after_min = Instant::new(Date::MIN, 1).unwrap();
    assert_eq!(after_min.checked_add_minutes(-1), Ok(Instant::MIN));
    // 入力の最後の分から、型の最後の分まで、分の加算で届く（計算の結果は2200年まで持てる）。
    let span = Instant::MAX.minutes_since(Instant::INPUT_MAX);
    assert_eq!(
        Instant::INPUT_MAX.checked_add_minutes(span),
        Ok(Instant::MAX)
    );
    assert_eq!(Instant::MAX.to_string(), "2200-12-31T23:59");
    assert_eq!(Instant::new(Date::MAX, 1439), Ok(Instant::MAX));
}
