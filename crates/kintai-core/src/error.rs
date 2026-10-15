//! 部品の誤り。

use std::fmt;

/// 日付・時点・区間を作れなかった理由。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CoreError {
    /// 表記が `YYYY-MM-DD` または `YYYY-MM-DDTHH:MM`（区切りは `T` か空白）の形でない。
    InvalidFormat,
    /// 暦にない日付（2月30日、13月など）。
    InvalidDate,
    /// 時刻が範囲外（25時、60分など）。
    InvalidTime,
    /// 24:00 の表記。翌日の 0:00 と書く。
    Hour24,
    /// 範囲の外（入力は 2000-01-01〜2099-12-31、計算の結果は 1900-01-01〜2200-12-31）。
    OutOfRange,
    /// 区間の開始が終了以上。
    EmptyInterval,
    /// 月の起算日が 1〜28 の外。
    InvalidMonthStartDay,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            CoreError::InvalidFormat => {
                "表記が YYYY-MM-DD または YYYY-MM-DDTHH:MM の形ではありません"
            }
            CoreError::InvalidDate => "暦にない日付です",
            CoreError::InvalidTime => "時刻が範囲外です（0:00〜23:59）",
            CoreError::Hour24 => "24:00 は使えません。翌日の 0:00 と書いてください",
            CoreError::OutOfRange => {
                "日付が範囲の外です（入力は 2000-01-01〜2099-12-31、計算の結果は 1900-01-01〜2200-12-31）"
            }
            CoreError::EmptyInterval => "区間の開始が終了以上です",
            CoreError::InvalidMonthStartDay => "月の起算日は 1〜28 です",
        };
        f.write_str(text)
    }
}

impl std::error::Error for CoreError {}
