//! 打刻CSVの取込の速さを測る（AC-10.3: 100万行の合成データを3秒以内。`docs/spec/nfr.md`）。
//!
//! ```text
//! cargo run --release -p kintai-import --example bench_import
//! cargo run --release -p kintai-import --example bench_import -- --runs 11
//! ```
//!
//! 1,000人×250日×4打刻（始業・休憩の開始・休憩の終了・終業）＝100万行の合成データ（UTF-8、LF。
//! 秒あり、区切りは空白と `T` の両方）をメモリの中に作り、取込の時間を測る。測るのは、上限つきの
//! 読み込み（`read_limited`。メモリの中の読み手から）→ `read_punches`（文字コード・行・見出し・
//! 値の検査と変換）の通し。ディスクの読み込みは環境で大きく変わるので含めない。
//! 参考に、打刻の対（`pair`）の時間も出す。
//!
//! 慣らしの1回のあと `--runs` 回（既定7、最小3）測り、最良と中央値を出す。判定は中央値で行い、
//! 目標を超えたら終了コード1。時計は時間の計測にだけ使い、計算には使わない。
//! 測定用のデータを作る関数はこのファイルの中に閉じ込め、ライブラリの公開APIには足さない。

use std::fmt::Write as _;
use std::hint::black_box;
use std::process::ExitCode;
use std::time::{Duration, Instant as Clock};

use kintai_core::Date;
use kintai_core::punch::{SecondsPolicy, pair};
use kintai_import::{read_limited, read_punches};

const EMPLOYEES: u32 = 1_000;
const DAYS: i64 = 250;
/// 目標（マイクロ秒）。
const TARGET_MICROS: u128 = 3_000_000;
/// 測る回数の既定と最小。
const DEFAULT_RUNS: usize = 7;
const MIN_RUNS: usize = 3;

fn synthetic() -> Option<String> {
    let first = Date::from_ymd(2026, 1, 1).ok()?;
    let mut csv = String::with_capacity(40 * 1_000_000);
    csv.push_str("employee_id,timestamp,kind\n");
    for day in 0..DAYS {
        let date = first.checked_add_days(day).ok()?;
        for employee in 0..EMPLOYEES {
            // 人ごとに始業を少しずらし、秒も入れる（取込の手間を実際に近づける）。
            let minute = employee % 60;
            let second = employee % 59;
            writeln!(csv, "E{employee:04},{date} 08:{minute:02}:{second:02},in").ok()?;
            writeln!(csv, "E{employee:04},{date} 12:00:{second:02},break_start").ok()?;
            writeln!(csv, "E{employee:04},{date}T12:59:{second:02},break_end").ok()?;
            writeln!(csv, "E{employee:04},{date} 17:{minute:02}:{second:02},out").ok()?;
        }
    }
    Some(csv)
}

/// `xx.xxx ms` の形（浮動小数点を使わない）。
fn millis(duration: Duration) -> String {
    let micros = duration.as_micros();
    format!("{}.{:03} ms", micros / 1000, micros % 1000)
}

fn best_and_median(mut samples: Vec<Duration>) -> Option<(Duration, Duration)> {
    samples.sort_unstable();
    let best = *samples.first()?;
    let middle = samples.len() / 2;
    let median = if samples.len() % 2 == 1 {
        *samples.get(middle)?
    } else {
        let low = *samples.get(middle.checked_sub(1)?)?;
        let high = *samples.get(middle)?;
        (low + high) / 2
    };
    Some((best, median))
}

/// 慣らしの1回のあと `runs` 回測る。`work` は、最適化で消されないように結果の数を返す。
fn measure(
    runs: usize,
    mut work: impl FnMut() -> Option<usize>,
) -> Option<(Duration, Duration, usize)> {
    let checksum = black_box(work()?);
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Clock::now();
        let value = black_box(work()?);
        samples.push(started.elapsed());
        if value != checksum {
            return None; // 同じ入力で答えが変わった（決定性の破れ）
        }
    }
    let (best, median) = best_and_median(samples)?;
    Some((best, median, checksum))
}

fn parse_runs() -> Result<usize, String> {
    let mut runs = DEFAULT_RUNS;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--runs", Some(value)) => runs = value.parse().map_err(|_| "--runs は整数です")?,
            _ => return Err(format!("知らない引数です: {arg}")),
        }
    }
    if runs < MIN_RUNS {
        return Err(format!("--runs は{MIN_RUNS}以上にしてください"));
    }
    Ok(runs)
}

fn main() -> ExitCode {
    let runs = match parse_runs() {
        Ok(runs) => runs,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let Some(csv) = synthetic() else {
        eprintln!("合成データを作れませんでした");
        return ExitCode::FAILURE;
    };
    let rows = csv.lines().count().saturating_sub(1);
    let Some((best, median, punches)) = measure(runs, || {
        let bytes = read_limited(black_box(csv.as_bytes())).ok()?;
        let imported = read_punches(&bytes).ok()?;
        Some(imported.punches.len())
    }) else {
        eprintln!("取込に失敗しました");
        return ExitCode::FAILURE;
    };
    let Ok(imported) = read_punches(csv.as_bytes()) else {
        eprintln!("取込に失敗しました");
        return ExitCode::FAILURE;
    };
    let Some((pair_best, pair_median, people)) = measure(runs, || {
        let paired = pair(black_box(&imported.punches), SecondsPolicy::WorkerFavorable).ok()?;
        Some(paired.len())
    }) else {
        eprintln!("打刻の対に失敗しました");
        return ExitCode::FAILURE;
    };

    println!("打刻CSVの取込（AC-10.3。docs/spec/nfr.md。合成データ）");
    println!(
        "  {}・{}、{}、慣らし1回のあと{runs}回",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug ビルド（目標の判定には --release で測ること）"
        } else {
            "release ビルド"
        },
    );
    println!(
        "  行数: {rows}（{} バイト、UTF-8・LF）、打刻{punches}件",
        csv.len()
    );
    let target = Duration::from_micros(u64::try_from(TARGET_MICROS).unwrap_or(u64::MAX));
    let within = median.as_micros() <= TARGET_MICROS;
    println!(
        "- 取込（read_limited → read_punches）: 目標 {}、最良 {}、中央値 {} → {}",
        millis(target),
        millis(best),
        millis(median),
        if within {
            "目標以内"
        } else {
            "目標を超えた"
        }
    );
    println!(
        "- 参考 打刻の対（pair、worker_favorable）: 最良 {}、中央値 {}（{people} 人）",
        millis(pair_best),
        millis(pair_median)
    );
    if within {
        println!("判定: 目標以内（中央値で判定）");
        ExitCode::SUCCESS
    } else {
        println!("判定: 目標を超えた");
        ExitCode::FAILURE
    }
}
