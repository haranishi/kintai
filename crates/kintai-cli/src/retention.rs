//! `kintai retention`: 保存の期限の目安（`docs/spec/report.md` の「保存の目安」。AC-08.5 を、コマンドから出す）。
//!
//! `statutory_current`（現行の法定の最低）、`statutory_base`（本則）、`product_default`（製品の既定の5年）の
//! 3つの期限を出す。期限は、その日まで保存する最後の日。自動では削除しない。出力は人が読む形だけ
//! （`docs/spec/cli.md` の表に `--format` がない）。先頭の3行は `名前: 日付` だけにして、機械でも読めるようにする
//! （説明は、その後の行。`docs/spec/cli-output.md`）。

use kintai_core::Date;
use kintai_jp_labor::Provenance;
use kintai_jp_labor::report::{Retention, RetentionKind, retention};

use crate::args::{self, Args, required};
use crate::failure::Failure;
use crate::text::{CommandOutput, EXIT_OK, footer, line};

/// `kintai retention` の引数。
#[derive(Clone, Copy, Debug)]
pub struct RetentionArgs {
    /// 記録の種類。
    pub kind: RetentionKind,
    /// 完了の日（賃金台帳は最後の記入をした日、重要な書類は完結の日、年休管理簿は期間の満了の日）。
    pub completed: Date,
    /// その記録に係る賃金の支払期日。
    pub wage_due: Date,
}

impl RetentionArgs {
    /// 引数を読む。
    ///
    /// # Errors
    ///
    /// 必須の引数がない、値の形が違う、余分な引数があるときは不正な入力。
    pub fn parse(mut raw: Args) -> Result<RetentionArgs, Failure> {
        let kind = raw.text("--kind")?;
        let completed = raw.text("--completed")?;
        let wage_due = raw.text("--wage-due")?;
        raw.finish()?;
        let kind = required(kind, "--kind")?;
        Ok(RetentionArgs {
            kind: RetentionKind::from_code(&kind).ok_or_else(|| {
                Failure::invalid(
                    "--kind の値が定義にありません（wage-ledger・attendance・status-record・leave-register）",
                )
            })?,
            completed: args::date(&required(completed, "--completed")?, "--completed")?,
            wage_due: args::date(&required(wage_due, "--wage-due")?, "--wage-due")?,
        })
    }
}

const fn kind_label(kind: RetentionKind) -> &'static str {
    match kind {
        RetentionKind::WageLedger => "賃金台帳",
        RetentionKind::Attendance => "出勤簿・タイムカードなどの重要な書類",
        RetentionKind::StatusRecord => "労働時間の状況の記録",
        RetentionKind::LeaveRegister => "年次有給休暇管理簿",
    }
}

const fn completed_label(kind: RetentionKind) -> &'static str {
    match kind {
        RetentionKind::WageLedger => "最後の記入をした日",
        RetentionKind::Attendance => "完結の日",
        RetentionKind::StatusRecord => "完了の日",
        RetentionKind::LeaveRegister => "期間の満了の日",
    }
}

fn start_text(result: &Retention, args: &RetentionArgs) -> String {
    let completed = completed_label(args.kind);
    if result.start_is_statutory {
        format!(
            "{}（{completed} {} と、賃金の支払期日 {} の遅い方。法令の定めによる）",
            result.start, args.completed, args.wage_due
        )
    } else {
        format!(
            "{}（{completed}。起算日の定めがないので、製品の決めごととして完了の日から数えます。法定の起算日ではありません。--wage-due は使いません）",
            result.start
        )
    }
}

/// `kintai retention` の本体。
///
/// # Errors
///
/// 期限が日付の型の範囲の外になるとき（入力の範囲の日付では起きない）は不正な入力。
pub fn run(args: &RetentionArgs) -> Result<CommandOutput, Failure> {
    let result = retention(args.kind, args.completed, args.wage_due)
        .map_err(|error| Failure::invalid(error.to_string()))?;
    let kind = args.kind;
    let mut out = String::new();
    // 先頭の3行は「名前: 日付」だけにする（`docs/spec/cli-output.md`。機械でも読める。L2-11）。
    line(
        &mut out,
        &format!("statutory_current: {}", result.statutory_current),
    );
    line(
        &mut out,
        &format!("statutory_base: {}", result.statutory_base),
    );
    line(
        &mut out,
        &format!("product_default: {}", result.product_default),
    );
    line(&mut out, "保存の期限の目安（kintai retention）");
    line(
        &mut out,
        &format!("種類（kind）: {}（{}）", kind.code(), kind_label(kind)),
    );
    line(
        &mut out,
        &format!("起算日（start）: {}", start_text(&result, args)),
    );
    line(
        &mut out,
        &format!(
            "statutory_current は現行の法定の最低（当分の間{}年）、statutory_base は本則（{}年）、product_default は製品の既定（{}年）の期限です。",
            kind.statutory_current_years(),
            kind.statutory_base_years(),
            kind.product_default_years()
        ),
    );
    let basis: Vec<String> = kind
        .provenances()
        .iter()
        .map(Provenance::citation)
        .collect();
    line(&mut out, &format!("根拠: {}", basis.join("、")));
    line(
        &mut out,
        "期限は、その日まで保存する最後の日です（民法の期間の数え方: 起算日の翌日から数え、応当日の前日に満了。応当日がなければ月末）。自動では削除しません。",
    );
    if kind == RetentionKind::LeaveRegister {
        line(
            &mut out,
            "年次有給休暇管理簿は、付与の期間中も保存します。ここに出すのは、期間の満了後の期限です。",
        );
    }
    out.push_str(&footer(&[]));
    Ok(CommandOutput {
        stdout: out,
        exit: EXIT_OK,
    })
}
