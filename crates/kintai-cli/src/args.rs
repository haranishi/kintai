//! 引数の読み取り（`pico-args` の薄い包み）。
//!
//! 誤りはどれも不正な入力（終了コード2）。文には引数の名前だけを入れ、値は入れない。
//! 同じ引数を2回書いたときと、知らない引数は「余分な引数」になる（`pico-args` は最初の1つだけを取る）。

use std::convert::Infallible;
use std::ffi::OsStr;
use std::path::PathBuf;

use kintai_core::Date;
use kintai_core::punch::EmployeeId;

use crate::failure::Failure;
use crate::inputs::YearMonth;

/// 出力の形（`--format`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// 人が読む表（既定）。
    Text,
    /// JSON（1行）。
    Json,
    /// CSV（`report` だけ）。
    Csv,
}

impl Format {
    /// `--format` の値。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Json => "json",
            Format::Csv => "csv",
        }
    }
}

/// コマンドの引数（サブコマンドの名前を取り除いたあと）。
pub struct Args {
    inner: pico_args::Arguments,
}

fn argument_error(key: &str, error: &pico_args::Error) -> Failure {
    match error {
        pico_args::Error::OptionWithoutAValue(_) => {
            Failure::invalid(format!("{key} の値がありません"))
        }
        pico_args::Error::NonUtf8Argument => {
            Failure::invalid(format!("{key} の値が UTF-8 ではありません"))
        }
        _ => Failure::invalid(format!("{key} を読めません")),
    }
}

impl Args {
    /// 包む。
    #[must_use]
    pub fn new(inner: pico_args::Arguments) -> Args {
        Args { inner }
    }

    /// 値をとる任意の引数（文字列）。
    ///
    /// # Errors
    ///
    /// 値がない、UTF-8 でないときは不正な入力。
    pub fn text(&mut self, key: &'static str) -> Result<Option<String>, Failure> {
        self.inner
            .opt_value_from_str::<_, String>(key)
            .map_err(|error| argument_error(key, &error))
    }

    /// 値をとる任意の引数（ファイルやフォルダの場所。UTF-8 でなくてよい）。
    ///
    /// # Errors
    ///
    /// 値がないときは不正な入力。
    pub fn path(&mut self, key: &'static str) -> Result<Option<PathBuf>, Failure> {
        self.inner
            .opt_value_from_os_str(key, |value: &OsStr| {
                Ok::<PathBuf, Infallible>(PathBuf::from(value))
            })
            .map_err(|error| argument_error(key, &error))
    }

    /// 値をとらない引数（`--fail-on-violation` など）。
    pub fn flag(&mut self, key: &'static str) -> bool {
        self.inner.contains(key)
    }

    /// 残った引数がないことを確かめる。
    ///
    /// # Errors
    ///
    /// 知らない引数、同じ引数の2回目、余分な値があれば不正な入力。
    pub fn finish(self) -> Result<(), Failure> {
        if self.inner.finish().is_empty() {
            Ok(())
        } else {
            Err(Failure::invalid(
                "余分な引数があります（知らない引数か、同じ引数が2回あります。kintai --help）",
            ))
        }
    }
}

/// 必須の引数がなければ不正な入力。
///
/// # Errors
///
/// `value` が `None` のとき。
pub fn required<T>(value: Option<T>, key: &str) -> Result<T, Failure> {
    value.ok_or_else(|| Failure::invalid(format!("{key} を指定してください（kintai --help）")))
}

/// 日付の引数（`YYYY-MM-DD`、2000-01-01〜2099-12-31）。
///
/// # Errors
///
/// 書式の誤り・暦にない日付・範囲外は不正な入力。
pub fn date(text: &str, key: &str) -> Result<Date, Failure> {
    Date::parse(text).map_err(|error| Failure::invalid(format!("{key}: {error}")))
}

/// 月の引数（`YYYY-MM`、2000-01〜2099-12）。
///
/// # Errors
///
/// 書式の誤り・範囲外は不正な入力。
pub fn month(text: &str, key: &str) -> Result<YearMonth, Failure> {
    YearMonth::parse(text).ok_or_else(|| {
        Failure::invalid(format!(
            "{key} は YYYY-MM の月（2000-01〜2099-12）にしてください"
        ))
    })
}

/// 従業員IDの引数（`[A-Za-z0-9][A-Za-z0-9._-]{0,63}`）。
///
/// # Errors
///
/// 形が違えば不正な入力。
pub fn employee(text: &str, key: &str) -> Result<EmployeeId, Failure> {
    EmployeeId::new(text).map_err(|_| {
        Failure::invalid(format!(
            "{key} の形が違います（英数字で始まり、英数字と . _ - だけの64文字まで）"
        ))
    })
}

/// `--format` の値。`allowed` にない値は不正な入力。省略は `text`。
///
/// # Errors
///
/// 値が `allowed` にないとき。
pub fn format(value: Option<&str>, allowed: &[Format]) -> Result<Format, Failure> {
    let Some(text) = value else {
        return Ok(Format::Text);
    };
    allowed
        .iter()
        .copied()
        .find(|format| format.code() == text)
        .ok_or_else(|| {
            let names: Vec<&str> = allowed.iter().map(|format| format.code()).collect();
            Failure::invalid(format!(
                "--format の値が定義にありません（{}）",
                names.join("・")
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::{Args, Format, format};
    use std::ffi::OsString;

    fn args(list: &[&str]) -> Args {
        Args::new(pico_args::Arguments::from_vec(
            list.iter().map(OsString::from).collect(),
        ))
    }

    #[test]
    fn format_values_and_default() {
        let all = [Format::Text, Format::Json, Format::Csv];
        assert_eq!(format(None, &all).unwrap(), Format::Text);
        assert_eq!(format(Some("csv"), &all).unwrap(), Format::Csv);
        assert!(format(Some("csv"), &[Format::Text, Format::Json]).is_err());
        assert!(format(Some("JSON"), &all).is_err());
    }

    /// AC-12.8 同じ引数の2回目と知らない引数は、不正な入力。値のない引数も。
    #[test]
    fn ac12_8_extra_and_valueless_arguments_are_invalid() {
        let mut twice = args(&["--policy", "a", "--policy", "b"]);
        assert!(twice.path("--policy").unwrap().is_some());
        assert_eq!(twice.finish().unwrap_err().exit, 2);
        let mut unknown = args(&["--polcy", "a"]);
        assert!(unknown.path("--policy").unwrap().is_none());
        assert!(unknown.finish().is_err());
        let mut valueless = args(&["--month"]);
        assert_eq!(valueless.text("--month").unwrap_err().code, "InvalidInput");
    }
}
