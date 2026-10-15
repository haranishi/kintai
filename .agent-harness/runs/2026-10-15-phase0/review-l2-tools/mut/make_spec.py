import json, os, sys
WS = sys.argv[1]
L = "crates/kintai-jp-labor/src/limits36.rs"
LV = "crates/kintai-jp-labor/src/leave.rs"
EX = "crates/kintai-jp-labor/src/explain.rs"
RP = "crates/kintai-jp-labor/src/report.rs"
P = "crates/kintai-core/src/punch.rs"
IM = "crates/kintai-import/src/lib.rs"
PF = "crates/kintai-cli/src/policy_file.rs"
CK = "crates/kintai-cli/src/check.rs"
CR = "crates/kintai-cli/src/report.rs"
CE = "crates/kintai-cli/src/explain.rs"
muts = [
 ("L2-M01 36: 特別条項の月の上限ちょうどを超過にする", L, "if activated && total > special_monthly_total {", "if activated && total >= special_monthly_total {"),
 ("L2-M02 36: 特別条項の月の上限の最大を6000にする", L, "pub const SPECIAL_MONTHLY_TOTAL_MAX_MINUTES: i64 = 5999;", "pub const SPECIAL_MONTHLY_TOTAL_MAX_MINUTES: i64 = 6000;"),
 ("L2-M03 36: 特別条項の年の上限の最大を43201にする", L, "pub const SPECIAL_ANNUAL_MAX_MINUTES: i64 = 43_200;", "pub const SPECIAL_ANNUAL_MAX_MINUTES: i64 = 43_201;"),
 ("L2-M04 36: 協定の年限度の最大を21601にする", L, "            ANNUAL_OVERTIME_LIMIT_MINUTES,\n            \"0〜21600\",", "            ANNUAL_OVERTIME_LIMIT_MINUTES + 1,\n            \"0〜21600\","),
 ("L2-M05 36: 直近の合計を5か月までにする", L, "for back in 0..6_i64 {", "for back in 0..5_i64 {"),
 ("L2-M06 36: 履歴の月を1か月ずらす", L, "history.get(&(first_index + offset)).copied()", "history.get(&(first_index + offset - 1)).copied()"),
 ("L2-M07 36: months が空のとき履歴の日を29〜31も許す", L, "            if !(1..=28).contains(&entry.start.day()) {\n                return Err(Limits36Error::HistoryNotAgreementMonth { index });", "            if !(1..=31).contains(&entry.start.day()) {\n                return Err(Limits36Error::HistoryNotAgreementMonth { index });"),
 ("L2-M08 年休: 年48日を表の外にしない（47日を週1日の行に）", LV, "GrantRow::OneDay => Some((48, 72)),", "GrantRow::OneDay => Some((47, 72)),"),
 ("L2-M09 年休: 5日の期間を13か月にする", LV, "pub const FIVE_DAY_WINDOW_MONTHS: u32 = 12;", "pub const FIVE_DAY_WINDOW_MONTHS: u32 = 13;"),
 ("L2-M10 年休: 取得を日付順に並べない", LV, "    counted.sort_by_key(|(_, record)| record.date);\n", ""),
 ("L2-M11 打刻: 1か月の勤務の数を年を見ずに数える", P, "let this_month = (date.year(), date.month());", "let this_month = (2000, date.month());"),
 ("L2-M12 打刻: 始業を切り上げる", P, "        start: shift.start.time.floor(),", "        start: shift.start.time.ceil().unwrap_or(shift.start.time.floor()),"),
 ("L2-M13 打刻: 休憩の開始を切り捨てる", P, "        let from = start\n            .time\n            .ceil()\n            .map_err(|_| PunchError::OutOfRange { index: start.index })?;", "        let from = start.time.floor();"),
 ("L2-M14 取込: 行の長さに CR を含める", IM, "        let line = line.strip_suffix(b\"\\r\").unwrap_or(line);\n        if line.len() > MAX_LINE_BYTES {\n            return Err(ImportError::line(ImportErrorKind::LineTooLong, number));", "        if line.len() > MAX_LINE_BYTES {\n            return Err(ImportError::line(ImportErrorKind::LineTooLong, number));"),
 ("L2-M15 方針: 行の長さに CR を含める", PF, "        let line = line.strip_suffix(b\"\\r\").unwrap_or(line);\n        if line.len() > MAX_LINE_BYTES {\n            return Err(invalid_at(", "        if line.len() > MAX_LINE_BYTES {\n            return Err(invalid_at("),
 ("L2-M16 説明: 法定内の深夜の割増を0%にする", EX, "(Category::Within | Category::ExtraWithin, true) => (25,", "(Category::Within | Category::ExtraWithin, true) => (0,"),
 ("L2-M17 説明: 時間外の深夜を25%にする", EX, "(Category::Overtime, true) => (50,", "(Category::Overtime, true) => (25,"),
 ("L2-M18 説明: 法定休日を25%にする", EX, "(Category::Holiday, false) => (35,", "(Category::Holiday, false) => (25,"),
 ("L2-M19 集計: 週の記録を週の初日だけで期間に入れる", RP, "RecordDate::WeekStart(week) => (0..7).any(|offset| {", "RecordDate::WeekStart(week) => (0..1).any(|offset| {"),
 ("L2-M20 check: 週の記録を週の初日だけで期間に入れる", CK, "RecordDate::WeekStart(week) => (0..7).any(|offset| {", "RecordDate::WeekStart(week) => (0..1).any(|offset| {"),
 ("L2-M21 check: 休憩の不足の期間の終わりを含める", CK, "self.from <= date && date < self.until", "self.from <= date && date <= self.until"),
 ("L2-M22 report: data_from の既定を使わない", CR, "let data_from = args.data_from.or(punches.first_date());", "let data_from = args.data_from;"),
 ("L2-M23 check: data_from の既定を使わない（区分だけ）", CK, "        let work = punches.work(&valid, &employee.id, data_from)?;", "        let work = punches.work(&valid, &employee.id, args.data_from)?;"),
 ("L2-M24 explain: data_from の既定を使わない", CE, "let data_from = args.data_from.or(punches.first_date());", "let data_from = args.data_from;"),
 ("L2-M25 36: 一般条項で発動月の判定を協定の月限度にする", L, "            if activated {\n                findings.push(Finding::MonthlyOvertimeExceeded);\n            } else if overtime > monthly {", "            if overtime > monthly {\n                findings.push(Finding::MonthlyOvertimeExceeded);\n            } else if overtime > monthly {"),
 ("L2-M26 年休: 失効したロットの未使用分を残数に入れる", LV, "let remaining_halves = if expired { 0 } else { unused };", "let remaining_halves = unused;"),
]
ok = True
for label, f, find, rep in muts:
    n = open(os.path.join(WS, f), encoding='utf-8').read().count(find)
    if n != 1:
        ok = False
        print('NG', n, label)
spec = {"test_cmd": ["cargo", "test", "-q", "--workspace", "--no-fail-fast"], "timeout_sec": 900,
        "exclude": ["target", ".git", ".cargo-home", ".cargo-target", ".tmp", "tools/difftest-out", "__pycache__"],
        "mutations": [{"label": l, "file": f, "find": a, "replace": b} for l, f, a, b in muts]}
json.dump(spec, open('l2-mutations.json', 'w'), ensure_ascii=False, indent=1)
print('ok' if ok else 'fix needed', len(muts))
