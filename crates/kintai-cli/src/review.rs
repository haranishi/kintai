//! Offline monthly review. Reuses the JSON report; never recalculates in JavaScript.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::args::{Args, Format, required};
use crate::failure::Failure;
use crate::report::{self, ReportArgs};
use crate::text::CommandOutput;

/// Arguments for an exclusively created local HTML report.
pub struct ReviewArgs {
    report: ReportArgs,
    out: PathBuf,
}

impl ReviewArgs {
    /// Parse review options.
    ///
    /// # Errors
    /// Missing output or invalid report arguments.
    pub fn parse(mut raw: Args) -> Result<Self, Failure> {
        let out = required(raw.path("--out")?, "--out")?;
        if raw.text("--format")?.is_some() {
            return Err(Failure::invalid(
                "review の出力は HTML です（--format は使いません）",
            ));
        }
        let mut report = ReportArgs::parse(raw)?;
        report.format = Format::Json;
        Ok(Self { report, out })
    }
}

/// Escape JSON before embedding in an HTML raw-text element. AC-14.3.
pub fn embedded_json(json: &str) -> String {
    json.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Create a portable, private file from the existing report. AC-14.1, AC-14.2.
///
/// # Errors
/// Calculation/input failures or an existing/unwritable destination.
pub fn run(args: &ReviewArgs) -> Result<CommandOutput, Failure> {
    let report = report::run(&args.report)?;
    let html = include_str!("review/index.html")
        .replace("/* KINTAI_STYLES */", include_str!("review/style.css"))
        .replace("/* KINTAI_SCRIPT */", include_str!("review/app.js"))
        .replace("KINTAI_REPORT_JSON", &embedded_json(&report.stdout));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&args.out).map_err(|_| {
        Failure::invalid(
            "HTML を新規作成できません。既存のファイルを避け、保存先の権限を確認してください",
        )
    })?;
    if file
        .write_all(html.as_bytes())
        .and_then(|()| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = std::fs::remove_file(&args.out);
        return Err(Failure::invalid(
            "HTML を保存できません。空き容量と保存先の権限を確認してください",
        ));
    }
    Ok(CommandOutput {
        stdout: "月次の確認画面を保存しました。ブラウザで開いてください。勤怠情報を含むため、公開しないでください。\n".to_owned(),
        exit: report.exit,
    })
}

#[cfg(test)]
mod tests {
    /// AC-14.3: raw-text HTML delimiters are escaped without changing the JSON value.
    #[test]
    fn embedded_json_cannot_close_the_script_element() {
        let original =
            serde_json::json!({"text": "</script><script>alert('&')</script>\u{2028}\u{2029}"});
        let escaped = super::embedded_json(&original.to_string());
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert!(!escaped.contains('&'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&escaped).unwrap(),
            original
        );
    }
}
