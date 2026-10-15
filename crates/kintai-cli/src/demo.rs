//! `kintai demo [--out DIR]`: 合成データを書き、各コマンドの出力を一通り出す（`docs/spec/cli.md`。AC-12.7・AC-10.8）。
//!
//! 合成データは `tests/fixtures/synthetic/` の4つのファイル（試験と `docs/getting-started.md` が使うもの）を、
//! そのまま埋め込む。実在の人・会社は使わない（従業員IDは `E001` 〜 `E005`）。
//!
//! 書くのは `--out` のフォルダ（存在しないか空のフォルダだけ）か、省略時は新しく作る一時フォルダの中だけ。
//! Unix系では、作るフォルダを0700、ファイルを0600にする。既にあるファイルは上書きしない（新しく作るだけ）。
//! 一時フォルダの場所は、OS の一時フォルダ（Unix系では環境変数 `TMPDIR`）の下。計算には使わない。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use kintai_core::Date;
use kintai_core::punch::EmployeeId;
use kintai_jp_labor::report::RetentionKind;

use crate::args::{Args, Format};
use crate::check::CheckArgs;
use crate::explain::ExplainArgs;
use crate::failure::Failure;
use crate::inputs::YearMonth;
use crate::leave::LeaveArgs;
use crate::report::ReportArgs;
use crate::retention::RetentionArgs;
use crate::text::{CommandOutput, EXIT_OK, REFERENCE_LINE, line};

/// 方針ファイル（合成）。
pub const POLICY: &str = include_str!("../../../tests/fixtures/synthetic/policy.toml");
/// 打刻CSV（合成）。
pub const PUNCHES: &str = include_str!("../../../tests/fixtures/synthetic/punches.csv");
/// 休暇CSV（合成）。
pub const LEAVES: &str = include_str!("../../../tests/fixtures/synthetic/leaves.csv");
/// 36協定の履歴CSV（合成）。
pub const HISTORY: &str = include_str!("../../../tests/fixtures/synthetic/history.csv");

/// 書くファイル（名前、中身、説明）。
pub const FILES: [(&str, &str, &str); 4] = [
    (
        "policy.toml",
        POLICY,
        "方針（5人。打刻は 2026-09-27〜10-31）",
    ),
    ("punches.csv", PUNCHES, "打刻"),
    ("leaves.csv", LEAVES, "年休の取得"),
    ("history.csv", HISTORY, "36協定の履歴（2026-05〜09 の T）"),
];

/// 一時フォルダの名前を試す回数。
const TEMP_ATTEMPTS: u32 = 1000;

/// `kintai demo` の引数。
#[derive(Clone, Debug)]
pub struct DemoArgs {
    /// 書くフォルダ（省略時は新しい一時フォルダ）。
    pub out: Option<PathBuf>,
}

impl DemoArgs {
    /// 引数を読む。
    ///
    /// # Errors
    ///
    /// 値のない `--out` や余分な引数は不正な入力。
    pub fn parse(mut raw: Args) -> Result<DemoArgs, Failure> {
        let out = raw.path("--out")?;
        raw.finish()?;
        Ok(DemoArgs { out })
    }
}

/// フォルダを作る（Unix系では0700）。
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// ファイルを新しく作って書く（Unix系では0600。既にあれば失敗する）。
fn write_private_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.flush()
}

/// `--out` のフォルダを用意する。存在しなければ作り、あれば空のフォルダであることを確かめる。
/// シンボリックリンクはたどらない（フォルダでないものとして拒否する）。
fn prepare_out(path: &Path) -> Result<PathBuf, Failure> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            let mut entries =
                fs::read_dir(path).map_err(|_| Failure::invalid("--out のフォルダを読めません"))?;
            if entries.next().is_some() {
                return Err(Failure::invalid(
                    "--out のフォルダが空ではありません（存在しないか空のフォルダを指定してください）",
                ));
            }
            Ok(path.to_path_buf())
        }
        Ok(_) => Err(Failure::invalid("--out がフォルダではありません")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_private_dir(path).map_err(|_| {
                Failure::invalid(
                    "--out のフォルダを作れません（上のフォルダがあるか確かめてください）",
                )
            })?;
            Ok(path.to_path_buf())
        }
        Err(_) => Err(Failure::invalid("--out を確かめられません")),
    }
}

/// OS の一時フォルダの下に、新しいフォルダを作る（名前はプロセス番号と通し番号。乱数は使わない）。
fn create_temp_dir() -> Result<PathBuf, Failure> {
    let base = std::env::temp_dir();
    let process = std::process::id();
    for attempt in 0..TEMP_ATTEMPTS {
        let candidate = base.join(format!("kintai-demo-{process}-{attempt}"));
        match create_private_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => break,
        }
    }
    Err(Failure::invalid(
        "一時フォルダを作れません（--out で書く場所を指定してください）",
    ))
}

/// 動かすコマンド（表示する引数と、動かす関数）。
struct Step {
    shown: &'static str,
    run: fn(&Path) -> Result<CommandOutput, Failure>,
}

fn demo_month() -> Result<YearMonth, Failure> {
    YearMonth::parse("2026-10").ok_or_else(|| Failure::invalid("デモの月を読めません"))
}

fn demo_date(text: &str) -> Result<Date, Failure> {
    Date::parse(text).map_err(|_| Failure::invalid("デモの日付を読めません"))
}

fn report_with(dir: &Path, format: Format) -> Result<CommandOutput, Failure> {
    crate::report::run(&ReportArgs {
        policy: dir.join("policy.toml"),
        punches: dir.join("punches.csv"),
        month: demo_month()?,
        data_from: None,
        employee: None,
        format,
    })
}

fn explain_with(dir: &Path, employee: &str, date: &str) -> Result<CommandOutput, Failure> {
    let employee = EmployeeId::new(employee)
        .map_err(|_| Failure::invalid("デモの従業員IDが正しくありません"))?;
    crate::explain::run(&ExplainArgs {
        policy: dir.join("policy.toml"),
        punches: dir.join("punches.csv"),
        employee,
        date: demo_date(date)?,
        data_from: None,
        format: Format::Text,
    })
}

const STEPS: [Step; 8] = [
    Step {
        shown: "kintai report --policy policy.toml --punches punches.csv --month 2026-10",
        run: |dir| report_with(dir, Format::Text),
    },
    Step {
        shown: "kintai report --policy policy.toml --punches punches.csv --month 2026-10 --format csv",
        run: |dir| report_with(dir, Format::Csv),
    },
    Step {
        shown: "kintai report --policy policy.toml --punches punches.csv --month 2026-10 --format json",
        run: |dir| report_with(dir, Format::Json),
    },
    Step {
        shown: "kintai check --policy policy.toml --punches punches.csv --month 2026-10 --history history.csv",
        run: |dir| {
            crate::check::run(&CheckArgs {
                policy: dir.join("policy.toml"),
                punches: dir.join("punches.csv"),
                month: demo_month()?,
                data_from: None,
                history: Some(dir.join("history.csv")),
                fail_on_violation: false,
            })
        },
    },
    Step {
        shown: "kintai leave --policy policy.toml --leaves leaves.csv --as-of 2026-10-31",
        run: |dir| {
            crate::leave::run(&LeaveArgs {
                policy: dir.join("policy.toml"),
                leaves: dir.join("leaves.csv"),
                as_of: demo_date("2026-10-31")?,
                employee: None,
                format: Format::Text,
            })
        },
    },
    Step {
        shown: "kintai explain --policy policy.toml --punches punches.csv --employee E001 --date 2026-10-08",
        run: |dir| explain_with(dir, "E001", "2026-10-08"),
    },
    Step {
        shown: "kintai explain --policy policy.toml --punches punches.csv --employee E002 --date 2026-10-12",
        run: |dir| explain_with(dir, "E002", "2026-10-12"),
    },
    Step {
        shown: "kintai retention --kind wage-ledger --completed 2026-10-31 --wage-due 2026-11-25",
        run: |_| {
            crate::retention::run(&RetentionArgs {
                kind: RetentionKind::WageLedger,
                completed: demo_date("2026-10-31")?,
                wage_due: demo_date("2026-11-25")?,
            })
        },
    },
];

/// 終了コードの意味（デモの出力に添える）。
const fn exit_meaning(exit: u8) -> &'static str {
    match exit {
        0 => "成功",
        1 => "違反か警告がある（--fail-on-violation）",
        2 => "不正な入力",
        3 => "対応外",
        _ => "曖昧さか打刻の異常で、確定集計を拒否した従業員がいる",
    }
}

/// `kintai demo` の本体。デモ自体の終了コードは、書けたら0（各コマンドの終了コードは出力に示す）。
///
/// # Errors
///
/// フォルダやファイルを作れない、`--out` が空のフォルダでないときは不正な入力。
pub fn run(args: &DemoArgs) -> Result<CommandOutput, Failure> {
    let dir = match &args.out {
        Some(path) => prepare_out(path)?,
        None => create_temp_dir()?,
    };
    for (name, contents, _) in FILES {
        write_private_file(&dir.join(name), contents.as_bytes())
            .map_err(|_| Failure::invalid("デモのファイルを書けません"))?;
    }
    let mut out = String::new();
    line(
        &mut out,
        "kintai demo: 合成データ（実在の人・会社ではありません）を、次のフォルダに書きました。このフォルダの外には書きません。",
    );
    line(&mut out, &format!("  {}", dir.display()));
    for (name, _, description) in FILES {
        line(&mut out, &format!("  {name}: {description}"));
    }
    line(
        &mut out,
        "そのフォルダで、次のコマンドを順に動かした出力です。",
    );
    for step in &STEPS {
        out.push('\n');
        line(&mut out, &format!("$ {}", step.shown));
        let (stdout, exit) = match (step.run)(&dir) {
            Ok(output) => (output.stdout, output.exit),
            Err(failure) => (failure.to_json_line(), failure.exit),
        };
        out.push_str(&stdout);
        line(
            &mut out,
            &format!("（終了コード {exit}: {}）", exit_meaning(exit)),
        );
    }
    // 人が読むどの出力も、最後の1行は参考値の1行（`docs/spec/cli-output.md`。L2-12）。
    out.push('\n');
    line(&mut out, REFERENCE_LINE);
    Ok(CommandOutput {
        stdout: out,
        exit: EXIT_OK,
    })
}

#[cfg(test)]
mod tests {
    use super::{FILES, HISTORY, LEAVES, POLICY, PUNCHES};
    use crate::policy_file::PolicyFile;

    /// 合成データの従業員IDは E001〜E005 だけ（実在の人名・会社名を使わない）。どのファイルも読める形。
    #[test]
    fn demo_data_is_synthetic_and_well_formed() {
        let policy = PolicyFile::parse(POLICY.as_bytes()).unwrap();
        let ids: Vec<String> = policy.employees_by_id().map(|e| e.id.to_string()).collect();
        assert_eq!(ids, ["E001", "E002", "E003", "E004", "E005"]);
        for csv in [PUNCHES, LEAVES, HISTORY] {
            for row in csv.lines().skip(1) {
                let id = row.split(',').next().unwrap();
                assert!(ids.iter().any(|known| known == id), "{row}");
            }
            assert!(csv.ends_with('\n') && !csv.contains('\r'));
        }
        assert!(kintai_import::read_punches(PUNCHES.as_bytes()).is_ok());
        assert!(kintai_import::read_leaves(LEAVES.as_bytes()).is_ok());
        assert!(kintai_import::read_history(HISTORY.as_bytes()).is_ok());
        assert_eq!(FILES.len(), 4);
    }
}
