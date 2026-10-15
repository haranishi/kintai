# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""オラクル: 年休のケース kintai.caseleave.v1 を読み、kintai.leave.v1 を出す（仕様 第2版）。

使い方: python3 tools/oracle/leave_oracle.py FILE

docs/spec/leave.md の「日付の規則」と規則1〜7、docs/spec/case-format-36-leave.md の形式を、
素朴に書いた。取得は「半日ずつ」古いロットから消化する。
検査の順序は、区分（docs/spec/validation.md）と同じ流儀で、形と型 → 列挙と範囲 → 対応外 →
日付の書式と整合 → 計算（残数が足りない記録は、ここで不正な入力）。


対応する受け入れ条件: AC-06.11
"""
import sys

sys.dont_write_bytecode = True   # .gitignore に __pycache__ が無いので、作業ツリーを汚さない
import datetime as dt

from common import (check_list, check_object, check_type, cli, invalid, is_int, is_str,
                    next_start, parse_date, parse_date_or_none, period_end, unsupported)

SCHEMA_IN = 'kintai.caseleave.v1'
SCHEMA_OUT = 'kintai.leave.v1'

EMPLOYEE_FIELDS = ['hire_date', 'weekly_scheduled_days', 'weekly_scheduled_minutes',
                   'annual_scheduled_days', 'attendance', 'grants']
# 必須は hire_date・weekly_scheduled_days・weekly_scheduled_minutes・attendance。
# annual_scheduled_days と grants は、項目がなくても null でも指定なし（決着（旧Q29））。
REQUIRED_EMPLOYEE_FIELDS = ['hire_date', 'weekly_scheduled_days', 'weekly_scheduled_minutes',
                            'attendance']
ATTENDANCE_VALUES = ['meets', 'below', 'unknown']
UNIT_HALVES = {'full': 2, 'half': 1}          # 半日を1とする

# leave.md 規則2: 6か月、1年6か月…の付与日数。6年6か月以後は最後の値。
NORMAL_DAYS = [10, 11, 12, 14, 16, 18, 20]
PROPORTIONAL_DAYS = {
    4: [7, 8, 9, 10, 12, 13, 15],
    3: [5, 6, 6, 8, 9, 10, 11],
    2: [3, 4, 4, 5, 6, 6, 7],
    1: [1, 2, 2, 2, 3, 3, 3],
}
FIVE_DAYS_IN_HALVES = 10          # 規則6: 5日
FIVE_DAY_RULE_FROM_HALVES = 20    # 規則6: 付与日数が10日以上の付与だけが対象
SPLIT_GRANT_BELOW_HALVES = 20     # 範囲: 10日（20半日）未満の付与に続く付与が早すぎると分割付与


def leave(case):
    """年休のケース（kintai.caseleave.v1）を計算する。"""
    employee, taken_raw = step1_structure(case)
    step2_values(employee, taken_raw)
    step3_unsupported(case, employee, taken_raw)
    dates = step4_dates(case, employee, taken_raw)
    return step5_compute(employee, dates)


# ---------------------------------------------------------------- 手順1: 形と型

def step1_structure(case):
    # トップレベルの必須は schema・as_of・employee・taken（taken は空でもよく、null は型の誤り。
    # case-format-36-leave.md、決着（旧Q42））。
    check_object(case, 'case', ['schema', 'as_of', 'employee', 'taken'],
                 ['schema', 'as_of', 'employee', 'taken'])
    if case['schema'] != SCHEMA_IN or not is_str(case['schema']):
        invalid(f'schema must be {SCHEMA_IN}')
    check_type(case['as_of'], is_str, 'as_of')
    employee = check_object(case['employee'], 'employee', EMPLOYEE_FIELDS, REQUIRED_EMPLOYEE_FIELDS)
    check_type(employee['hire_date'], is_str, 'employee.hire_date')
    check_type(employee['weekly_scheduled_days'], is_int, 'employee.weekly_scheduled_days')
    check_type(employee['weekly_scheduled_minutes'], is_int, 'employee.weekly_scheduled_minutes')
    check_type(employee.get('annual_scheduled_days'), is_int, 'employee.annual_scheduled_days',
               nullable=True)
    for i, value in enumerate(check_list(employee['attendance'], 'employee.attendance')):
        check_type(value, is_str, f'employee.attendance[{i}]')
    grants = employee.get('grants')
    if grants is not None:
        for i, grant in enumerate(check_list(grants, 'employee.grants')):
            check_object(grant, f'employee.grants[{i}]', ['date', 'halves'], ['date', 'halves'])
            check_type(grant['date'], is_str, f'employee.grants[{i}].date')
            check_type(grant['halves'], is_int, f'employee.grants[{i}].halves')
    taken = check_list(case['taken'], 'taken')
    for i, record in enumerate(taken):
        check_object(record, f'taken[{i}]', ['date', 'unit'], ['date', 'unit'])
        check_type(record['date'], is_str, f'taken[{i}].date')
        check_type(record['unit'], is_str, f'taken[{i}].unit')
    return employee, taken


# ---------------------------------------------------------------- 手順2: 列挙と範囲

def step2_values(employee, taken):
    # 範囲は、週の日数0〜7、週の分0〜10080、年間の日数0〜366（case-format-36-leave.md、決着（旧Q20））。
    if not 0 <= employee['weekly_scheduled_days'] <= 7:
        invalid('employee.weekly_scheduled_days must be 0..7')
    if not 0 <= employee['weekly_scheduled_minutes'] <= 7 * 1440:
        invalid('employee.weekly_scheduled_minutes must be 0..10080')
    annual = employee.get('annual_scheduled_days')
    if annual is not None and not 0 <= annual <= 366:
        invalid('employee.annual_scheduled_days must be 0..366')
    for i, value in enumerate(employee['attendance']):
        if value not in ATTENDANCE_VALUES:
            invalid(f'employee.attendance[{i}] must be meets, below or unknown')
    for i, grant in enumerate(employee.get('grants') or []):
        # halves は1〜200の整数（case-format-36-leave.md、決着（旧Q41））。
        if not 1 <= grant['halves'] <= 200:
            invalid(f'employee.grants[{i}].halves must be 1..200')
    for i, record in enumerate(taken):
        if record['unit'] not in ('full', 'half', 'hours'):
            invalid(f'taken[{i}].unit must be full or half')


# ---------------------------------------------------------------- 手順3: 対応外

def step3_unsupported(case, employee, taken):
    """leave.md「範囲」の順に見る: LeaveHourly → LeaveSplitGrant → LeaveOutOfTable（決着（旧Q27・Q28））。

    as_of と日付が正しい書式で、as_of より後の取得と付与は、対応外の判定に入れない。書式の誤りは
    入れたまま判定し、手順4で不正な入力になる。as_of が書式の誤りなら、何も除かない（決着（旧Q37））。
    「正しい書式」は、書式と実在（2月30日は誤り）だけを見る。範囲外の日付も比べる。
    """
    as_of = parse_date_or_none(case['as_of'])

    def after_as_of(text):
        date = parse_date_or_none(text)
        return as_of is not None and date is not None and date > as_of

    for record in taken:
        if record['unit'] == 'hours' and not after_as_of(record['date']):
            unsupported('LeaveHourly', 'hourly annual leave is not supported (leave.md)')

    grants = employee.get('grants')
    if grants is not None:
        included = [g for g in grants if not after_as_of(g['date'])]
        parsed = [(parse_date_or_none(g['date']), g['halves']) for g in included]
        # 判定に入る付与に書式の誤った付与日があれば、分割付与の判定はしない（そのあと LeaveOutOfTable を
        # 見て、手順4で InvalidInput になる。leave.md、決着（旧Q45））。
        if all(date is not None for date, _ in parsed):
            # 付与日の昇順に安定に並べる。同じ日は入力の順（決着（旧Q38））。
            ordered = sorted(parsed, key=lambda g: g[0])
            for (date, halves), (next_date, _) in zip(ordered, ordered[1:]):
                if halves < SPLIT_GRANT_BELOW_HALVES and next_date < next_start(date, 12):
                    unsupported('LeaveSplitGrant', 'split first-year grants are not supported (leave.md)')

    # LeaveOutOfTable は、規則2の①（週5日以上か週1800分以上）に当たらず、②で年間48日未満、または③で
    # 週0日のときだけ。grants を明示しても同じ（決着（旧Q36））。
    days, minutes = employee['weekly_scheduled_days'], employee['weekly_scheduled_minutes']
    annual = employee.get('annual_scheduled_days')
    if not (days >= 5 or minutes >= 1800):
        if annual is not None and annual < 48:
            unsupported('LeaveOutOfTable', 'annual scheduled days below 48 (leave.md)')
        if annual is None and days == 0:
            unsupported('LeaveOutOfTable', 'weekly scheduled days 0 (leave.md)')


# ---------------------------------------------------------------- 手順4: 日付

def step4_dates(case, employee, taken_raw):
    as_of = parse_date(case['as_of'], 'as_of')
    hire_date = parse_date(employee['hire_date'], 'employee.hire_date')
    explicit = None
    if employee.get('grants') is not None:
        explicit = []
        for i, grant in enumerate(employee['grants']):
            explicit.append({'date': parse_date(grant['date'], f'employee.grants[{i}].date'),
                             'halves': grant['halves']})
    taken = []
    for i, record in enumerate(taken_raw):
        taken.append({'date': parse_date(record['date'], f'taken[{i}].date'),
                      'unit': record['unit'], 'echo': {'date': record['date'], 'unit': record['unit']}})
    return {'as_of': as_of, 'hire_date': hire_date, 'explicit': explicit, 'taken': taken}


# ---------------------------------------------------------------- 手順5: 計算

def step5_compute(employee, dates):
    as_of = dates['as_of']

    # 規則1〜3: 付与（付与日、日数、出勤率）。as_of より後の付与日のロットは出さない。
    # grants の明示も同じ（決着（旧Q31））。
    grants = [g for g in list_grants(employee, dates) if g['date'] <= as_of]
    lots = []
    notices = []
    for grant in grants:
        if grant['attendance'] == 'below':      # 規則3: below の年は付与しない
            continue
        if grant['attendance'] == 'unknown' and 'AttendanceUnconfirmed' not in notices:
            notices.append('AttendanceUnconfirmed')   # 1回だけ（決着（旧Q22））
        lots.append({
            'grant_date': grant['date'],
            'granted': grant['halves'],
            # 失効日 = 次の初日(付与日, 24)。当日は使えない。2099年を超えてもよい（決着（旧Q19・Q32））。
            'expires': next_start(grant['date'], 24),
            'used': 0,
        })
    lots.sort(key=lambda lot: lot['grant_date'])   # 同じ付与日なら元の順（安定な並べ替え）

    # 規則5: as_of より後の記録は数えず、残数の検査もしない（決着（旧Q24））。同じ日の複数の記録は、
    # そのまま数える。日付の順に、半日ずつ、その日に使えるロットの古いものから消化する。1日の取得は
    # 残り半日のロットと次のロットにまたがってよい（決着（旧Q23））。
    taken = sorted((r for r in dates['taken'] if r['date'] <= as_of), key=lambda r: r['date'])
    for record in taken:
        need = UNIT_HALVES[record['unit']]
        for lot in lots:
            usable_on_that_day = lot['grant_date'] <= record['date'] < lot['expires']
            while need > 0 and usable_on_that_day and lot['used'] < lot['granted']:
                lot['used'] += 1
                need -= 1
        if need > 0:
            invalid(f'taken {record["echo"]["date"]}: not enough remaining leave on that day')

    out_lots = []
    balance = 0
    for lot in lots:
        unused = lot['granted'] - lot['used']
        expired = as_of >= lot['expires']          # 失効の当日から失効済み
        remaining = 0 if expired else unused
        balance += remaining
        out_lots.append({
            'grant_date': lot['grant_date'].isoformat(),
            'granted_halves': lot['granted'],
            'expires': lot['expires'].isoformat(),
            'used_halves': lot['used'],
            'remaining_halves': remaining,
            'expired_halves': unused if expired else 0,
        })

    # 規則6と7: 5日の期間は、付与日から 期間の終わり(付与日, 12) まで。
    five_day = []
    register = []
    for lot in lots:
        base = lot['grant_date']
        window_end = period_end(base, 12)
        in_window = [r for r in taken if base <= r['date'] <= window_end]
        taken_halves = sum(UNIT_HALVES[r['unit']] for r in in_window)
        # needed_halves は 10 − taken_halves（0未満にならない）。Met と NotApplicable は0（決着（旧Q26））。
        if lot['granted'] < FIVE_DAY_RULE_FROM_HALVES:
            status, needed = 'NotApplicable', 0
        elif taken_halves >= FIVE_DAYS_IN_HALVES:
            status, needed = 'Met', 0
        elif as_of > window_end:
            status, needed = 'NotMet', max(0, FIVE_DAYS_IN_HALVES - taken_halves)
        else:
            status, needed = 'InProgress', max(0, FIVE_DAYS_IN_HALVES - taken_halves)
        five_day.append({'base_date': base.isoformat(), 'window_end': window_end.isoformat(),
                         'taken_halves': taken_halves, 'status': status, 'needed_halves': needed})
        # 管理簿の取得は、基準日から5日の期間に入る取得（消化したロットとは限らない。決着（旧Q25））。
        # 日付の昇順で、同じ日は入力の順（taken は安定に並べてある）。
        register.append({'base_date': base.isoformat(), 'granted_halves': lot['granted'],
                         'taken': [r['echo'] for r in in_window], 'taken_halves': taken_halves})

    # 保存の目安は JSON に欄を作らない（leave.md、決着（旧Q33））。
    return {'schema': SCHEMA_OUT, 'lots': out_lots, 'balance_halves': balance,
            'five_day': five_day, 'register': register, 'notices': notices}


def list_grants(employee, dates):
    """付与の一覧（付与日の順）。as_of で切るのは呼ぶ側。"""
    if dates['explicit'] is not None:
        # grants を明示したときは、付与を事実として扱い、attendance は使わず注意も付けない
        # （case-format-36-leave.md、決着（旧Q21））。
        explicit = sorted(dates['explicit'], key=lambda g: g['date'])
        return [{'date': g['date'], 'halves': g['halves'], 'attendance': 'meets'} for g in explicit]

    attendance = employee['attendance']

    def attendance_of(index):
        # case-format-36-leave.md「足りない付与は unknown」。付与の数より長いときの余りは使わない。
        return attendance[index] if index < len(attendance) else 'unknown'

    table = grant_table_row(employee)
    first = next_start(dates['hire_date'], 6)    # g0 = 次の初日(雇入れ日, 6)
    grants = []
    k = 0
    while True:
        date = next_start(first, 12 * k)         # g_k = 次の初日(g0, 12k)。g0 から数える（決着（旧Q18））
        if date > dates['as_of']:
            break
        grants.append({'date': date, 'halves': 2 * table[min(k, 6)], 'attendance': attendance_of(k)})
        k += 1      # 規則3: below の年も勤続年数は進む
    return grants


def grant_table_row(employee):
    """leave.md 規則2: ①週5日以上か週1800分以上 → ②年間の日数 → ③週の日数（決着（旧Q20））。"""
    days = employee['weekly_scheduled_days']
    minutes = employee['weekly_scheduled_minutes']
    annual = employee.get('annual_scheduled_days')
    if days >= 5 or minutes >= 1800:
        return NORMAL_DAYS
    if annual is not None:
        if annual >= 217:
            return NORMAL_DAYS
        if annual >= 169:
            return PROPORTIONAL_DAYS[4]
        if annual >= 121:
            return PROPORTIONAL_DAYS[3]
        if annual >= 73:
            return PROPORTIONAL_DAYS[2]
        return PROPORTIONAL_DAYS[1]          # 48〜72日（48日未満は手順3で拒否済み）
    return PROPORTIONAL_DAYS[days]           # 週1〜4日（週0日は手順3で拒否済み）


if __name__ == '__main__':
    sys.exit(cli(leave))
