//! 区分の計算・36協定の年間判定・起動の速さを測る（`docs/spec/nfr.md` の「速さ」）。
//!
//! ```text
//! cargo build --release -p kintai-cli          # 起動を測るときだけ（kintai の実行ファイル）
//! cargo run --release -p kintai-jp-labor --example bench_jp_labor
//! cargo run --release -p kintai-jp-labor --example bench_jp_labor -- --runs 11 --startup-runs 500 --kintai target/release/kintai
//! ```
//!
//! - AC-10.1: 区分の計算が、1,000人×1か月（約3万日）で100ミリ秒以内。入出力を除く計算だけを測る。
//!   検査済みの勤務の区分（`classify`）と、勤務の検査からの通し（`ShiftSet::new` → `classify`）の
//!   両方を出し、判定は通しの方（厳しい方）で行う。
//! - AC-10.2: 36協定の年間判定が、1,000人×12か月で50ミリ秒以内（`Agreement::validate` → `judge`）。
//! - nfr.md の表の起動（`kintai --version` が50ミリ秒以内）: ビルドした実行ファイルを
//!   `std::process::Command` で多数回動かして測る。実行ファイルがなければ測らずに知らせる。
//!
//! 合成データは決まった式で作る（乱数・時計・環境変数を使わない。同じ入力で毎回同じ）。
//! 時計は時間の計測にだけ使う。各項目を慣らしの1回のあと `--runs` 回（既定7、最小3）測り、
//! 最良と中央値を出す。判定は中央値で行い、どれかが目標を超えたら終了コード1。
//! 測定用のデータを作る関数はこのファイルの中に閉じ込め、ライブラリの公開APIには足さない。

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant as Clock};

use kintai_core::{Date, Instant, Weekday};
use kintai_jp_labor::limits36::{
    Agreement, AgreementKind, HistoryMonth, MonthHours, ValidAgreement, judge,
};
use kintai_jp_labor::{
    AmbiguityMode, ShiftInput, ShiftSet, ValidPolicy, WorkPolicy, WorkSystem, classify,
};

/// 従業員の数。
const EMPLOYEES: u32 = 1_000;
/// 区分の月（2026年10月。1日は木曜）。
const CLASSIFY_YEAR: i32 = 2026;
const CLASSIFY_MONTH: u32 = 10;
const CLASSIFY_DAYS: u32 = 31;
/// 36協定の協定年の起算月と、協定月の数。
const YEAR_START_MONTH: i64 = 4;
const AGREEMENT_MONTHS: u32 = 12;
/// 目標（マイクロ秒）。
const CLASSIFY_TARGET_MICROS: u128 = 100_000;
const LIMITS36_TARGET_MICROS: u128 = 50_000;
const STARTUP_TARGET_MICROS: u128 = 50_000;
/// 測る回数の既定と最小。
const DEFAULT_RUNS: usize = 7;
const MIN_RUNS: usize = 3;
/// 起動を測る回数の既定。
const DEFAULT_STARTUP_RUNS: usize = 200;

/// 1つの項目の測定の結果。
struct Measured {
    label: &'static str,
    target_micros: u128,
    best: Duration,
    median: Duration,
    note: String,
}

impl Measured {
    fn within_target(&self) -> bool {
        self.median.as_micros() <= self.target_micros
    }
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
    mut work: impl FnMut() -> Option<u64>,
) -> Option<(Duration, Duration, u64)> {
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

// ---- 区分の計算（AC-10.1） ----

fn classify_policy() -> Option<ValidPolicy> {
    WorkPolicy {
        work_system: WorkSystem::Standard,
        scheduled_minutes_per_day: 450,
        week_start: Some(Weekday::Sunday),
        weekly_limit_minutes: Some(2400),
        special_44h_confirmed: None,
        legal_holiday_weekday: Some(Weekday::Sunday),
        month_start_day: 1,
        // 曖昧さ（またぐ勤務、打刻のそろっていない週）があっても、計算を最後まで通す。
        ambiguity: Some(AmbiguityMode::UseDefault),
        worker_kind: None,
        holiday_system: None,
        holiday_swaps: 0,
        night_window: None,
    }
    .validate()
    .ok()
}

/// その日の `minute` 分目の時点（`minute` は1440以上でもよい。翌日に進む）。
fn at(day: Date, minute: u32) -> Option<Instant> {
    let base = Instant::new(day, 0).ok()?;
    base.checked_add_minutes(i64::from(minute)).ok()
}

/// 従業員 `e` の、日 `d`（0から）の勤務。休みの日は `None`。
///
/// 5人に3人は日勤（9時台に始まり、休憩1時間、時間外0〜150分）、1人は遅番（13時台から22時台、
/// 深夜を少し含む）、1人は夜勤（22時から翌朝。暦日と週と月をまたぐ）。日勤と遅番は7日に1日休み。
/// 日曜（法定休日）の労働、週40時間の超過、月60時間の超過を含む。
fn shift_of(e: u32, d: u32, day: Date) -> Option<ShiftInput> {
    let extra = ((e * 7 + d * 3) % 6) * 30;
    let offset = (e % 4) * 15;
    let (start, end, rest) = match e % 5 {
        0..=2 => {
            if (d + e).is_multiple_of(7) {
                return None;
            }
            let start = 9 * 60 + offset;
            (start, start + 9 * 60 + extra, (12 * 60, 13 * 60))
        }
        3 => {
            if (d + e) % 7 == 1 {
                return None;
            }
            let start = 13 * 60 + offset;
            (start, start + 8 * 60 + 45 + extra, (17 * 60, 17 * 60 + 45))
        }
        _ => {
            let start = 22 * 60;
            (start, start + 9 * 60 + extra, (26 * 60, 27 * 60))
        }
    };
    Some(ShiftInput {
        start: at(day, start)?,
        end: at(day, end)?,
        breaks: vec![(at(day, rest.0)?, at(day, rest.1)?)],
    })
}

/// 1,000人×31日の勤務の入力（従業員ごと）。
fn classify_inputs() -> Option<Vec<Vec<ShiftInput>>> {
    let first = Date::from_ymd(CLASSIFY_YEAR, CLASSIFY_MONTH, 1).ok()?;
    let mut all = Vec::new();
    for e in 0..EMPLOYEES {
        let mut shifts = Vec::new();
        for d in 0..CLASSIFY_DAYS {
            let day = first.checked_add_days(i64::from(d)).ok()?;
            if let Some(shift) = shift_of(e, d, day) {
                shifts.push(shift);
            }
        }
        all.push(shifts);
    }
    Some(all)
}

/// 区分の結果から、答えの指紋（実労働・時間外・60時間超・深夜・曖昧さの数の和）を作る。
fn fingerprint(result: &kintai_jp_labor::Classification) -> u64 {
    let minutes: i64 = result
        .periods
        .iter()
        .map(|p| p.total + p.overtime + p.over60 + p.holiday + p.night)
        .sum();
    u64::try_from(minutes).unwrap_or(0) + u64::try_from(result.ambiguities.len()).unwrap_or(0)
}

fn bench_classify(runs: usize) -> Option<Vec<Measured>> {
    let policy = classify_policy()?;
    let inputs = classify_inputs()?;
    let data_from = Date::from_ymd(CLASSIFY_YEAR, CLASSIFY_MONTH, 1).ok();
    let sets: Vec<ShiftSet> = inputs
        .iter()
        .map(|shifts| ShiftSet::new(shifts).ok())
        .collect::<Option<_>>()?;
    let shift_count: usize = sets.iter().map(ShiftSet::len).sum();

    let (best, median, checksum) = measure(runs, || {
        let mut sum = 0_u64;
        for set in &sets {
            let result = classify(&policy, black_box(set), data_from).ok()?;
            sum = sum.wrapping_add(fingerprint(&result));
        }
        Some(sum)
    })?;
    let calc = Measured {
        label: "区分の計算（検査済みの勤務 → classify）",
        target_micros: CLASSIFY_TARGET_MICROS,
        best,
        median,
        note: format!("{EMPLOYEES}人×{CLASSIFY_DAYS}日、勤務{shift_count}件、指紋{checksum}"),
    };

    let (best, median, checksum) = measure(runs, || {
        let mut sum = 0_u64;
        for shifts in &inputs {
            let set = ShiftSet::new(black_box(shifts)).ok()?;
            let result = classify(&policy, &set, data_from).ok()?;
            sum = sum.wrapping_add(fingerprint(&result));
        }
        Some(sum)
    })?;
    let through = Measured {
        label: "区分の計算（AC-10.1。勤務の検査 ShiftSet::new → classify）",
        target_micros: CLASSIFY_TARGET_MICROS,
        best,
        median,
        note: format!("{EMPLOYEES}人×{CLASSIFY_DAYS}日、勤務{shift_count}件、指紋{checksum}"),
    };
    Some(vec![calc, through])
}

// ---- 36協定の年間判定（AC-10.2） ----

/// 従業員 `e` の協定。3人に1人ずつ、特別条項つき・一般条項・協定なし。
fn agreement_of(e: u32) -> Agreement {
    let (kind, special) = match e % 3 {
        0 => (AgreementKind::Special, true),
        1 => (AgreementKind::General, false),
        _ => (AgreementKind::None, false),
    };
    Agreement {
        kind,
        year_start_month: YEAR_START_MONTH,
        monthly_limit_minutes: (kind != AgreementKind::None).then_some(2700),
        annual_limit_minutes: (kind != AgreementKind::None).then_some(21_600),
        special_monthly_total_limit_minutes: special.then_some(5_940),
        special_annual_limit_minutes: special.then_some(43_200),
        special_months_max: special.then_some(6),
    }
}

/// 協定年（2026-04〜2027-03）の12か月の O と H。発動月・100時間・平均80時間の境を踏むように変える
/// （O は0〜63時間、H は0〜40時間）。
fn months_of(e: u32) -> Option<Vec<MonthHours>> {
    let first = Date::from_ymd(2026, 4, 1).ok()?;
    let mut months = Vec::new();
    for m in 0..AGREEMENT_MONTHS {
        let year = first.year() + i32::try_from((3 + m) / 12).ok()?;
        let month = (3 + m) % 12 + 1;
        let overtime = i64::from(((e * 37 + m * 211) % 64) * 60);
        let holiday = i64::from(((e + m) % 5) * 600);
        months.push(MonthHours {
            start: Date::from_ymd(year, month, 1).ok()?,
            overtime,
            holiday,
        });
    }
    Some(months)
}

/// `months` より前の5か月（2025-11〜2026-03）の T。7人に1人は1か月が歯抜け（UnknownHistory）。
fn history_of(e: u32) -> Option<Vec<HistoryMonth>> {
    let mut history = Vec::new();
    for (i, (year, month)) in [(2025, 11), (2025, 12), (2026, 1), (2026, 2), (2026, 3)]
        .into_iter()
        .enumerate()
    {
        let i = u32::try_from(i).ok()?;
        if e.is_multiple_of(7) && i == 2 {
            continue;
        }
        history.push(HistoryMonth {
            start: Date::from_ymd(year, month, 1).ok()?,
            total: i64::from(((e * 13 + i * 97) % 90) * 60),
        });
    }
    Some(history)
}

fn bench_limits36(runs: usize) -> Option<Measured> {
    type Case = (Agreement, Vec<MonthHours>, Vec<HistoryMonth>);
    let cases: Vec<Case> = (0..EMPLOYEES)
        .map(|e| Some((agreement_of(e), months_of(e)?, history_of(e)?)))
        .collect::<Option<_>>()?;
    let (best, median, checksum) = measure(runs, || {
        let mut findings = 0_u64;
        for (agreement, months, history) in &cases {
            let valid: ValidAgreement = black_box(agreement).validate().ok()?;
            let result = judge(&valid, black_box(months), black_box(history)).ok()?;
            let count: usize = result.months.iter().map(|m| m.findings.len()).sum();
            findings = findings.wrapping_add(u64::try_from(count).ok()?);
        }
        Some(findings)
    })?;
    Some(Measured {
        label: "36協定の年間判定（AC-10.2。Agreement::validate → judge）",
        target_micros: LIMITS36_TARGET_MICROS,
        best,
        median,
        note: format!("{EMPLOYEES}人×{AGREEMENT_MONTHS}か月＋履歴5か月、指摘{checksum}件"),
    })
}

// ---- 起動（nfr.md の表。`kintai --version`） ----

/// `kintai` の実行ファイル。`--kintai` がなければ、この例と同じビルドの出力（`<target>/<profile>/kintai`）。
fn kintai_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit.or_else(|| {
        let exe = std::env::current_exe().ok()?;
        // <target>/<profile>/examples/bench → <target>/<profile>/kintai
        let profile_dir = exe.parent()?.parent()?;
        Some(profile_dir.join(format!("kintai{}", std::env::consts::EXE_SUFFIX)))
    })
}

fn bench_startup(path: &Path, runs: usize) -> Result<Measured, String> {
    let mut samples = Vec::with_capacity(runs);
    let mut version = String::new();
    // 慣らしの1回（実行ファイルを読み込み済みにする）を含めて runs＋1 回動かし、最初の1回は数えない。
    for index in 0..=runs {
        let started = Clock::now();
        let output = Command::new(path)
            .arg("--version")
            .output()
            .map_err(|error| format!("{} を動かせませんでした: {error}", path.display()))?;
        let elapsed = started.elapsed();
        if !output.status.success() {
            return Err(format!(
                "kintai --version の終了コードが0ではありません: {}",
                output.status
            ));
        }
        if index == 0 {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .clone_into(&mut version);
        } else {
            samples.push(elapsed);
        }
    }
    let (best, median) = best_and_median(samples).ok_or("測った回数が0です")?;
    Ok(Measured {
        label: "起動（kintai --version。nfr.md の表）",
        target_micros: STARTUP_TARGET_MICROS,
        best,
        median,
        note: format!("{runs}回、出力「{version}」"),
    })
}

// ---- 引数と出力 ----

struct Options {
    runs: usize,
    startup_runs: usize,
    kintai: Option<PathBuf>,
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        runs: DEFAULT_RUNS,
        startup_runs: DEFAULT_STARTUP_RUNS,
        kintai: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().ok_or(format!("{arg} の値がありません"))?;
        match arg.as_str() {
            "--runs" => options.runs = value.parse().map_err(|_| "--runs は整数です")?,
            "--startup-runs" => {
                options.startup_runs = value.parse().map_err(|_| "--startup-runs は整数です")?;
            }
            "--kintai" => options.kintai = Some(PathBuf::from(value)),
            _ => return Err(format!("知らない引数です: {arg}")),
        }
    }
    if options.runs < MIN_RUNS || options.startup_runs < MIN_RUNS {
        return Err(format!("回数は{MIN_RUNS}以上にしてください"));
    }
    Ok(options)
}

fn report(item: &Measured) {
    let verdict = if item.within_target() {
        "目標以内"
    } else {
        "目標を超えた"
    };
    println!("- {}", item.label);
    println!(
        "    目標 {}、最良 {}、中央値 {} → {verdict}",
        millis(Duration::from_micros(
            u64::try_from(item.target_micros).unwrap_or(u64::MAX)
        )),
        millis(item.best),
        millis(item.median),
    );
    println!("    {}", item.note);
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    println!("kintai の速さの測定（docs/spec/nfr.md。合成データ）");
    println!(
        "  {}・{}、{}、各項目は慣らし1回のあと{}回（起動は{}回）",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug ビルド（目標の判定には --release で測ること）"
        } else {
            "release ビルド"
        },
        options.runs,
        options.startup_runs,
    );
    let mut items = Vec::new();
    match bench_classify(options.runs) {
        Some(found) => items.extend(found),
        None => eprintln!("区分の計算を測れませんでした（合成データか計算の誤り）"),
    }
    match bench_limits36(options.runs) {
        Some(found) => items.push(found),
        None => eprintln!("36協定の判定を測れませんでした（合成データか計算の誤り）"),
    }
    let measured_all = items.len() == 3;
    for item in &items {
        report(item);
    }
    match kintai_path(options.kintai) {
        Some(path) if path.is_file() => match bench_startup(&path, options.startup_runs) {
            Ok(item) => {
                report(&item);
                items.push(item);
            }
            Err(message) => eprintln!("起動を測れませんでした: {message}"),
        },
        _ => println!(
            "- 起動: 測っていません（kintai の実行ファイルがない。先に cargo build --release -p kintai-cli）"
        ),
    }
    if measured_all && items.iter().all(Measured::within_target) {
        println!("判定: すべて目標以内（中央値で判定）");
        ExitCode::SUCCESS
    } else {
        println!("判定: 目標を超えた項目か、測れなかった項目があります");
        ExitCode::FAILURE
    }
}
