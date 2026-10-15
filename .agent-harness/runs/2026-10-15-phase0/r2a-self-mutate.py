#!/usr/bin/env python3
"""R2a の自前の変異の確認（作業コピーの中だけで、ファイルを書き換えて試験を回し、必ず戻す）。

tools/mutate.py は作業コピーの外（一時フォルダ）に写すので使わない。
使い方: python3 -I r2a-self-mutate.py <作業コピーの絶対パス> [ラベルの一部]
変異の前に元のファイルを r2a-self-mutate-backup/ に写す（止められたときに戻すため）。
"""
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

WS = Path(sys.argv[1])
LIMITS = "crates/kintai-jp-labor/src/limits36.rs"
LEAVE = "crates/kintai-jp-labor/src/leave.rs"
HOLIDAYS = "crates/kintai-jp-labor/src/holidays.rs"
CASE36 = "crates/kintai-cli/src/case36.rs"
CASELEAVE = "crates/kintai-cli/src/case_leave.rs"

def tests(package, names):
    command = ["cargo", "test", "-q", "-p", package, "--no-fail-fast"]
    for name in names:
        command += ["--test", name]
    return command


L36 = tests("kintai-jp-labor", ["ac05_limits36_rules", "ac05_limits36_vectors", "ac05_limits36_properties"])
LEV = tests("kintai-jp-labor", ["ac06_leave_rules", "ac06_leave_dates", "ac06_leave_table", "ac06_leave_properties"])
HOL = tests("kintai-jp-labor", ["ac07_holidays"])
CLI = tests("kintai-cli", ["ac09_36_case", "ac09_leave_case", "ac09_36_properties", "ac09_leave_properties"])

MUTATIONS = [
    # 36協定
    ("36: 発動月を2700分以上にする", LIMITS, "let activated = overtime > MONTHLY_OVERTIME_LIMIT_MINUTES;", "let activated = overtime >= MONTHLY_OVERTIME_LIMIT_MINUTES;", None),
    ("36: 発動月の数を2700分以上で数える", LIMITS, "if month.overtime > MONTHLY_OVERTIME_LIMIT_MINUTES {", "if month.overtime >= MONTHLY_OVERTIME_LIMIT_MINUTES {", None),
    ("36: 100時間ちょうどを適合にする", LIMITS, "if total >= MONTHLY_TOTAL_CAP_MINUTES {", "if total > MONTHLY_TOTAL_CAP_MINUTES {", None),
    ("36: 平均80時間ちょうどを違反にする", LIMITS, "Some(sum) if *sum > AVERAGE_CAP_MINUTES * i64::from(n) => {", "Some(sum) if *sum >= AVERAGE_CAP_MINUTES * i64::from(n) => {", None),
    ("36: 平均を4800ではなく4799で比べる", LIMITS, "pub const AVERAGE_CAP_MINUTES: i64 = 4800;", "pub const AVERAGE_CAP_MINUTES: i64 = 4799;", None),
    ("36: 年360時間ちょうどを法定の超過にする", LIMITS, "if year.overtime > ANNUAL_OVERTIME_LIMIT_MINUTES {", "if year.overtime >= ANNUAL_OVERTIME_LIMIT_MINUTES {", None),
    ("36: 協定の年限度ちょうどを超過にする", LIMITS, "} else if year.overtime > annual {", "} else if year.overtime >= annual {", None),
    ("36: 協定の月限度ちょうどを超過にする", LIMITS, "} else if overtime > monthly {", "} else if overtime >= monthly {", None),
    ("36: 特別条項の回数ちょうどを超過にする", LIMITS, "if activated && i64::from(year.special_count) > special_months {", "if activated && i64::from(year.special_count) >= special_months {", None),
    ("36: 回数の超過を発動月でない月にも付ける", LIMITS, "if activated && i64::from(year.special_count) > special_months {", "if i64::from(year.special_count) > special_months {", None),
    ("36: 月の上限を発動月でない月にも見る", LIMITS, "if activated && total > special_monthly_total {", "if total > special_monthly_total {", None),
    ("36: 特別条項の年の上限ちょうどを超過にする", LIMITS, "if year.overtime > special_annual {", "if year.overtime >= special_annual {", None),
    ("36: 協定年で戻さない", LIMITS, "if index == 0 || position == 0 {", "if index == 0 {", None),
    ("36: 途中から始まる年も ΣO が分かるとする", LIMITS, "known: position == 0,", "known: true,", None),
    ("36: 足りない月を0分とみなす", LIMITS, "            _ => None,\n        };\n        let slot", "            (Some(sum), None) => Some(sum),\n            _ => None,\n        };\n        let slot", None),
    ("36: 協定月の初日に29日を許す", LIMITS, "        if !(1..=28).contains(&month.start.day()) {\n            return Err(Limits36Error::MonthStartDay { index });", "        if !(1..=29).contains(&month.start.day()) {\n            return Err(Limits36Error::MonthStartDay { index });", None),
    ("36: 履歴に months の最初の月を許す", LIMITS, "if key >= month_index(first) {", "if key > month_index(first) {", None),
    ("36: 履歴の同じ月の2回を許す", LIMITS, "if totals.insert(key, entry.total).is_some() {", "if totals.insert(key, entry.total).is_some() && false {", None),
    ("36: 月の分の上限を44641にする", LIMITS, "pub const MONTH_MINUTES_MAX: i64 = 44_640;", "pub const MONTH_MINUTES_MAX: i64 = 44_641;", None),
    ("36: 協定なしで H だけの月を違反にしない", LIMITS, "if overtime > 0 || month.holiday > 0 {", "if overtime > 0 {", None),
    ("36: 協定の月限度の最大を2701にする", LIMITS, "            MONTHLY_OVERTIME_LIMIT_MINUTES,\n            \"0〜2700\",", "            MONTHLY_OVERTIME_LIMIT_MINUTES + 1,\n            \"0〜2700\",", None),
    ("36: 回数の最大を7にする", LIMITS, "pub const SPECIAL_MONTHS_MAX: i64 = 6;", "pub const SPECIAL_MONTHS_MAX: i64 = 7;", None),
    # 年休
    ("年休: 失効日の当日を失効にしない", LEAVE, "let expired = as_of >= draft.expires;", "let expired = as_of > draft.expires;", None),
    ("再: 年休: 失効日の当日の取得を許す", LEAVE, "self.grant_date <= date && date < self.expires", "self.grant_date <= date && date <= self.expires", None),
    ("再: 年休: 付与日の当日を使えなくする", LEAVE, "self.grant_date <= date && date < self.expires", "self.grant_date < date && date < self.expires", None),
    ("再: 年休: 付与日の当日のロットを飛ばす", LEAVE, "if need == 0 || draft.grant_date > record.date {", "if need == 0 || draft.grant_date >= record.date {", None),
    ("年休: 10日ちょうどを対象外にする", LEAVE, "let status = if draft.granted_halves < FIVE_DAY_GRANT_MIN_HALVES {", "let status = if draft.granted_halves <= FIVE_DAY_GRANT_MIN_HALVES {", None),
    ("年休: 5日ちょうどを未達にする", LEAVE, "} else if taken_halves >= FIVE_DAY_TAKEN_HALVES {", "} else if taken_halves > FIVE_DAY_TAKEN_HALVES {", None),
    ("年休: 期間の最後の日を NotMet にする", LEAVE, "} else if as_of > draft.window_end {", "} else if as_of >= draft.window_end {", None),
    ("年休: 応当日があるとき前日にしない", LEAVE, "        if self.day <= last {\n            CalendarDate {", "        if self.day < last {\n            CalendarDate {", None),
    ("年休: 半日を2半日にする", LEAVE, "            LeaveUnit::Half => Some(1),", "            LeaveUnit::Half => Some(2),", None),
    ("年休: below の年も付与する", LEAVE, "            Attendance::Below => {}\n", "            Attendance::Below => drafts.push(Draft::new(date, row.halves(k))?),\n", None),
    ("年休: below の年で勤続年数を戻す", LEAVE, "drafts.push(Draft::new(date, row.halves(k))?);", "drafts.push(Draft::new(date, row.halves(u32::try_from(drafts.len()).unwrap_or(0)))?);", None),
    ("年休: unknown に注意を付けない", LEAVE, "unconfirmed |= attendance == Attendance::Unknown;", "unconfirmed |= false;", None),
    ("年休: as_of 当日の付与を出さない", LEAVE, "        if date > input.as_of {\n            break;", "        if date >= input.as_of {\n            break;", None),
    ("年休: 対応外の判定に as_of 当日を入れない", LEAVE, "(Some(as_of), Some(date)) => date <= as_of,", "(Some(as_of), Some(date)) => date < as_of,", None),
    ("年休: 分割付与を10日ちょうどでも見る", LEAVE, "*halves < FIVE_DAY_GRANT_MIN_HALVES\n", "*halves <= FIVE_DAY_GRANT_MIN_HALVES\n", None),
    ("年休: 分割付与で次の初日ちょうども分割にする", LEAVE, ".is_some_and(|limit| *next < limit)", ".is_some_and(|limit| *next <= limit)", None),
    ("年休: 週1800分ちょうどを比例付与にする", LEAVE, "|| self.weekly_minutes >= NORMAL_WEEKLY_MINUTES_MIN", "|| self.weekly_minutes > NORMAL_WEEKLY_MINUTES_MIN", None),
    ("年休: 年217日を比例付与にする", LEAVE, "if annual >= NORMAL_ANNUAL_DAYS_MIN {", "if annual > NORMAL_ANNUAL_DAYS_MIN {", None),
    ("年休: 年間の日数より週の日数を優先する", LEAVE, "        if let Some(annual) = self.annual_days {", "        if let Some(annual) = self.annual_days.filter(|_| self.weekly_days == 0) {", None),
    ("年休: 付与表の1セルを変える", LEAVE, "pub const THREE_DAYS_GRANT_DAYS: [u32; 7] = [5, 6, 6, 8, 9, 10, 11];", "pub const THREE_DAYS_GRANT_DAYS: [u32; 7] = [5, 6, 7, 8, 9, 10, 11];", None),
    ("年休: 消化を新しいロットから", LEAVE, "for (draft, used) in drafts.iter().zip(used.iter_mut()).skip(first_active) {", "for (draft, used) in drafts.iter().zip(used.iter_mut()).skip(first_active).rev() {", None),
    ("年休: 半日の上限を201にする", LEAVE, "pub const GRANT_HALVES_RANGE: RangeInclusive<i64> = 1..=200;", "pub const GRANT_HALVES_RANGE: RangeInclusive<i64> = 1..=201;", None),
    ("年休: 失効を23か月にする", LEAVE, "pub const EXPIRY_MONTHS: u32 = 24;", "pub const EXPIRY_MONTHS: u32 = 23;", None),
    # 祝日
    ("再: 祝日: 春分の定数を変える", HOLIDAYS, "const VERNAL_BASE: i64 = 20_843_100;", "const VERNAL_BASE: i64 = 20_843_000;", None),
    ("再: 祝日: 秋分の定数を0.015下げる（2074年が変わる大きさ）", HOLIDAYS, "const AUTUMNAL_BASE: i64 = 23_248_800;", "const AUTUMNAL_BASE: i64 = 23_233_700;", None),
    ("再: 祝日: 秋分の定数を0.01上げる（2045年が変わる大きさ）", HOLIDAYS, "const AUTUMNAL_BASE: i64 = 23_248_800;", "const AUTUMNAL_BASE: i64 = 23_258_800;", None),
    ("再: 祝日: 年の増分を変える", HOLIDAYS, "const EQUINOX_DRIFT: i64 = 242_194;", "const EQUINOX_DRIFT: i64 = 242_190;", None),
    ("祝日: 4年ごとの補正を外す", HOLIDAYS, "- EQUINOX_SCALE * years.div_euclid(4);", "- EQUINOX_SCALE * years.div_euclid(5);", None),
    ("祝日: 土曜の祝日を振り替える", HOLIDAYS, "if date.weekday() != Weekday::Sunday {", "if date.weekday() != Weekday::Saturday {", None),
    ("祝日: 国民の休日で自身が祝日かを見ない", HOLIDAYS, "if !set.contains(&middle) && set.contains(&after) {", "if set.contains(&after) {", None),
    ("祝日: 振替休日を再帰して国民の休日を数える", HOLIDAYS, "    for &date in &set {\n        let Ok(middle)", "    let set: BTreeSet<Date> = set.iter().copied().chain(derived.keys().copied()).collect();\n    for &date in &set {\n        let Ok(middle)", None),
    ("祝日: 2028年を公式にする", HOLIDAYS, "pub const OFFICIAL_LAST_YEAR: i32 = 2027;", "pub const OFFICIAL_LAST_YEAR: i32 = 2028;", None),
    ("祝日: 2050年に注意を付ける", HOLIDAYS, "&& year > EQUINOX_VERIFIED_LAST_YEAR", "&& year >= EQUINOX_VERIFIED_LAST_YEAR", None),
    ("祝日: 第n月曜を1週ずらす", HOLIDAYS, "+ 7 * n.saturating_sub(1);", "+ 7 * n;", None),
    ("祝日: 2021年の山の日を通常にする", HOLIDAYS, "2021 => (ymd(7, 22)?, ymd(7, 23)?, ymd(8, 8)?),", "2021 => (ymd(7, 22)?, ymd(7, 23)?, ymd(8, 11)?),", None),
    # CLI
    ("CLI36: 知らない項目を許す", CASE36, "    reject_unknown(map, &MONTH_KEYS, &path)?;\n", "", CLI),
    ("CLI36: 種類の値の大文字を許す", CASE36, "        \"special\" => Ok(AgreementKind::Special),", "        \"special\" | \"Special\" => Ok(AgreementKind::Special),", CLI),
    ("CLI36: n のない指摘にも n を出す", CASE36, "    #[serde(skip_serializing_if = \"Option::is_none\")]\n", "", CLI),
    ("CLIleave: hours を全日にする", CASELEAVE, "        \"hours\" => Ok(LeaveUnit::Hours),", "        \"hours\" => Ok(LeaveUnit::Full),", CLI),
    ("CLIleave: 日付の検査を対応外より先にする", CASELEAVE, "    // 手順3（日付は書式だけを見て比べる。範囲の誤りと書式の誤りは手順4）\n", "    let _ = date(raw.as_of, \"as_of\")?;\n    for t in &raw.taken { let _ = date(t.date, \"taken\")?; }\n", CLI),
    ("CLIleave: 範囲の誤りの日付を書式の誤りと同じにする", CASELEAVE, "            date: CalendarDate::parse(record.date),", "            date: kintai_core::Date::parse(record.date).ok().map(CalendarDate::from_date),", CLI),
]


def run(cmd):
    env = dict(os.environ)
    p = subprocess.run(cmd, cwd=WS, env=env, capture_output=True, text=True, timeout=1500)
    out = p.stdout + p.stderr
    if p.returncode == 0:
        return "ok"
    if "could not compile" in out:
        return "compile"
    return "red"


def main():
    only = sys.argv[2] if len(sys.argv) > 2 else None
    base = {"limits36": run(L36), "leave": run(LEV), "holidays": run(HOL), "cli": run(CLI)}
    print("基準:", base, flush=True)
    if any(v != "ok" for v in base.values()):
        return 2
    results = []
    for label, file, find, replace, cmd in MUTATIONS:
        cmd = cmd or {LIMITS: L36, LEAVE: LEV, HOLIDAYS: HOL}[file]
        if only and only not in label:
            continue
        path = WS / file
        original = path.read_text(encoding="utf-8")
        n = original.count(find)
        if n != 1:
            print(f"[{label}] 適用できない: 見つかった数={n}", flush=True)
            results.append((label, "not-applied"))
            continue
        backup_dir = WS / ".agent-harness/runs/2026-10-15-phase0/r2a-self-mutate-backup"
        backup_dir.mkdir(exist_ok=True)
        backup = backup_dir / (path.name + ".orig")
        shutil.copy(path, backup)
        try:
            path.write_text(original.replace(find, replace), encoding="utf-8")
            time.sleep(1.1)
            outcome = run(cmd)
        finally:
            path.write_text(original, encoding="utf-8")
            time.sleep(1.1)
        verdict = {"red": "検出", "ok": "生き残り", "compile": "コンパイルできない"}[outcome]
        print(f"[{label}] {verdict}", flush=True)
        results.append((label, verdict))
    survivors = [label for label, v in results if v != "検出"]
    print("\n== 生き残り・適用できない・コンパイルできない ==")
    print("\n".join(survivors) if survivors else "なし")
    print(f"件数 {len(results)}、検出 {sum(1 for _, v in results if v == '検出')}")
    return 1 if survivors else 0


if __name__ == "__main__":
    import signal

    # 止められても finally で元に戻すように、SIGTERM を例外にする。
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(1))
    sys.exit(main())
