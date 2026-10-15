//! kintai のコマンドライン（report・check・leave・explain・retention・case・demo）。仕様は `docs/spec/cli.md`。
//!
//! 出力は標準出力、エラーは標準エラーに `{"error": {"code": ..., "message": ...}}`。
//! 終了コードは 0（成功）・1（`check --fail-on-violation` で違反か警告）・2（不正な入力）・3（対応外）・
//! 4（曖昧さか打刻の異常による確定集計の拒否）。

mod args;
mod case;
mod case36;
mod case_leave;
mod check;
mod common_out;
mod demo;
mod dto;
mod explain;
mod failure;
mod inputs;
mod leave;
mod policy_file;
mod report;
mod retention;
mod strict_json;
mod text;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use args::Args;
use failure::Failure;
use text::CommandOutput;

/// 版と git の短いハッシュ（取れなければ `unknown`）。
const VERSION_LINE: &str = concat!(
    "kintai ",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("KINTAI_GIT_HASH"),
    ")\n"
);

const HELP: &str = "kintai: 日本の労働基準法に基づく勤怠の計算（Phase 0。結果は専門家の確認前の参考値）

使い方:
  kintai report --policy P --punches C --month YYYY-MM [--data-from D] [--employee ID]
                [--format text|json|csv]
      月の期間ごとの台帳の項目（労働日数・労働時間・延長・60時間超・休日・深夜・法定内）、
      曖昧さの記録、休憩の不足
  kintai check --policy P --punches C --month YYYY-MM [--data-from D] [--history H]
               [--fail-on-violation]
      36協定の判定、休憩の不足、打刻の異常（--fail-on-violation で、あれば終了コード1）
  kintai leave --policy P --leaves L --as-of YYYY-MM-DD [--employee ID] [--format text|json]
      年休のロット、残数、年5日の取得義務、管理簿
  kintai explain --policy P --punches C --employee ID --date YYYY-MM-DD [--data-from D]
                 [--format text|json]
      その日の区間ごとの区分、割増の下限（%）、根拠の条文、表の確認日
  kintai retention --kind wage-ledger|attendance|status-record|leave-register
                   --completed YYYY-MM-DD --wage-due YYYY-MM-DD
      保存の期限の目安
  kintai case FILE
      ケースのJSON（kintai.case.v1・kintai.case36.v1・kintai.caseleave.v1）を計算して、結果のJSONを1行で出す
  kintai demo [--out DIR]
      合成データを DIR（省略時は新しい一時フォルダ）に書き、上の各コマンドの出力を一通り出す
  kintai --version     版と git の短いハッシュ
  kintai --help        この説明

--data-from D は、打刻がそろっている最初の日（report・check・explain。既定は打刻CSV全体の最初の
日付）。打刻CSVが週の途中から始まると、その週は IncompleteWeek で拒否されるので、週の起算日から
打刻を含めるか、--data-from でその日からそろっていることを伝えます。
方針ファイル（P）は TOML で、書き方は docs/spec/cli.md の「方針ファイル」。
打刻CSV（C）の列は employee_id,timestamp,kind、休暇CSV（L）は employee_id,date,unit、
履歴CSV（H）は employee_id,month_start,total_minutes。

終了コード:
  0 成功 / 1 違反か警告（check --fail-on-violation）/ 2 不正な入力 / 3 対応外の働き方 /
  4 曖昧さか打刻の異常による確定集計の拒否
エラーは標準エラーに {\"error\": {\"code\": ..., \"message\": ...}} の形で出します。
";

/// 1回の実行の結果。
struct Outcome {
    stdout: String,
    stderr: String,
    exit: u8,
}

impl Outcome {
    fn ok(stdout: impl Into<String>) -> Outcome {
        Outcome {
            stdout: stdout.into(),
            stderr: String::new(),
            exit: 0,
        }
    }

    fn failed(failure: &Failure) -> Outcome {
        Outcome {
            stdout: String::new(),
            stderr: failure.to_json_line(),
            exit: failure.exit,
        }
    }

    fn from_result(result: Result<CommandOutput, Failure>) -> Outcome {
        match result {
            Ok(output) => Outcome {
                stdout: output.stdout,
                stderr: String::new(),
                exit: output.exit,
            },
            Err(failure) => Outcome::failed(&failure),
        }
    }
}

fn run_case(mut args: pico_args::Arguments) -> Outcome {
    let Ok(path) = args.free_from_str::<PathBuf>() else {
        return Outcome::failed(&Failure::invalid(
            "ファイルを指定してください（kintai case FILE）",
        ));
    };
    if !args.finish().is_empty() {
        return Outcome::failed(&Failure::invalid("余分な引数があります"));
    }
    match case::run_file(&path) {
        Ok(output) => Outcome {
            stdout: output.stdout,
            stderr: String::new(),
            exit: output.exit,
        },
        Err(failure) => Outcome::failed(&failure),
    }
}

fn run(args: Vec<OsString>) -> Outcome {
    let mut args = pico_args::Arguments::from_vec(args);
    if args.contains(["-V", "--version"]) {
        return Outcome::ok(VERSION_LINE);
    }
    if args.contains(["-h", "--help"]) {
        return Outcome::ok(HELP);
    }
    let command = match args.subcommand() {
        Ok(Some(command)) => command,
        Ok(None) => {
            return Outcome::failed(&Failure::invalid("コマンドがありません（kintai --help）"));
        }
        Err(_) => return Outcome::failed(&Failure::invalid("引数を読めません")),
    };
    let result = match command.as_str() {
        "case" => return run_case(args),
        "report" => {
            report::ReportArgs::parse(Args::new(args)).and_then(|parsed| report::run(&parsed))
        }
        "check" => check::CheckArgs::parse(Args::new(args)).and_then(|parsed| check::run(&parsed)),
        "leave" => leave::LeaveArgs::parse(Args::new(args)).and_then(|parsed| leave::run(&parsed)),
        "explain" => {
            explain::ExplainArgs::parse(Args::new(args)).and_then(|parsed| explain::run(&parsed))
        }
        "retention" => retention::RetentionArgs::parse(Args::new(args))
            .and_then(|parsed| retention::run(&parsed)),
        "demo" => demo::DemoArgs::parse(Args::new(args)).and_then(|parsed| demo::run(&parsed)),
        _ => Err(Failure::invalid("知らないコマンドです（kintai --help）")),
    };
    Outcome::from_result(result)
}

fn main() -> ExitCode {
    let outcome = run(std::env::args_os().skip(1).collect());
    // 書けなかったとき（パイプが閉じたなど）も、終了コードは計算の結果のままにする。
    let _ = std::io::stdout().write_all(outcome.stdout.as_bytes());
    let _ = std::io::stderr().write_all(outcome.stderr.as_bytes());
    ExitCode::from(outcome.exit)
}
