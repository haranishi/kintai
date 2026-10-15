# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""オラクル: 区分のケース kintai.case.v1 を読み、kintai.classify.v1 を出す（仕様 第2版）。

使い方: python3 tools/oracle/classify_oracle.py FILE

docs/spec/classify.md の手順を、実労働の分を「1分ずつ」数えるループで、そのまま書いた。
区間の演算や最適化はしない（わざと素朴に書く）。
検査の順序は docs/spec/validation.md の手順1〜5に合わせ、関数を手順ごとに分けた。


対応する受け入れ条件: AC-03.14、AC-13.6
"""
import sys

sys.dont_write_bytecode = True   # .gitignore に __pycache__ が無いので、作業ツリーを汚さない
from collections import defaultdict

from common import (MINUTES_PER_DAY, WEEKDAY_NAMES, check_list, check_object, check_type, cli,
                    date_of, day_number, invalid, is_bool, is_int, is_str, parse_date,
                    parse_instant, period_start_of, unsupported, week_start_of,
                    weekday_name_of_day)

SCHEMA_IN = 'kintai.case.v1'
SCHEMA_OUT = 'kintai.classify.v1'

TOP_FIELDS = ['schema', 'data_from', 'policy', 'shifts']
REQUIRED_TOP_FIELDS = ['schema', 'policy', 'shifts']
# holiday_swaps は方針の任意の配列（case-format.md の入力の表、決着（旧Q35））。
POLICY_FIELDS = ['work_system', 'scheduled_minutes_per_day', 'week_start', 'weekly_limit_minutes',
                 'special_44h_confirmed', 'legal_holiday_weekday', 'month_start_day', 'ambiguity',
                 'worker_kind', 'holiday_system', 'holiday_swaps', 'night_window']
# case-format.md の表で「必須」の項目。legal_holiday_weekday も「必須」だが、欠けと null は
# validation.md 手順1のとおり手順3（LegalHolidayUnspecified）で見るので、ここには入れない。
REQUIRED_POLICY_FIELDS = ['work_system', 'scheduled_minutes_per_day', 'month_start_day']

# validation.md「対応外の指定」の表（第2版）。上から順に見る。
UNSUPPORTED_WORK_SYSTEM = {
    'variable_monthly': 'VariableMonthly', 'variable_yearly': 'VariableYearly',
    'variable_weekly_irregular': 'VariableWeeklyIrregular', 'flex': 'Flex',
    'discretionary': 'DiscretionaryDeemed', 'offsite_deemed': 'OffsiteDeemed',
    'managerial': 'Managerial', 'agriculture_exempt': 'AgricultureExempt',
    'surveillance_intermittent': 'SurveillanceIntermittent', 'night_duty_permit': 'NightDutyPermit',
    'highly_professional': 'HighlyProfessional',
}
UNSUPPORTED_WORKER_KIND = {
    'minor': 'Minor', 'pregnant_or_postpartum': 'PregnantOrPostpartum',
    'research_development': 'ResearchDevelopment', 'driver': 'AutomobileDriver',
    'physician': 'Physician', 'disaster_recovery_construction': 'DisasterRecoveryConstruction',
    'hazardous_work': 'HazardousWork',
}
UNSUPPORTED_HOLIDAY_SYSTEM = {'four_weeks': 'FourWeekHolidays',
                              'continuous_rotation': 'ContinuousRotation'}

DAY_LIMIT_MINUTES = 480          # classify.md 手順4: 1日の先頭480分
OVER60_THRESHOLD = 3600          # classify.md 手順5: 累計3600分まで「60時間以下」
NIGHT_FROM = 22 * 60             # classify.md 手順7: 22:00以上
NIGHT_UNTIL = 5 * 60             #                    5:00未満
# validation.md「曖昧さの厳密な定義」の表の順（同じ日付ではこの順に並べる）。
AMBIGUITY_ORDER = ['HolidayBoundary', 'WeekBoundary', 'MonthBoundary', 'IncompleteWeek']

DAY_FIELDS = ['total', 'within', 'extra_within', 'overtime', 'over60', 'holiday', 'night',
              'night_within', 'night_overtime', 'night_over60', 'night_holiday']
PERIOD_FIELDS = ['total', 'within', 'overtime', 'over60', 'holiday', 'night']


def classify(case):
    """区分のケース（kintai.case.v1）を計算する。"""
    policy, raw_shifts = step1_structure(case)
    settings = step2_values(policy)
    step3_unsupported(policy, settings)
    shifts, data_from = step4_shifts_and_dates(raw_shifts, case.get('data_from'))
    return step5_compute(settings, shifts, data_from)


# ---------------------------------------------------------------- 手順1: 形と型

def step1_structure(case):
    """JSONの形、schema、型、知らない項目、必須の項目。日時・日付は「文字列であること」だけを見る。

    任意の項目の null は、項目がないのと同じ。必須の項目の null は型の誤り（validation.md 手順1、
    決着（旧Q5））。
    """
    check_object(case, 'case', TOP_FIELDS, REQUIRED_TOP_FIELDS)
    if case['schema'] != SCHEMA_IN or not is_str(case['schema']):
        invalid(f'schema must be {SCHEMA_IN}')
    check_type(case.get('data_from'), is_str, 'data_from', nullable=True)
    policy = check_object(case['policy'], 'policy', POLICY_FIELDS, REQUIRED_POLICY_FIELDS)
    check_type(policy['work_system'], is_str, 'policy.work_system')
    check_type(policy['scheduled_minutes_per_day'], is_int, 'policy.scheduled_minutes_per_day')
    check_type(policy['month_start_day'], is_int, 'policy.month_start_day')
    check_type(policy.get('week_start'), is_str, 'policy.week_start', nullable=True)
    check_type(policy.get('weekly_limit_minutes'), is_int, 'policy.weekly_limit_minutes', nullable=True)
    check_type(policy.get('special_44h_confirmed'), is_bool, 'policy.special_44h_confirmed', nullable=True)
    check_type(policy.get('legal_holiday_weekday'), is_str, 'policy.legal_holiday_weekday', nullable=True)
    check_type(policy.get('ambiguity'), is_str, 'policy.ambiguity', nullable=True)
    for key in ('worker_kind', 'holiday_system', 'night_window'):
        check_type(policy.get(key), is_str, f'policy.{key}', nullable=True)
    # holiday_swaps は配列。要素の中身は検査しない（validation.md の表）。
    check_type(policy.get('holiday_swaps'), lambda v: type(v) is list, 'policy.holiday_swaps', nullable=True)

    shifts = check_list(case['shifts'], 'shifts')
    for i, shift in enumerate(shifts):
        where = f'shifts[{i}]'
        check_object(shift, where, ['start', 'end', 'breaks'], ['start', 'end'])
        check_type(shift['start'], is_str, f'{where}.start')
        check_type(shift['end'], is_str, f'{where}.end')
        breaks = shift.get('breaks')          # null は項目がないのと同じ（休憩なし）
        if breaks is not None:
            check_list(breaks, f'{where}.breaks')
            for j, pair in enumerate(breaks):
                if type(pair) is not list or len(pair) != 2 or not all(is_str(x) for x in pair):
                    invalid(f'{where}.breaks[{j}]: [start, end] of two strings expected')
    return policy, shifts


# ---------------------------------------------------------------- 手順2: 列挙と範囲

def step2_values(policy):
    """列挙の値と数値の範囲。既定値もここで埋める。"""
    work_system = policy['work_system']
    if work_system != 'standard' and work_system not in UNSUPPORTED_WORK_SYSTEM:
        invalid('policy.work_system: undefined value')
    if not 0 <= policy['scheduled_minutes_per_day'] <= 480:
        invalid('policy.scheduled_minutes_per_day: must be 0..480')
    if not 1 <= policy['month_start_day'] <= 28:
        invalid('policy.month_start_day: must be 1..28')

    settings = {
        'scheduled': policy['scheduled_minutes_per_day'],
        'month_start_day': policy['month_start_day'],
        'week_start': value_or_default(policy, 'week_start', 'sun'),
        'weekly_limit': value_or_default(policy, 'weekly_limit_minutes', 2400),
        'special_44h_confirmed': value_or_default(policy, 'special_44h_confirmed', False),
        'legal_holiday': policy.get('legal_holiday_weekday'),     # None は手順3で拒否
        'ambiguity': value_or_default(policy, 'ambiguity', 'reject'),
        'worker_kind': value_or_default(policy, 'worker_kind', 'standard'),
        'holiday_system': value_or_default(policy, 'holiday_system', 'weekly'),
        'holiday_swaps': value_or_default(policy, 'holiday_swaps', []),
        'night_window': value_or_default(policy, 'night_window', '22-05'),
    }
    if settings['weekly_limit'] not in (2400, 2640):
        invalid('policy.weekly_limit_minutes: must be 2400 or 2640')
    if settings['ambiguity'] not in ('reject', 'use_default'):
        invalid('policy.ambiguity: must be reject or use_default')
    if settings['week_start'] not in WEEKDAY_NAMES:
        invalid('policy.week_start: must be sun..sat')
    if settings['legal_holiday'] is not None and settings['legal_holiday'] not in WEEKDAY_NAMES:
        invalid('policy.legal_holiday_weekday: must be sun..sat')
    # 第2版で表から外れた construction・sugar_manufacturing なども、ここで InvalidInput になる。
    if settings['worker_kind'] != 'standard' and settings['worker_kind'] not in UNSUPPORTED_WORKER_KIND:
        invalid('policy.worker_kind: undefined value')
    if settings['holiday_system'] not in ('weekly', *UNSUPPORTED_HOLIDAY_SYSTEM):
        invalid('policy.holiday_system: undefined value')
    if settings['night_window'] not in ('22-05', '23-06'):
        invalid('policy.night_window: undefined value')
    return settings


def value_or_default(policy, key, default):
    value = policy.get(key)
    return default if value is None else value


# ---------------------------------------------------------------- 手順3: 対応外

def step3_unsupported(policy, settings):
    """validation.md の表を上から順に見て、最初に当たったコードで拒否する（終了コード3）。"""
    if policy['work_system'] in UNSUPPORTED_WORK_SYSTEM:
        unsupported(UNSUPPORTED_WORK_SYSTEM[policy['work_system']],
                    'work_system is not the standard system (unsupported.md)')
    if settings['worker_kind'] in UNSUPPORTED_WORKER_KIND:
        unsupported(UNSUPPORTED_WORKER_KIND[settings['worker_kind']],
                    'worker_kind is not supported (unsupported.md)')
    if settings['holiday_system'] in UNSUPPORTED_HOLIDAY_SYSTEM:
        unsupported(UNSUPPORTED_HOLIDAY_SYSTEM[settings['holiday_system']],
                    'holiday_system is not supported (unsupported.md)')
    if len(settings['holiday_swaps']) > 0:
        unsupported('HolidaySwap', 'holiday swaps are not supported (unsupported.md)')
    if settings['night_window'] == '23-06':
        unsupported('NightWindowOverride', 'night window 23-06 is not supported (unsupported.md)')
    if settings['legal_holiday'] is None:
        unsupported('LegalHolidayUnspecified', 'legal_holiday_weekday is required (unsupported.md)')
    if settings['weekly_limit'] == 2640 and settings['special_44h_confirmed'] is not True:
        unsupported('Weekly44WithoutConfirmation', '44h week needs special_44h_confirmed (unsupported.md)')


# ---------------------------------------------------------------- 手順4: 勤務と日付の検査

def step4_shifts_and_dates(raw_shifts, raw_data_from):
    """日時・日付の書式・実在・範囲と、勤務と休憩の関係（validation.md 手順4）。

    日時・日付の文字列の書式の誤りは手順1ではなくここ（決着（旧Q1））。範囲は2000-01-01T00:00〜
    2099-12-31T23:59で、2100-01-01T00:00 に終わる勤務も不可（決着（旧Q3））。
    """
    shifts = []
    for i, raw in enumerate(raw_shifts):
        where = f'shifts[{i}]'
        start = parse_instant(raw['start'], f'{where}.start')
        end = parse_instant(raw['end'], f'{where}.end')
        if not start < end:
            invalid(f'{where}: start must be before end')
        if end - start >= MINUTES_PER_DAY:
            invalid(f'{where}: a shift must be shorter than 1440 minutes')
        breaks = []
        for j, pair in enumerate(raw.get('breaks') or []):
            b_start = parse_instant(pair[0], f'{where}.breaks[{j}][0]')
            b_end = parse_instant(pair[1], f'{where}.breaks[{j}][1]')
            if not b_start < b_end:
                invalid(f'{where}.breaks[{j}]: start must be before end')
            if not (start < b_start and b_end < end):
                invalid(f'{where}.breaks[{j}]: a break must be strictly inside the shift')
            breaks.append((b_start, b_end))
        breaks.sort()
        for j in range(1, len(breaks)):
            if breaks[j - 1][1] > breaks[j][0]:           # 接するのはよい（決着（旧Q4））
                invalid(f'{where}: breaks overlap')
        shifts.append({'start': start, 'end': end, 'breaks': breaks})
    shifts.sort(key=lambda s: (s['start'], s['end']))
    for i in range(1, len(shifts)):
        if shifts[i - 1]['end'] > shifts[i]['start']:     # 接する（等しい）のはよい
            invalid('shifts overlap')
    data_from = None if raw_data_from is None else parse_date(raw_data_from, 'data_from')
    return shifts, data_from


# ---------------------------------------------------------------- 手順5: 計算

def step5_compute(settings, shifts, data_from):
    legal = settings['legal_holiday']

    # classify.md 手順1: 各勤務の実労働を、1分ずつ並べる。労働日は勤務の開始の暦日。
    # 接する2つの勤務はつながず、別々の勤務・別々の労働日として数える（classify.md、決着（旧Q34））。
    work_by_shift = []
    for shift in shifts:
        labor_day = shift['start'] // MINUTES_PER_DAY
        minutes = []
        t = shift['start']
        while t < shift['end']:
            if not in_some_break(t, shift['breaks']):
                minutes.append(t)
            t += 1
        work_by_shift.append((labor_day, minutes))

    # 曖昧さ（validation.md「曖昧さの厳密な定義」）。D は実労働の分が属する暦日の集合。
    records = set()       # (日付, コード)。日付は日の番号
    for labor_day, minutes in work_by_shift:
        calendar_days = sorted({t // MINUTES_PER_DAY for t in minutes})
        if len(calendar_days) < 2:
            continue
        if any(weekday_name_of_day(d) == legal for d in calendar_days):
            records.add((labor_day, 'HolidayBoundary'))
        weeks = {week_start_of(date_of(d), settings['week_start']) for d in calendar_days}
        if len(weeks) >= 2:
            records.add((labor_day, 'WeekBoundary'))
        periods = {period_start_of(date_of(d), settings['month_start_day']) for d in calendar_days}
        if len(periods) >= 2:
            records.add((labor_day, 'MonthBoundary'))
    # IncompleteWeek: data_from があり、実労働のある労働日を含む週の起算日が data_from より前。
    # 記録の日付は週の起算日。data_from がなければ付かない。
    if data_from is not None:
        for labor_day, minutes in work_by_shift:
            if not minutes:
                continue
            week = week_start_of(date_of(labor_day), settings['week_start'])
            if week < data_from:
                records.add((day_number(week), 'IncompleteWeek'))
    ordered = sorted(records, key=lambda r: (r[0], AMBIGUITY_ORDER.index(r[1])))

    output = {'schema': SCHEMA_OUT, 'days': [], 'periods': [], 'ambiguities': [], 'refused': None}
    if ordered and settings['ambiguity'] == 'reject':
        # reject では ambiguities は空の配列（case-format.md、決着（旧Q7））。
        output['refused'] = {'code': 'AmbiguousBoundary',
                             'dates': sorted({date_of(d).isoformat() for d, _ in ordered})}
        return output
    output['ambiguities'] = [{'date': date_of(d).isoformat(), 'code': code} for d, code in ordered]

    minutes_by_labor_day = defaultdict(list)
    for labor_day, minutes in work_by_shift:
        minutes_by_labor_day[labor_day].extend(minutes)

    within_in_week = defaultdict(int)       # 週の初日 → それまでに「法定内」と数えた分
    overtime_in_period = defaultdict(int)   # 月の期間の初日 → それまでに数えた時間外の分
    period_rows = {}
    for labor_day in sorted(minutes_by_labor_day):       # classify.md 手順4: 労働日の順
        labor_date = date_of(labor_day)
        week = week_start_of(labor_date, settings['week_start'])          # 労働日が属する週
        period = period_start_of(labor_date, settings['month_start_day'])  # 労働日が属する期間
        row = dict.fromkeys(DAY_FIELDS, 0)
        within_today = 0
        for t in sorted(minutes_by_labor_day[labor_day]):  # classify.md 手順3: 時刻順
            calendar_day = t // MINUTES_PER_DAY
            minute_of_day = t % MINUTES_PER_DAY
            is_night = minute_of_day >= NIGHT_FROM or minute_of_day < NIGHT_UNTIL

            # classify.md 手順2: 暦日が法定休日の分は「法定休日」。
            if weekday_name_of_day(calendar_day) == legal:
                category = 'holiday'
            # classify.md 手順4: 先頭 min(n, 480, r) 分が「法定内」。1分ずつ見ると、
            # 今日の法定内が480分未満で、週の法定内が上限未満のあいだは法定内。
            elif within_today < DAY_LIMIT_MINUTES and within_in_week[week] < settings['weekly_limit']:
                category = 'within'
                within_today += 1
                within_in_week[week] += 1
            else:
                category = 'overtime'

            row['total'] += 1
            row[category] += 1
            if is_night:
                row['night'] += 1
                row['night_' + category] += 1
            if category == 'overtime':
                # classify.md 手順5: 月の期間ごとに時刻順に数え、3601分目から60時間超。
                overtime_in_period[period] += 1
                if overtime_in_period[period] > OVER60_THRESHOLD:
                    row['over60'] += 1
                    if is_night:
                        row['night_over60'] += 1

        # classify.md 手順6: 所定外法定内は、法定内のうち所定労働時間を超える分（0未満にしない）。
        row['extra_within'] = max(0, row['within'] - settings['scheduled'])
        output['days'].append({'date': labor_date.isoformat(), **row})

        # periods は実労働のある期間だけ（case-format.md、決着（旧Q6））。
        totals = period_rows.setdefault(period, dict.fromkeys(PERIOD_FIELDS, 0))
        for key in PERIOD_FIELDS:
            totals[key] += row[key]
    output['periods'] = [{'start': p.isoformat(), **period_rows[p]} for p in sorted(period_rows)]
    # classify.v1 には notices の欄を作らない（unsupported.md、決着（旧Q8））。
    return output


def in_some_break(t, breaks):
    for b_start, b_end in breaks:
        if b_start <= t < b_end:
            return True
    return False


if __name__ == '__main__':
    sys.exit(cli(classify))
