# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""オラクル: 36協定のケース kintai.case36.v1 を読み、kintai.limits36.v1 を出す（仕様 第2版）。

使い方: python3 tools/oracle/limits36_oracle.py FILE

docs/spec/limits36.md の「判定」1〜9と docs/spec/case-format-36-leave.md の形式を、
月ごとに1か月ずつ素朴に数えて書いた。平均は割らず、合計で比べる。
検査の順序は、区分（docs/spec/validation.md）と同じ流儀で、形と型 → 列挙と範囲 → 日付 → 計算。
36協定のケースには対応外の指定がない（終了コード3を返す場合がない）。


対応する受け入れ条件: AC-05.11
"""
import sys

sys.dont_write_bytecode = True   # .gitignore に __pycache__ が無いので、作業ツリーを汚さない

from common import (add_months_same_day, check_list, check_object, check_type, cli, invalid,
                    is_int, is_str, parse_date)

SCHEMA_IN = 'kintai.case36.v1'
SCHEMA_OUT = 'kintai.limits36.v1'

AGREEMENT_FIELDS = ['kind', 'year_start_month', 'monthly_limit_minutes', 'annual_limit_minutes',
                    'special_monthly_total_limit_minutes', 'special_annual_limit_minutes',
                    'special_months_max']
# case-format-36-leave.md: 限度は0以上で、法定の最大（2700・21600・5999・43200・6回）を超えたら不正な入力。
LEGAL_MAXIMUM = {
    'monthly_limit_minutes': 2700, 'annual_limit_minutes': 21600,
    'special_monthly_total_limit_minutes': 5999, 'special_annual_limit_minutes': 43200,
    'special_months_max': 6,
}
# case-format-36-leave.md: 限度の項目は none では不要、general は月・年の限度、special は特別条項の
# 4項目も必須。year_start_month は none でも必須（決着（旧Q9））。
REQUIRED_BY_KIND = {
    'none': ['year_start_month'],
    'general': ['year_start_month', 'monthly_limit_minutes', 'annual_limit_minutes'],
    'special': ['year_start_month', 'monthly_limit_minutes', 'annual_limit_minutes',
                'special_monthly_total_limit_minutes', 'special_annual_limit_minutes',
                'special_months_max'],
}
MINUTES_IN_LONGEST_MONTH = 44640   # 31日×1440分。O＋H と history の total の上限
STATUTORY_MONTHLY = 2700           # 法定の月45時間。O がこれを超えた月が「発動月」
STATUTORY_ANNUAL = 21600           # 法定の年360時間
MONTHLY_TOTAL_LIMIT = 6000         # 判定2: T < 6000分（6000分ちょうどは違反）
AVERAGE_LIMIT_PER_MONTH = 4800     # 判定2: 直近 n か月の T の合計 ≤ 4800 × n
WINDOW_SIZES = [2, 3, 4, 5, 6]


def limits36(case):
    """36協定のケース（kintai.case36.v1）を判定する。"""
    agreement, months_raw, history_raw = step1_structure(case)
    step2_values(agreement, months_raw, history_raw)
    months, history = step4_dates(months_raw, history_raw)
    return step5_compute(agreement, months, history)


# ---------------------------------------------------------------- 手順1: 形と型

def step1_structure(case):
    # history は必須の配列（空でもよい）。null は型の誤り（case-format-36-leave.md、決着（旧Q14））。
    check_object(case, 'case', ['schema', 'agreement', 'months', 'history'],
                 ['schema', 'agreement', 'months', 'history'])
    if case['schema'] != SCHEMA_IN or not is_str(case['schema']):
        invalid(f'schema must be {SCHEMA_IN}')
    agreement = check_object(case['agreement'], 'agreement', AGREEMENT_FIELDS, ['kind'])
    check_type(agreement['kind'], is_str, 'agreement.kind')
    for key in AGREEMENT_FIELDS[1:]:
        check_type(agreement.get(key), is_int, f'agreement.{key}', nullable=True)

    months = check_list(case['months'], 'months')
    for i, month in enumerate(months):
        where = f'months[{i}]'
        check_object(month, where, ['start', 'overtime', 'holiday'], ['start', 'overtime', 'holiday'])
        check_type(month['start'], is_str, f'{where}.start')
        check_type(month['overtime'], is_int, f'{where}.overtime')
        check_type(month['holiday'], is_int, f'{where}.holiday')

    history = check_list(case['history'], 'history')
    for i, entry in enumerate(history):
        where = f'history[{i}]'
        check_object(entry, where, ['start', 'total'], ['start', 'total'])
        check_type(entry['start'], is_str, f'{where}.start')
        check_type(entry['total'], is_int, f'{where}.total')
    return agreement, months, history


# ---------------------------------------------------------------- 手順2: 列挙と範囲

def step2_values(agreement, months, history):
    kind = agreement['kind']
    if kind not in REQUIRED_BY_KIND:
        invalid('agreement.kind must be none, general or special')
    for key in REQUIRED_BY_KIND[kind]:
        if agreement.get(key) is None:
            invalid(f'agreement.{key} is required for kind {kind}')
    # その kind で使わない限度の項目も、範囲だけ検査する（決着（旧Q16））。限度は0以上（決着（旧Q17））。
    if agreement.get('year_start_month') is not None and not 1 <= agreement['year_start_month'] <= 12:
        invalid('agreement.year_start_month must be 1..12')
    for key, maximum in LEGAL_MAXIMUM.items():
        value = agreement.get(key)
        if value is not None and not 0 <= value <= maximum:
            invalid(f'agreement.{key} must be 0..{maximum}')
    # O と H は0以上の整数で、合わせて44640分（31日×1440分）以下。history の total は0〜44640分。
    # 超えたら手順2の範囲の検査で InvalidInput（case-format-36-leave.md、決着（旧Q46））。
    for i, month in enumerate(months):
        if month['overtime'] < 0 or month['holiday'] < 0:
            invalid(f'months[{i}]: minutes must not be negative')
        if month['overtime'] + month['holiday'] > MINUTES_IN_LONGEST_MONTH:
            invalid(f'months[{i}]: overtime + holiday must be at most {MINUTES_IN_LONGEST_MONTH}')
    for i, entry in enumerate(history):
        if not 0 <= entry['total'] <= MINUTES_IN_LONGEST_MONTH:
            invalid(f'history[{i}]: total must be 0..{MINUTES_IN_LONGEST_MONTH}')


# ---------------------------------------------------------------- 手順4: 日付

def step4_dates(months_raw, history_raw):
    # months は空でもよい（決着（旧Q15））。
    months = []
    for i, month in enumerate(months_raw):
        start = parse_date(month['start'], f'months[{i}].start')
        if start.day > 28:            # 協定月の初日が29〜31日は不正な入力（決着（旧Q13））
            invalid(f'months[{i}].start: the day must be 1..28')
        if months and start != add_months_same_day(months[-1]['start'], 1):
            invalid(f'months[{i}].start: months must be consecutive agreement months in order')
        months.append({'start': start, 'O': month['overtime'], 'H': month['holiday']})

    history = {}
    for i, entry in enumerate(history_raw):
        start = parse_date(entry['start'], f'history[{i}].start')
        # 協定月の初日でない日付、months の最初の月以後の月、同じ月の2回は不正な入力。並びは問わない
        # （決着（旧Q14））。months が空のときは、日が1〜28であることだけを見る（決着（旧Q40））。
        if start.day > 28:
            invalid(f'history[{i}].start: the day must be 1..28')
        if months:
            if start.day != months[0]['start'].day:
                invalid(f'history[{i}].start: not the first day of an agreement month')
            if not start < months[0]['start']:
                invalid(f'history[{i}].start: history must be before the first month')
        if start in history:
            invalid(f'history[{i}].start: duplicated month')
        history[start] = entry['total']
    return months, history


# ---------------------------------------------------------------- 手順5: 判定

def step5_compute(agreement, months, history):
    kind = agreement['kind']
    year_start_month = agreement['year_start_month']
    monthly_limit = agreement.get('monthly_limit_minutes')
    annual_limit = agreement.get('annual_limit_minutes')

    def total_of(index):
        """months[index] の T。index が負なら、その協定月の履歴の T。どちらも無ければ None。"""
        if index >= 0:
            return months[index]['O'] + months[index]['H']
        # months[0] が2000年の前半なら、2000年より前の月を探すことになる。履歴にその月は書けない
        # （入力の範囲外）ので、None（UnknownHistory）になる（決着（旧Q32））。
        start = add_months_same_day(months[0]['start'], index)
        return history.get(start)

    out_months = []
    year_overtime = 0
    special_count = 0
    # 判定9: months の最初の月が協定年の途中なら、その年の ΣO と発動月の数は months の月だけの
    # 下限値で、year_overtime_known は偽（決着（旧Q10））。協定年の初月が来たら真になる。
    year_known = bool(months) and months[0]['start'].month == year_start_month
    for i, month in enumerate(months):
        O, H = month['O'], month['H']
        T = O + H

        # 判定7: 協定年が替わると、ΣO と発動月の数は0に戻る。
        if month['start'].month == year_start_month:
            year_overtime = 0
            special_count = 0
            year_known = True
        year_overtime += O
        is_activation_month = O > STATUTORY_MONTHLY        # 発動月: O > 2700分
        if is_activation_month:
            special_count += 1

        # 判定2・6: 直近 n か月の T の合計。データも履歴も無い月があれば None（UnknownHistory）。
        sums = {}
        for n in WINDOW_SIZES:
            values = [total_of(k) for k in range(i - n + 1, i + 1)]
            sums[str(n)] = None if any(v is None for v in values) else sum(values)

        # findings は case-format-36-leave.md のコードの順。year_overtime_known が偽のときも、
        # 下限値に同じ規則を当てる。下限値が超えていれば、真の値も必ず超えている（判定9、決着（旧Q39））。
        findings = []
        if kind == 'none' and (O > 0 or H > 0):                                     # 判定1
            findings.append({'code': 'NoAgreement'})
        if kind == 'general' and O > STATUTORY_MONTHLY:                             # 判定4
            findings.append({'code': 'MonthlyOvertimeExceeded'})
        if kind in ('general', 'special') and O <= STATUTORY_MONTHLY and O > monthly_limit:  # 判定3
            findings.append({'code': 'GeneralMonthlyLimitExceeded'})
        # 判定8: 年の累計の違反は、超えた月から協定年の終わりまで毎月付く（O が0の月でも）。
        if kind == 'general' and year_overtime > STATUTORY_ANNUAL:                  # 判定4
            findings.append({'code': 'AnnualOvertimeExceeded'})
        if kind == 'general' and year_overtime <= STATUTORY_ANNUAL and year_overtime > annual_limit:
            findings.append({'code': 'GeneralAnnualLimitExceeded'})
        # 判定5・8: SpecialCountExceeded は、回数が超えた発動月だけに付く。
        if kind == 'special' and is_activation_month and special_count > agreement['special_months_max']:
            findings.append({'code': 'SpecialCountExceeded'})
        if kind == 'special' and is_activation_month and T > agreement['special_monthly_total_limit_minutes']:
            findings.append({'code': 'SpecialMonthlyTotalExceeded'})
        if kind == 'special' and year_overtime > agreement['special_annual_limit_minutes']:
            findings.append({'code': 'SpecialAnnualExceeded'})
        if T >= MONTHLY_TOTAL_LIMIT:                                                # 判定2
            findings.append({'code': 'MonthlyTotalReached100h'})
        for n in WINDOW_SIZES:
            if sums[str(n)] is not None and sums[str(n)] > AVERAGE_LIMIT_PER_MONTH * n:
                findings.append({'code': 'MultiMonthSumExceeded', 'n': n})
        for n in WINDOW_SIZES:                                                      # 判定6
            if sums[str(n)] is None:
                findings.append({'code': 'UnknownHistory', 'n': n})

        out_months.append({
            'start': month['start'].isoformat(),
            'total': T,
            'sums': sums,
            'special_count': special_count,
            'year_overtime': year_overtime,
            'year_overtime_known': year_known,
            'findings': findings,
        })
    # notices は常に OwnEmployerOnly（limits36.md「副業・兼業先と転職前の時間」）。
    return {'schema': SCHEMA_OUT, 'months': out_months, 'notices': ['OwnEmployerOnly']}


if __name__ == '__main__':
    sys.exit(cli(limits36))
