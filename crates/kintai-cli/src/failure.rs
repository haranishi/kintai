//! 失敗（標準エラーに出す JSON と終了コード）。

use kintai_jp_labor::UnsupportedCode;
use serde::Serialize;

/// 不正な入力の終了コード。
pub const EXIT_INVALID: u8 = 2;
/// 対応外の終了コード。
pub const EXIT_UNSUPPORTED: u8 = 3;

/// コマンドの失敗。標準エラーに `{"error": {"code": ..., "message": ...}}` を出す。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// `InvalidInput` か、対応外のコード。
    pub code: &'static str,
    /// 説明。入力の中身（日時や値）は含めない。
    pub message: String,
    /// 終了コード。
    pub exit: u8,
}

#[derive(Serialize)]
struct ErrorOut<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

impl Failure {
    /// 不正な入力（終了コード2）。
    pub fn invalid(message: impl Into<String>) -> Failure {
        Failure {
            code: "InvalidInput",
            message: message.into(),
            exit: EXIT_INVALID,
        }
    }

    /// 対応外（終了コード3）。説明に、働き方と根拠の条文を含める。
    pub fn unsupported(code: UnsupportedCode) -> Failure {
        Failure {
            code: code.code(),
            message: code.to_string(),
            exit: EXIT_UNSUPPORTED,
        }
    }

    /// 標準エラーに出す1行（末尾の改行を含む）。
    pub fn to_json_line(&self) -> String {
        let out = ErrorOut {
            error: ErrorBody {
                code: self.code,
                message: &self.message,
            },
        };
        let mut line = serde_json::to_string(&out)
            .unwrap_or_else(|_| format!(r#"{{"error":{{"code":"{}","message":""}}}}"#, self.code));
        line.push('\n');
        line
    }
}

#[cfg(test)]
mod tests {
    use super::Failure;
    use kintai_jp_labor::UnsupportedCode;

    #[test]
    fn json_line_has_code_and_message() {
        let line = Failure::invalid("テスト \"引用\"").to_json_line();
        assert_eq!(
            line,
            "{\"error\":{\"code\":\"InvalidInput\",\"message\":\"テスト \\\"引用\\\"\"}}\n"
        );
        let flex = Failure::unsupported(UnsupportedCode::Flex);
        assert_eq!((flex.code, flex.exit), ("Flex", 3));
        assert!(flex.message.contains("労働基準法第32条の3"));
    }
}
