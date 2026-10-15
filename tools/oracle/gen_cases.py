# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""無作為のケースを作る（区分・36協定・年休、仕様 第2版）。境界を狙う生成を含む。

使い方:
  python3 tools/oracle/gen_cases.py --seed 1 --cases 20 --out DIR [--kind classify]
  （DIR に <種類>-<番号>.json と、生成の意図を並べた manifest.json を書く）

ケースは「乱数の種・種類・番号」だけで決まる。同じ3つなら、いつでも同じケースになる。
生成の意図（タグ）はケースのJSONには入れない（知らない項目は不正な入力になるため）。

intended（生成の意図）の値:
  'valid'    終了コード0を意図した有効なケース
  'refused'  曖昧さの拒否（終了コード4）を意図
  'any'      0か4のどちらでもよい（無作為の勤務で、またぐ勤務が偶然できる場合）
  'InvalidInput' / 対応外のコード   その誤りを意図
  'probe'    仕様の読みで答えが変わるケース（タグに疑問の番号 Qn を書く）
"""
import argparse
import datetime as dt
import json
import os
import random
import sys

sys.dont_write_bytecode = True   # .gitignore に __pycache__ が無いので、作業ツリーを汚さない

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from common import (add_months_same_day, date_of, day_number, next_start,  # noqa: E402
                    period_end, week_start_of)

KINDS = ['classify', 'case36', 'leave']
WEEKDAYS = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun']
DAY = 1440
FIRST_DAY = dt.date(2000, 1, 1)


def generate(kind, seed, index):
    """1件を作る。戻り値は {'kind','index','tag','intended','data'(bytes),'case'(dict|None)}。"""
    rng = random.Random(f'kintai-difftest:{kind}:{seed}:{index}')
    builder = {'classify': gen_classify, 'case36': gen_case36, 'leave': gen_leave}[kind]
    result = builder(rng)
    if isinstance(result['case'], (bytes, str)):
        data = result['case'] if isinstance(result['case'], bytes) else result['case'].encode('utf-8')
        case = None
    else:
        case = result['case']
        data = json.dumps(case, ensure_ascii=False, indent=1).encode('utf-8')
    return {'kind': kind, 'index': index, 'tag': result['tag'], 'intended': result['intended'],
            'data': data, 'case': case}


def pick(rng, table):
    """[(重み, 値), ...] から重みつきで1つ選ぶ。"""
    total = sum(weight for weight, _ in table)
    r = rng.random() * total
    for weight, value in table:
        r -= weight
        if r < 0:
            return value
    return table[-1][1]


def rand_date(rng, low=dt.date(2000, 1, 10), high=dt.date(2099, 10, 31)):
    return low + dt.timedelta(days=rng.randrange((high - low).days + 1))


def fmt_t(t, sep='T'):
    """1970-01-01 00:00 からの分を、YYYY-MM-DDTHH:MM にする。"""
    m = t % DAY
    return f'{date_of(t // DAY).isoformat()}{sep}{m // 60:02d}:{m % 60:02d}'


def dn(date):
    return day_number(date)


def wd(date):
    return WEEKDAYS[date.weekday()]


def result(case, tag, intended):
    return {'case': case, 'tag': tag, 'intended': intended}


SENTINEL = 987654321


def with_literal(case, setter, literal):
    """setter で目印の数を入れ、JSON の文字列でその数を literal（-0、1e2、NaN など）に置きかえる。"""
    setter(SENTINEL)
    return json.dumps(case, ensure_ascii=True).replace(str(SENTINEL), literal, 1)


def nested_arrays(count):
    """count 個の配列を入れ子にしたもの（[[[...]]]）。"""
    value = []
    for _ in range(count - 1):
        value = [value]
    return value


def nesting_level(value, level=1):
    """生成用: いちばん外の配列・オブジェクトを1段目として、いちばん深い段を数える。"""
    if isinstance(value, dict):
        return max([level] + [nesting_level(v, level + 1) for v in value.values()])
    if isinstance(value, list):
        return max([level] + [nesting_level(v, level + 1) for v in value])
    return 0


def duplicate_key_text(case, rng):
    """同じ名前の項目が2回あるJSONの文字列（validation.md 手順1で不正な入力）。"""
    text = json.dumps(case, ensure_ascii=False)
    if rng.random() < 0.5:
        return '{"schema": ' + json.dumps(case['schema']) + ', ' + text[1:]     # 同じ値の重複
    return text[:-1] + ', "schema": "again"}'


# ======================================================================== 区分

def random_policy(rng, **force):
    policy = {
        'work_system': 'standard',
        'scheduled_minutes_per_day': pick(rng, [(3, 480), (3, 420), (2, 450), (1, 465), (1, 360),
                                                (1, 240), (1, 0), (2, rng.randint(0, 480))]),
        'week_start': pick(rng, [(5, 'sun'), (3, 'mon'), (2, rng.choice(WEEKDAYS))]),
        'weekly_limit_minutes': 2400,
        'special_44h_confirmed': False,
        'legal_holiday_weekday': pick(rng, [(5, 'sun'), (2, 'sat'), (3, rng.choice(WEEKDAYS))]),
        'month_start_day': pick(rng, [(5, 1), (2, 16), (1, 21), (1, 26), (1, 28), (3, rng.randint(1, 28))]),
        'ambiguity': rng.choice(['reject', 'use_default']),
    }
    if rng.random() < 0.15:
        policy['weekly_limit_minutes'] = 2640
        policy['special_44h_confirmed'] = True
    policy.update(force)
    # 既定値の項目は、ときどき省くか null にする（どちらも項目がないのと同じ）。
    for key, default in (('week_start', 'sun'), ('weekly_limit_minutes', 2400),
                         ('special_44h_confirmed', False), ('ambiguity', 'reject')):
        if key in policy and policy[key] == default and key not in force:
            r = rng.random()
            if r < 0.2:
                del policy[key]
            elif r < 0.25:
                policy[key] = None
    # 任意の項目を、既定値のまま明示する・null にすることもある。
    for key, default in (('worker_kind', 'standard'), ('holiday_system', 'weekly'),
                         ('night_window', '22-05'), ('holiday_swaps', [])):
        if key not in policy:
            r = rng.random()
            if r < 0.08:
                policy[key] = default
            elif r < 0.1:
                policy[key] = None
    return policy


def effective(policy, key, default):
    value = policy.get(key)
    return default if value is None else value


def random_breaks(rng, start, end):
    length = end - start
    if length < 3 or rng.random() < 0.4:
        return []
    if length >= 300 and rng.random() < 0.6:
        b_len = rng.choice([45, 60, 60, 90])
        b_start = start + rng.randint(length // 3, length // 2)
        b_end = min(b_start + b_len, end - 1)
        return [(b_start, b_end)] if b_start < b_end else []
    count = rng.choice([1, 1, 2, 3])
    points = sorted(rng.sample(range(start + 1, end), min(2 * count, length - 1)))
    breaks = [(points[2 * i], points[2 * i + 1]) for i in range(len(points) // 2)]
    if len(breaks) >= 2 and rng.random() < 0.2:      # 接する休憩（許される。決着（旧Q4））
        breaks[1] = (breaks[0][1], breaks[1][1])
    return breaks


def make_shift(rng, start, end, breaks=None, sep='T'):
    if breaks is None:
        breaks = random_breaks(rng, start, end)
    shift = {'start': fmt_t(start, sep), 'end': fmt_t(end, sep)}
    if breaks:
        shift['breaks'] = [[fmt_t(a, sep), fmt_t(b, sep)] for a, b in breaks]
    else:
        r = rng.random()
        if r < 0.25:
            shift['breaks'] = []
        elif r < 0.3:
            shift['breaks'] = None        # null は項目がないのと同じ（決着（旧Q5））
    return shift


def labor_week_starts(shifts, week_start):
    """勤務の開始の暦日（労働日）が属する週の起算日の集合。"""
    starts = set()
    for shift in shifts:
        labor_date = dt.date.fromisoformat(shift['start'][:10])
        starts.add(week_start_of(labor_date, week_start))
    return starts


def classify_case(policy, shifts, rng=None, shuffle=True, data_from='auto'):
    """ケースを組み立てる。data_from='auto' なら、ときどき「そろっている」data_from を付ける。"""
    shifts = list(shifts)
    if rng is not None and shuffle and rng.random() < 0.3:
        rng.shuffle(shifts)      # 入力の順は問わない（オラクルは並べ替える）
    case = {'schema': 'kintai.case.v1', 'policy': policy, 'shifts': shifts}
    if data_from == 'auto':
        data_from = None
        if rng is not None and shifts and rng.random() < 0.3:
            earliest = min(labor_week_starts(shifts, effective(policy, 'week_start', 'sun')))
            candidate = earliest - dt.timedelta(days=pick(rng, [(3, 0), (2, rng.randint(1, 60))]))
            if candidate >= FIRST_DAY:     # 週の起算日以前なので IncompleteWeek にならない
                data_from = candidate.isoformat()
    if data_from is not None:
        case['data_from'] = data_from
    return case


def simple_day_shift(rng, day_no):
    """暦日をまたがない、ふつうの勤務（6:00〜12:59開始、長さ1〜600分）。"""
    start = day_no * DAY + rng.randint(6 * 60, 12 * 60 + 59)
    length = pick(rng, [(3, 480), (2, 540), (1, 420), (1, 600), (1, 240), (2, rng.randint(1, 600))])
    return start, min(start + length, (day_no + 1) * DAY)


def filler(rng, day_nos):
    """指定した日に、暦日をまたがない勤務を足す（曖昧さを作らない）。"""
    shifts = []
    for d in day_nos:
        if rng.random() < 0.7:
            s, e = simple_day_shift(rng, d)
            shifts.append(make_shift(rng, s, e))
    return shifts


def find_date(rng, ok, low=dt.date(2000, 1, 10), high=dt.date(2099, 10, 31)):
    for _ in range(500):
        d = rand_date(rng, low, high)
        if ok(d):
            return d
    raise RuntimeError('no date found')


def scenario_basic(rng):
    policy = random_policy(rng)
    first = rand_date(rng)
    n_days = pick(rng, [(2, 1), (2, 3), (4, 7), (3, 14), (2, 21), (2, 35)])
    shifts = []
    cursor = dn(first) * DAY
    for i in range(n_days):
        day_no = dn(first) + i
        if rng.random() < 0.25:
            continue
        for _ in range(1 if rng.random() < 0.88 else 2):
            start_min = pick(rng, [(2, 540), (1, 480), (1, 420), (1, 360), (1, 600), (1, 780), (1, 1020),
                                   (1, 1260), (1, 1320), (2, rng.randrange(DAY))])
            start = max(day_no * DAY + start_min, cursor)
            length = pick(rng, [(1, 1), (1, 60), (1, 240), (2, 480), (1, 479), (1, 481), (2, 540),
                                (1, 600), (1, 660), (1, 720), (1, 840), (3, rng.randint(1, 1439))])
            sep = ' ' if rng.random() < 0.05 else 'T'      # 区切りは T か空白（決着（旧Q2））
            shifts.append(make_shift(rng, start, start + length, sep=sep))
            cursor = start + length + pick(rng, [(1, 0), (3, rng.randint(1, 600))])
    return result(classify_case(policy, shifts, rng), 'basic', 'any')


def scenario_cross_plain(rng):
    """暦日をまたぐが、休日・週・月の境界はまたがない勤務（曖昧さなし）。"""
    policy = random_policy(rng)
    legal, wstart, msd = (policy['legal_holiday_weekday'], effective(policy, 'week_start', 'sun'),
                          policy['month_start_day'])
    nxt = lambda d: d + dt.timedelta(days=1)   # noqa: E731
    d = find_date(rng, lambda d: wd(d) != legal and wd(nxt(d)) != legal and wd(nxt(d)) != wstart
                  and nxt(d).day != msd)
    start = dn(d) * DAY + rng.randint(15 * 60, 23 * 60 + 59)
    end = min(dn(nxt(d)) * DAY + pick(rng, [(1, 1), (2, 300), (1, 480), (2, rng.randint(1, 600))]),
              start + 1439)
    shifts = [make_shift(rng, start, end)]
    shifts += filler(rng, [dn(d) - k for k in range(1, rng.randint(1, 6))])
    return result(classify_case(policy, shifts, rng), 'cross-midnight', 'valid')


NIGHT_PATTERNS = [(17 * 60, 22 * 60), (22 * 60, DAY), (0, 5 * 60), (5 * 60, 9 * 60), (21 * 60, DAY),
                  (60, 5 * 60 + 1), (21 * 60 + 59, 22 * 60 + 1), (4 * 60 + 59, 5 * 60 + 1),
                  (0, 22 * 60), (5 * 60, DAY), (13 * 60, 22 * 60), (22 * 60, 23 * 60 + 59)]


def scenario_night_edges(rng):
    """22:00・5:00・24:00 ちょうどに接する勤務。24:00に終わった翌日に0:00から始める勤務も。"""
    policy = random_policy(rng)
    first = rand_date(rng)
    shifts = []
    for i in range(rng.randint(2, 7)):
        a, b = rng.choice(NIGHT_PATTERNS)
        s = (dn(first) + i) * DAY + a
        shifts.append(make_shift(rng, s, (dn(first) + i) * DAY + b, breaks=[]))
    # 前の日の 24:00 に終わり、次の日の 0:00 に始まる（接するが、つながない。決着（旧Q34））。
    if rng.random() < 0.5:
        k = len(shifts)
        shifts.append(make_shift(rng, (dn(first) + k) * DAY + 22 * 60, (dn(first) + k + 1) * DAY, breaks=[]))
        shifts.append(make_shift(rng, (dn(first) + k + 1) * DAY, (dn(first) + k + 1) * DAY + 6 * 60,
                                 breaks=[]))
    return result(classify_case(policy, shifts, rng), 'night-edges', 'valid')


def scenario_day_480(rng):
    """1日の平日型の分が 479・480・481（分割した勤務の合計も含む）。"""
    policy = random_policy(rng)
    legal = policy['legal_holiday_weekday']
    d = find_date(rng, lambda d: wd(d) != legal)
    n = pick(rng, [(3, 480), (3, 481), (2, 479), (1, 960), (1, 482)])
    base = dn(d) * DAY
    shifts = []
    # どれも暦日をまたがないように、開始を早める（またぐと曖昧さの拒否になりうる）。
    if rng.random() < 0.6:
        b_len = rng.choice([0, 45, 60])
        start = base + min(rng.randint(6 * 60, 9 * 60), DAY - (n + b_len))
        breaks = [(start + 180, start + 180 + b_len)] if b_len else []
        shifts.append(make_shift(rng, start, start + n + b_len, breaks=breaks))
        tag = f'day-{n}'
    else:
        first_part = rng.randint(1, n - 1)
        gap = rng.randint(0, 120)
        s1 = base + min(rng.randint(6 * 60, 8 * 60), DAY - (n + gap))
        s2 = s1 + first_part + gap
        shifts.append(make_shift(rng, s1, s1 + first_part, breaks=[]))
        shifts.append(make_shift(rng, s2, s2 + n - first_part, breaks=[]))
        tag = f'day-{n}-split'
    shifts += filler(rng, [dn(d) - k for k in range(1, rng.randint(1, 5))])
    return result(classify_case(policy, shifts, rng), tag, 'valid')


def scenario_week_limit(rng):
    """週の法定内が、上限（2400か2640）ちょうど、その1分前、1分超など。"""
    policy = random_policy(rng)
    legal, wstart = policy['legal_holiday_weekday'], effective(policy, 'week_start', 'sun')
    limit = effective(policy, 'weekly_limit_minutes', 2400)
    w = find_date(rng, lambda d: wd(d) == wstart)
    days = [w + dt.timedelta(days=i) for i in range(7) if wd(w + dt.timedelta(days=i)) != legal]
    amounts = [480] * len(days)
    surplus = sum(amounts) - limit       # 6日×480 − 上限 を、どこかの日から減らす
    while surplus > 0:
        i = rng.randrange(len(amounts))
        cut = min(surplus, amounts[i], rng.randint(1, 480))
        amounts[i] -= cut
        surplus -= cut
    delta = pick(rng, [(3, 0), (3, 1), (2, -1), (1, 60), (1, 481)])
    amounts[-1] += delta
    if amounts[-1] < 0:
        amounts[-2] += amounts[-1]
        amounts[-1] = 0
    shifts = []
    for d, minutes in zip(days, amounts):
        if minutes <= 0:
            continue
        with_break = minutes > 300 and rng.random() < 0.5
        length = minutes + (60 if with_break else 0)
        start = dn(d) * DAY + min(rng.randint(5 * 60, 8 * 60), DAY - length)
        breaks = [(start + 240, start + 300)] if with_break else []
        shifts.append(make_shift(rng, start, start + length, breaks=breaks))
    # 週の全体がそろっていることを data_from で明示することがある（週の起算日ちょうど）。
    data_from = w.isoformat() if rng.random() < 0.3 else 'auto'
    return result(classify_case(policy, shifts, rng, data_from=data_from),
                  f'week-limit{limit}{delta:+d}', 'valid')


def scenario_over60(rng):
    """月の期間の時間外が3600分の近く（ちょうど、1分前、1分超など）。次の期間で累計が戻ることも見る。"""
    policy = random_policy(rng)
    legal, msd = policy['legal_holiday_weekday'], policy['month_start_day']
    rest = {legal, WEEKDAYS[(WEEKDAYS.index(legal) - 1) % 7]}
    p = find_date(rng, lambda d: d.day == msd, high=dt.date(2099, 9, 30))
    p_next = add_months_same_day(p, 1)
    work = [p + dt.timedelta(days=i) for i in range((p_next - p).days) if wd(p + dt.timedelta(days=i)) not in rest]
    target = pick(rng, [(3, 3600), (3, 3601), (2, 3599), (1, 3602), (1, 3540), (1, 3660),
                        (2, rng.randint(3300, 3900)), (1, rng.randint(4000, 7000))])
    overtime = [target // len(work) + (1 if i < target % len(work) else 0) for i in range(len(work))]
    shifts = []
    for d, ot in zip(work, overtime):
        length = 480 + ot + 60
        start = dn(d) * DAY + min(DAY - length, rng.choice([7 * 60, 8 * 60, 9 * 60, 13 * 60, 14 * 60]))
        breaks = [(start + 240, start + 300)]
        shifts.append(make_shift(rng, start, start + length, breaks=breaks))
    for i in range(rng.randint(0, 4)):    # 次の期間にも少し時間外を付ける（累計が0に戻る）
        d = p_next + dt.timedelta(days=i)
        if wd(d) in rest:
            continue
        start = dn(d) * DAY + 8 * 60
        shifts.append(make_shift(rng, start, start + 480 + 120 + 60, breaks=[(start + 240, start + 300)]))
    return result(classify_case(policy, shifts, rng), f'over60-target{target}', 'valid')


def crossing_scenario(rng, name, predicate):
    """predicate(d) を満たす日 d に始まり、翌日にまたぐ勤務（曖昧さの3つのコードを狙う）。"""
    policy = random_policy(rng)
    d = find_date(rng, lambda d: predicate(policy, d))
    start = dn(d) * DAY + rng.randint(16 * 60, 23 * 60 + 59)
    end = min(dn(d) * DAY + DAY + pick(rng, [(1, 1), (3, rng.randint(1, 600))]), start + 1439)
    shifts = [make_shift(rng, start, end)]
    shifts += filler(rng, [dn(d) - k for k in range(1, rng.randint(1, 8))])
    shifts += filler(rng, [dn(d) + k for k in range(2, rng.randint(2, 6))])
    intended = 'refused' if effective(policy, 'ambiguity', 'reject') == 'reject' else 'valid'
    return result(classify_case(policy, shifts, rng), name, intended)


def scenario_holiday_cross(rng):
    if rng.random() < 0.5:
        return crossing_scenario(rng, 'cross-into-legal-holiday', lambda p, d:
                                 wd(d + dt.timedelta(days=1)) == p['legal_holiday_weekday'])
    return crossing_scenario(rng, 'cross-out-of-legal-holiday', lambda p, d:
                             wd(d) == p['legal_holiday_weekday'])


def scenario_week_cross(rng):
    return crossing_scenario(rng, 'cross-week-boundary', lambda p, d:
                             wd(d + dt.timedelta(days=1)) == effective(p, 'week_start', 'sun'))


def scenario_month_cross(rng):
    return crossing_scenario(rng, 'cross-month-period-boundary', lambda p, d:
                             (d + dt.timedelta(days=1)).day == p['month_start_day'])


def scenario_incomplete_week(rng):
    """data_from が週の起算日より後（IncompleteWeek）、ちょうど起算日、すべての勤務より後など。"""
    policy = random_policy(rng)
    wstart = effective(policy, 'week_start', 'sun')
    near_2000 = rng.random() < 0.1
    if near_2000:      # 週の起算日が2000年より前になる（記録の日付が入力の範囲外になる）
        w = week_start_of(dt.date(2000, 1, rng.randint(1, 6)), wstart)
    else:
        w = find_date(rng, lambda d: wd(d) == wstart, high=dt.date(2099, 9, 30))
    span = rng.randint(3, 20)
    day_list = [max(w, FIRST_DAY) + dt.timedelta(days=i) for i in range(span)]   # 勤務は2000年以後だけ
    shifts = []
    for d in day_list:
        if rng.random() < 0.7 or not shifts:
            s, e = simple_day_shift(rng, dn(d))
            shifts.append(make_shift(rng, s, e))
    variant = pick(rng, [(4, 'inside-first-week'), (2, 'exact-week-start'), (1, 'after-all-shifts'),
                         (1, 'long-before'), (1, 'second-week-start')])
    first_labor = min(dt.date.fromisoformat(s['start'][:10]) for s in shifts)
    if variant == 'inside-first-week':
        data_from = max(FIRST_DAY, week_start_of(first_labor, wstart) + dt.timedelta(days=rng.randint(1, 6)))
    elif variant == 'exact-week-start':
        data_from = max(FIRST_DAY, week_start_of(first_labor, wstart))
    elif variant == 'after-all-shifts':
        last = max(dt.date.fromisoformat(s['start'][:10]) for s in shifts)
        data_from = last + dt.timedelta(days=rng.randint(1, 30))
    elif variant == 'long-before':
        data_from = max(FIRST_DAY, w - dt.timedelta(days=rng.randint(1, 400)))
    else:
        data_from = w + dt.timedelta(days=7)
    incomplete = any(start < data_from for start in labor_week_starts(shifts, wstart))
    if not incomplete:
        intended = 'valid'
    else:
        intended = 'refused' if effective(policy, 'ambiguity', 'reject') == 'reject' else 'valid'
    tag = f'incomplete-week-{variant}' + ('-near-2000' if near_2000 else '')
    return result(classify_case(policy, shifts, rng, data_from=data_from.isoformat()), tag, intended)


def scenario_holiday_long(rng):
    """法定休日に8時間を超えて働く（全部が法定休日、時間外にしない）。法定外休日の勤務も。"""
    policy = random_policy(rng)
    legal = policy['legal_holiday_weekday']
    d = find_date(rng, lambda d: wd(d) == legal)
    start = dn(d) * DAY + rng.randint(5 * 60, 9 * 60)
    end = min(start + rng.randint(481, 1000), (dn(d) + 1) * DAY)
    shifts = [make_shift(rng, start, end)]
    rest = d - dt.timedelta(days=1)      # 法定休日の前日（法定外休日として働かせる）
    s2 = dn(rest) * DAY + 8 * 60
    shifts.append(make_shift(rng, s2, s2 + rng.randint(300, 700)))
    shifts += filler(rng, [dn(d) - k for k in range(2, 7)])
    return result(classify_case(policy, shifts, rng), 'legal-holiday-long', 'valid')


def scenario_week_spans_periods(rng):
    """月の期間の境界をまたぐ週に長く働く（週は月をまたいで続けて数える）。"""
    policy = random_policy(rng)
    legal, wstart, msd = (policy['legal_holiday_weekday'], effective(policy, 'week_start', 'sun'),
                          policy['month_start_day'])
    p = find_date(rng, lambda d: d.day == msd and wd(d) != wstart)
    w = week_start_of(p, wstart)
    shifts = []
    for i in range(7):
        d = w + dt.timedelta(days=i)
        if wd(d) == legal:
            continue
        start = dn(d) * DAY + 8 * 60
        minutes = rng.choice([480, 540, 600, 420])
        shifts.append(make_shift(rng, start, start + minutes + 60, breaks=[(start + 240, start + 300)]))
    # 前月の日を含む週がそろっていることを、週の起算日の data_from で明示することが多い。
    data_from = w.isoformat() if rng.random() < 0.5 else None
    return result(classify_case(policy, shifts, rng, data_from=data_from), 'week-spans-two-periods', 'valid')


def scenario_long_short(rng):
    """1439分の勤務（最長）と、1分・2分の勤務。"""
    policy = random_policy(rng)
    legal, wstart, msd = (policy['legal_holiday_weekday'], effective(policy, 'week_start', 'sun'),
                          policy['month_start_day'])
    nxt = lambda d: d + dt.timedelta(days=1)   # noqa: E731
    d = find_date(rng, lambda d: wd(d) != legal and wd(nxt(d)) != legal and wd(nxt(d)) != wstart
                  and nxt(d).day != msd)
    start = dn(d) * DAY + rng.randint(0, 8 * 60)
    shifts = [make_shift(rng, start, start + 1439)]
    for k, length in ((3, 1), (4, 2)):
        s = (dn(d) + k) * DAY + rng.randint(0, DAY - 3)
        shifts.append(make_shift(rng, s, s + length, breaks=[]))
    return result(classify_case(policy, shifts, rng), 'shift-1439-and-1-minute', 'valid')


def scenario_breaks_edge(rng):
    """休憩が勤務の開始の1分後に始まる、終了の1分前に終わる、休憩どうしが接する。"""
    policy = random_policy(rng)
    d = rand_date(rng)
    start = dn(d) * DAY + 9 * 60
    end = start + 540
    breaks = [(start + 1, start + 30), (start + 180, start + 240)]
    if rng.random() < 0.5:
        breaks.append((start + 240, start + 250))       # 前の休憩に接する（決着（旧Q4））
    breaks.append((end - 30, end - 1))
    shifts = [make_shift(rng, start, end, breaks=breaks)]
    shifts += filler(rng, [dn(d) + k for k in range(1, 4)])
    return result(classify_case(policy, shifts, rng), 'breaks-at-edges', 'valid')


def scenario_range_edges(rng):
    """日付の範囲の端（2000-01-01、2099-12-31）。2100-01-01T00:00 に終わる勤務は不正（決着（旧Q3））。"""
    policy = random_policy(rng)
    choice = rng.random()
    if choice < 0.4:
        s = dn(dt.date(2000, 1, 1)) * DAY + rng.randint(0, 600)
        shifts = [make_shift(rng, s, s + rng.randint(1, 600))]
        return result(classify_case(policy, shifts, rng), 'date-range-start', 'valid')
    if choice < 0.8:
        s = dn(dt.date(2099, 12, 31)) * DAY + rng.randint(0, 14 * 60)
        shifts = [make_shift(rng, s, min(s + rng.randint(1, 600), dn(dt.date(2099, 12, 31)) * DAY + DAY - 1))]
        return result(classify_case(policy, shifts, rng), 'date-range-end', 'valid')
    s = dn(dt.date(2099, 12, 31)) * DAY + 18 * 60
    shifts = [make_shift(rng, s, dn(dt.date(2100, 1, 1)) * DAY, breaks=[])]
    return result(classify_case(policy, shifts, rng, data_from=None), 'ends-2100-01-01T00:00', 'InvalidInput')


def valid_base(rng):
    """壊すための有効な土台（勤務は少なくとも1つ、またぐ勤務なし）。"""
    policy = random_policy(rng)
    first = rand_date(rng)
    shifts = []
    for i in range(rng.randint(1, 5)):
        s, e = simple_day_shift(rng, dn(first) + i)
        shifts.append(make_shift(rng, s, e))
    return policy, shifts, first


def scenario_invalid(rng):
    policy, shifts, first = valid_base(rng)
    case = classify_case(policy, shifts, data_from=None)
    variant = rng.randrange(50)
    s0 = case['shifts'][0]
    base = dn(first) * DAY
    if variant == 0:
        return result(json.dumps(case)[:-rng.randint(1, 20)], 'bad-json-truncated', 'InvalidInput')
    if variant == 1:
        return result(rng.choice(['', 'null', '[]', '"kintai.case.v1"', '{', '{"schema": NaN}',
                                  json.dumps(case) + ' x']), 'bad-json-text', 'InvalidInput')
    if variant == 2:
        case['schema'] = rng.choice(['kintai.case.v2', 'kintai.case36.v1', 'KINTAI.CASE.V1', 1, None])
        return result(case, 'wrong-schema', 'InvalidInput')
    if variant == 3:
        del case[rng.choice(['schema', 'policy', 'shifts'])]
        return result(case, 'missing-top-field', 'InvalidInput')
    if variant == 4:
        case[rng.choice(['extra', 'note', 'Shifts', 'dataFrom'])] = rng.choice([1, 'x', None])
        return result(case, 'unknown-top-field', 'InvalidInput')
    if variant == 5:
        policy[rng.choice(['foo', 'legal_holiday', 'weekly_limit', 'scheduled', 'data_from'])] = 1
        return result(case, 'unknown-policy-field', 'InvalidInput')
    if variant == 6:
        s0[rng.choice(['note', 'break', 'id'])] = 'x'
        return result(case, 'unknown-shift-field', 'InvalidInput')
    if variant == 7:
        key = rng.choice(['work_system', 'scheduled_minutes_per_day', 'month_start_day'])
        if rng.random() < 0.5:
            del policy[key]
        else:
            policy[key] = None        # 必須の項目の null は型の誤り（決着（旧Q5））
        return result(case, 'missing-or-null-required-policy-field', 'InvalidInput')
    if variant == 8:
        del s0[rng.choice(['start', 'end'])]
        return result(case, 'missing-shift-field', 'InvalidInput')
    if variant == 9:
        policy['scheduled_minutes_per_day'] = rng.choice(['420', 420.0, 420.5, True, [420]])
        return result(case, 'wrong-type-scheduled', 'InvalidInput')
    if variant == 10:
        policy['month_start_day'] = rng.choice(['1', 1.0, False])
        return result(case, 'wrong-type-month-start-day', 'InvalidInput')
    if variant == 11:
        policy['weekly_limit_minutes'] = rng.choice(['2400', 2400.0, True])
        return result(case, 'wrong-type-weekly-limit', 'InvalidInput')
    if variant == 12:
        policy['special_44h_confirmed'] = rng.choice(['true', 1, 0, 'yes'])
        return result(case, 'wrong-type-44h-confirmed', 'InvalidInput')
    if variant == 13:
        policy[rng.choice(['week_start', 'legal_holiday_weekday'])] = rng.choice([0, 7, True, ['sun']])
        return result(case, 'wrong-type-weekday', 'InvalidInput')
    if variant == 14:
        policy[rng.choice(['ambiguity', 'worker_kind', 'holiday_system', 'night_window', 'work_system'])] = \
            rng.choice([1, True, {'v': 'x'}])
        return result(case, 'wrong-type-enum-field', 'InvalidInput')
    if variant == 15:
        case['shifts'] = rng.choice([{}, 'none', None, 1])
        return result(case, 'wrong-type-shifts', 'InvalidInput')
    if variant == 16:
        s0['start'] = rng.choice([202610070900, None, ['2026-10-07T09:00'], True])
        return result(case, 'wrong-type-shift-start', 'InvalidInput')
    if variant == 17:
        s0['breaks'] = rng.choice(['none', {}, 1, [['2026-10-07T12:00']],
                                   [['2026-10-07T12:00', '2026-10-07T12:30', '2026-10-07T12:40']],
                                   [[1, 2]], ['2026-10-07T12:00']])
        return result(case, 'wrong-type-breaks', 'InvalidInput')
    if variant == 18:
        policy['scheduled_minutes_per_day'] = rng.choice([481, -1, 1440, 100000])
        return result(case, 'range-scheduled', 'InvalidInput')
    if variant == 19:
        policy['month_start_day'] = rng.choice([0, 29, 31, -1])
        return result(case, 'range-month-start-day', 'InvalidInput')
    if variant == 20:
        policy['weekly_limit_minutes'] = rng.choice([2401, 0, 2399, 2880, 4400])
        return result(case, 'range-weekly-limit', 'InvalidInput')
    if variant == 21:
        policy['ambiguity'] = rng.choice(['Reject', 'default', '', 'use-default'])
        return result(case, 'enum-ambiguity', 'InvalidInput')
    if variant == 22:
        policy[rng.choice(['week_start', 'legal_holiday_weekday'])] = rng.choice(['Sun', 'sunday', 'SUN', '日', ''])
        return result(case, 'enum-weekday', 'InvalidInput')
    if variant == 23:
        policy['work_system'] = rng.choice(['normal', 'Standard', 'variable', '', 'agriculture'])
        return result(case, 'enum-work-system', 'InvalidInput')
    if variant == 24:
        key = rng.choice(['worker_kind', 'holiday_system', 'night_window'])
        values = {'worker_kind': ['adult', 'construction', 'sugar_manufacturing', 'automobile_driver', ''],
                  'holiday_system': ['daily', 'four-weeks', ''], 'night_window': ['22-5', '23-05', '']}
        policy[key] = rng.choice(values[key])
        # construction・sugar_manufacturing は第2版で表から外れた（通常制で計算できる）ので、列挙の外。
        return result(case, f'enum-{key}', 'InvalidInput')
    if variant == 25:
        s0['end'] = s0['start']
        s0.pop('breaks', None)
        return result(case, 'shift-start-equals-end', 'InvalidInput')
    if variant == 26:
        s0['start'], s0['end'] = s0['end'], s0['start']
        s0.pop('breaks', None)
        return result(case, 'shift-start-after-end', 'InvalidInput')
    if variant == 27:
        a = base + 9 * 60
        case['shifts'] = [make_shift(rng, a, a + 300, breaks=[]),
                          make_shift(rng, a + 300 - rng.randint(1, 200), a + 600, breaks=[])]
        return result(case, 'shifts-overlap', 'InvalidInput')
    if variant == 28:
        a = base + 9 * 60
        b = rng.choice([(a, a + 60), (a + 400, a + 480), (a - 30, a + 30), (a + 470, a + 490)])
        case['shifts'] = [make_shift(rng, a, a + 480, breaks=[b])]
        return result(case, 'break-touches-or-outside-shift', 'InvalidInput')
    if variant == 29:
        a = base + 9 * 60
        case['shifts'] = [make_shift(rng, a, a + 480, breaks=[rng.choice([(a + 200, a + 200), (a + 260, a + 200)])])]
        return result(case, 'break-start-not-before-end', 'InvalidInput')
    if variant == 30:
        a = base + 9 * 60
        case['shifts'] = [make_shift(rng, a, a + 480, breaks=[(a + 100, a + 200), (a + 199, a + 260)])]
        return result(case, 'breaks-overlap', 'InvalidInput')
    if variant == 31:
        a = base + rng.randint(0, 600)
        case['shifts'] = [make_shift(rng, a, a + rng.choice([1440, 1441, 2000]), breaks=[])]
        return result(case, 'shift-1440-or-longer', 'InvalidInput')
    if variant == 32:
        s0['start'] = rng.choice(['1999-12-31T09:00', '1999-12-31T23:00'])
        return result(case, 'date-before-2000', 'InvalidInput')
    if variant == 33:
        case['shifts'] = [{'start': '2100-01-01T09:00', 'end': '2100-01-01T17:00'}]
        return result(case, 'date-after-2099', 'InvalidInput')
    if variant == 34:
        d = s0['start'][:10]
        s0['end'] = f'{d}T24:00'
        s0.pop('breaks', None)
        return result(case, 'notation-24-00', 'InvalidInput')
    if variant == 35:
        s0['start'] = s0['start'][:11] + rng.choice(['25:00', '23:60', '99:99', '24:30'])
        return result(case, 'no-such-time', 'InvalidInput')
    if variant == 36:
        s0['start'] = rng.choice(['2026-02-30T09:00', '2025-02-29T09:00', '2026-13-01T09:00',
                                  '2026-00-10T09:00', '2026-04-31T09:00'])
        return result(case, 'no-such-date', 'InvalidInput')
    if variant == 37:
        s0['start'] = rng.choice([s0['start'][:11] + '9:00', s0['start'] + ':00', s0['start'].replace('-', '/'),
                                  s0['start'].replace('T', 'X'), s0['start'].replace('T', ''),
                                  s0['start'].replace('T', 't'), s0['start'] + 'Z', ' ' + s0['start'],
                                  s0['start'] + '+09:00', s0['start'][:10], s0['start'].replace('T', '  ')])
        return result(case, 'bad-datetime-format', 'InvalidInput')
    if variant == 38:
        full = str.maketrans('0123456789', '０１２３４５６７８９')
        s0['start'] = s0['start'].translate(full)
        return result(case, 'full-width-digits', 'InvalidInput')
    if variant == 39:
        a = base + 9 * 60
        case['shifts'] = [make_shift(rng, a, a + 480, breaks=[(a + 100, a + 160)])]
        case['shifts'][0]['breaks'][0][0] = case['shifts'][0]['breaks'][0][0][:11] + '25:10'
        return result(case, 'bad-break-time', 'InvalidInput')
    if variant == 40:
        a = base + 9 * 60
        case['shifts'] = [make_shift(rng, a, a + 480, breaks=[]), make_shift(rng, a, a + 480, breaks=[])]
        return result(case, 'shifts-identical', 'InvalidInput')
    if variant == 41:
        a = base + 9 * 60
        case['shifts'] = [make_shift(rng, a, a + 600, breaks=[]), make_shift(rng, a + 60, a + 120, breaks=[])]
        return result(case, 'shift-inside-shift', 'InvalidInput')
    if variant == 42:
        a = base + 9 * 60
        case['shifts'] = [make_shift(rng, a, a + 480, breaks=[(a + 10, a + 470), (a + 100, a + 200)])]
        return result(case, 'break-inside-break', 'InvalidInput')
    if variant == 43:
        case['data_from'] = rng.choice([20261001, ['2026-10-01'], True, {'d': 1}])
        return result(case, 'wrong-type-data-from', 'InvalidInput')
    if variant == 44:
        case['data_from'] = rng.choice(['2026/10/01', '2026-02-30', '2026-10-1', '2026-10-01T00:00',
                                        '1999-12-31', '2100-01-01', '', ' 2026-10-01', '２０２６-10-01'])
        return result(case, 'bad-data-from', 'InvalidInput')
    if variant == 45:
        policy['holiday_swaps'] = rng.choice(['none', {'from': '2026-10-04'}, 1, True])
        return result(case, 'wrong-type-holiday-swaps', 'InvalidInput')
    if variant == 46:
        return result(duplicate_key_text(case, rng), 'duplicate-key', 'InvalidInput')
    if variant == 47:
        text = json.dumps(case, ensure_ascii=False)
        text = text.replace('"work_system": "standard"', '"work_system": "standard", "work_system": "standard"', 1)
        return result(text, 'duplicate-policy-key', 'InvalidInput')
    if variant == 48:
        case['shifts'] = [{'start': '2099-12-31T22:00', 'end': '2100-01-01T00:00'}]
        return result(case, 'ends-2100-01-01T00:00', 'InvalidInput')
    policy['scheduled_minutes_per_day'] = rng.choice([480.0, 4.8e2, 1e3])
    return result(case, 'float-number', 'InvalidInput')


UNSUPPORTED_SETTINGS = [
    ('work_system', 'variable_monthly', 'VariableMonthly'), ('work_system', 'variable_yearly', 'VariableYearly'),
    ('work_system', 'variable_weekly_irregular', 'VariableWeeklyIrregular'),
    ('work_system', 'flex', 'Flex'), ('work_system', 'discretionary', 'DiscretionaryDeemed'),
    ('work_system', 'offsite_deemed', 'OffsiteDeemed'), ('work_system', 'managerial', 'Managerial'),
    ('work_system', 'agriculture_exempt', 'AgricultureExempt'),
    ('work_system', 'surveillance_intermittent', 'SurveillanceIntermittent'),
    ('work_system', 'night_duty_permit', 'NightDutyPermit'),
    ('work_system', 'highly_professional', 'HighlyProfessional'),
    ('worker_kind', 'minor', 'Minor'), ('worker_kind', 'pregnant_or_postpartum', 'PregnantOrPostpartum'),
    ('worker_kind', 'research_development', 'ResearchDevelopment'),
    ('worker_kind', 'driver', 'AutomobileDriver'), ('worker_kind', 'physician', 'Physician'),
    ('worker_kind', 'disaster_recovery_construction', 'DisasterRecoveryConstruction'),
    ('worker_kind', 'hazardous_work', 'HazardousWork'),
    ('holiday_system', 'four_weeks', 'FourWeekHolidays'),
    ('holiday_system', 'continuous_rotation', 'ContinuousRotation'),
    ('night_window', '23-06', 'NightWindowOverride'),
]
WORK_SYSTEM_CODES = [s for s in UNSUPPORTED_SETTINGS if s[0] == 'work_system']


def random_swaps(rng):
    """空でない holiday_swaps（要素の中身は検査されない）。"""
    return rng.choice([[{'from': '2026-10-04', 'to': '2026-10-07'}], ['x'], [1], [None], [[]], [{}],
                       [{'from': '2026-10-04'}, {'from': '2026-10-11'}]])


def apply_unsupported(rng, policy):
    """対応外の指定を1つ入れ、そのコードを返す。"""
    r = rng.random()
    if r < 0.62:
        key, value, code = rng.choice(UNSUPPORTED_SETTINGS)
        policy[key] = value
        return code
    if r < 0.74:
        policy['holiday_swaps'] = random_swaps(rng)
        return 'HolidaySwap'
    if r < 0.87:
        if rng.random() < 0.5:
            policy.pop('legal_holiday_weekday', None)
        else:
            policy['legal_holiday_weekday'] = None
        return 'LegalHolidayUnspecified'
    policy['weekly_limit_minutes'] = 2640
    choice = rng.random()
    if choice < 0.4:
        policy['special_44h_confirmed'] = False
    elif choice < 0.7:
        policy.pop('special_44h_confirmed', None)
    else:
        policy['special_44h_confirmed'] = None
    return 'Weekly44WithoutConfirmation'


def scenario_unsupported(rng):
    policy, shifts, _ = valid_base(rng)
    code = apply_unsupported(rng, policy)
    return result(classify_case(policy, shifts, rng), f'unsupported-{code}', code)


def scenario_multi_error(rng):
    """誤りを2つ入れて、検査の順序（validation.md 手順1〜5）と、対応外の表の順を見る。"""
    policy, shifts, first = valid_base(rng)
    case = classify_case(policy, shifts, data_from=None)
    s0 = case['shifts'][0]
    variant = rng.randrange(12)
    if variant == 0:      # 手順1（型）と手順3（対応外）→ 手順1
        apply_unsupported(rng, policy)
        policy['month_start_day'] = str(policy['month_start_day'])
        return result(case, 'order-type-before-unsupported', 'InvalidInput')
    if variant == 1:      # 手順2（範囲）と手順3 → 手順2
        apply_unsupported(rng, policy)
        policy['scheduled_minutes_per_day'] = 481
        return result(case, 'order-range-before-unsupported', 'InvalidInput')
    if variant == 2:      # 手順3と手順4（勤務の重なり）→ 手順3
        code = apply_unsupported(rng, policy)
        case['shifts'].append(dict(s0))
        return result(case, 'order-unsupported-before-shift-check', code)
    if variant == 3:      # 手順3と日時の書式の誤り → 手順3
        code = apply_unsupported(rng, policy)
        s0['start'] = rng.choice(['2026-02-30T09:00', s0['start'][:11] + '25:00', s0['start'][:11] + '24:00',
                                  s0['start'].replace('T', '/'), '2100-01-01T09:00'])
        return result(case, 'order-unsupported-before-datetime-format', code)
    if variant == 4:      # 手順3と data_from の書式の誤り → 手順3
        code = apply_unsupported(rng, policy)
        case['data_from'] = rng.choice(['2026-02-30', '2026/10/01', '1999-12-31'])
        return result(case, 'order-unsupported-before-data-from-format', code)
    if variant == 5:      # data_from の型の誤り（手順1）と対応外 → 手順1
        apply_unsupported(rng, policy)
        case['data_from'] = 20261001
        return result(case, 'order-data-from-type-before-unsupported', 'InvalidInput')
    if variant == 6:      # 対応外が2つ以上（work_system と、それより下の行）→ work_system
        key1, value1, code1 = rng.choice(WORK_SYSTEM_CODES)
        policy[key1] = value1
        if rng.random() < 0.5:
            policy['night_window'] = '23-06'
        policy['holiday_swaps'] = random_swaps(rng)
        policy.pop('legal_holiday_weekday', None)
        return result(case, 'order-two-unsupported', code1)
    if variant == 7:      # holiday_system と holiday_swaps → holiday_system（表の3行目が先）
        policy['holiday_system'] = rng.choice(['four_weeks', 'continuous_rotation'])
        policy['holiday_swaps'] = random_swaps(rng)
        code = 'FourWeekHolidays' if policy['holiday_system'] == 'four_weeks' else 'ContinuousRotation'
        return result(case, 'order-holiday-system-before-swaps', code)
    if variant == 8:      # holiday_swaps と night_window・法定休日なし → HolidaySwap（4行目が先）
        policy['holiday_swaps'] = random_swaps(rng)
        if rng.random() < 0.5:
            policy['night_window'] = '23-06'
        policy['legal_holiday_weekday'] = None
        policy['weekly_limit_minutes'] = 2640
        policy['special_44h_confirmed'] = False
        return result(case, 'order-swaps-before-night-window', 'HolidaySwap')
    if variant == 9:      # 対応外が2つ（表の下の2つ）
        policy['legal_holiday_weekday'] = None
        policy['weekly_limit_minutes'] = 2640
        policy['special_44h_confirmed'] = False
        return result(case, 'order-holiday-before-44h', 'LegalHolidayUnspecified')
    if variant == 10:     # 手順4と手順5（曖昧さ・IncompleteWeek）→ 手順4
        d = first + dt.timedelta(days=10)
        s = dn(d) * DAY + 22 * 60
        case['shifts'] = [make_shift(rng, s, s + 600, breaks=[]), make_shift(rng, s + 100, s + 200, breaks=[])]
        case['data_from'] = (d + dt.timedelta(days=3)).isoformat()
        return result(case, 'order-shift-check-before-ambiguity', 'InvalidInput')
    # 知らない項目と対応外 → 手順1
    apply_unsupported(rng, policy)
    policy['legal_holiday'] = 'sun'
    return result(case, 'order-unknown-field-before-unsupported', 'InvalidInput')


def scenario_json_edges(rng):
    """JSON の読み方（validation.md 手順1）: 入れ子の深さ、孤立したサロゲート、-0・1e2 などの整数の欄。"""
    policy, shifts, _ = valid_base(rng)
    case = classify_case(policy, shifts, data_from=None)
    variant = rng.randrange(12)
    if variant in (0, 1, 2):
        # holiday_swaps の中身は検査されないので、入れ子の深さだけが効く。最も外側の値(1)、
        # policy(2)、holiday_swaps(3) と数え、127段まで読める（決着（旧Q43））。
        target = {0: 127, 1: 128, 2: rng.choice([126, 129, 200])}[variant]
        policy['holiday_swaps'] = nested_arrays(target - 2)
        depth = nesting_level(case)
        return result(case, f'json-depth-{depth}', 'HolidaySwap' if depth <= 127 else 'InvalidInput')
    if variant == 3:
        policy['holiday_swaps'] = [rng.choice(['\ud800', '\udfff', 'a\udc00b', '\ud83d'])]
        return result(json.dumps(case, ensure_ascii=True), 'json-lone-surrogate-in-swaps', 'InvalidInput')
    if variant == 4:
        policy['holiday_swaps'] = [rng.choice(['😀', '日曜を水曜に', '\U0001f600 x'])]
        return result(json.dumps(case, ensure_ascii=rng.random() < 0.5), 'json-surrogate-pair-in-swaps',
                      'HolidaySwap')
    if variant == 5:      # サロゲートの誤り（手順1）は、対応外（手順3）より先
        apply_unsupported(rng, policy)
        case['shifts'][0]['start'] = case['shifts'][0]['start'] + '\ud800'
        return result(json.dumps(case, ensure_ascii=True), 'json-lone-surrogate-before-unsupported',
                      'InvalidInput')
    if variant == 6:
        key = rng.choice(['scheduled_minutes_per_day', 'month_start_day', 'weekly_limit_minutes'])
        literal = rng.choice(['-0', '1e2', '1.0', '480.0', '-0.0', '2.4e3', '1E2'])
        return result(with_literal(case, lambda v: policy.__setitem__(key, v), literal),
                      f'json-integer-field-{literal}', 'InvalidInput')
    if variant == 7:
        literal = rng.choice(['NaN', 'Infinity', '-Infinity'])
        key = rng.choice(['scheduled_minutes_per_day', 'holiday_swaps'])
        if key == 'holiday_swaps':
            return result(with_literal(case, lambda v: policy.__setitem__(key, [v]), literal),
                          f'json-{literal}-in-swaps', 'InvalidInput')
        return result(with_literal(case, lambda v: policy.__setitem__(key, v), literal),
                      f'json-{literal}', 'InvalidInput')
    if variant == 8:      # 中身を検査しない要素の -0 は読める。1e400 はどこでも読めない（決着（旧Q44））
        literal = rng.choice(['-0', '1e400', '-1e400'])
        return result(with_literal(case, lambda v: policy.__setitem__('holiday_swaps', [v]), literal),
                      f'json-{literal}-in-swaps', 'HolidaySwap' if literal == '-0' else 'InvalidInput')
    if variant == 9:      # 中身を検査しない要素の小数・指数・大きな整数は、ふつうの JSON の数
        literal = rng.choice(['1.5', '1e2', '-0.0', '123456789012345678901234567890'])
        return result(with_literal(case, lambda v: policy.__setitem__('holiday_swaps', [v]), literal),
                      f'json-number-{literal[:12]}-in-swaps', 'HolidaySwap')
    if variant == 10:     # true・false は整数の欄の型の誤り
        policy[rng.choice(['scheduled_minutes_per_day', 'month_start_day'])] = rng.choice([True, False])
        return result(case, 'json-bool-in-integer-field', 'InvalidInput')
    return result(duplicate_key_text(case, rng), 'json-duplicate-key', 'InvalidInput')


def scenario_optional_nulls(rng):
    """任意の項目の null（項目がないのと同じ）と、空白の区切り（決着（旧Q2・Q5））。"""
    policy, _, first = valid_base(rng)
    for key in rng.sample(['week_start', 'weekly_limit_minutes', 'special_44h_confirmed', 'ambiguity',
                           'worker_kind', 'holiday_system', 'night_window', 'holiday_swaps'], rng.randint(1, 4)):
        policy[key] = None
    if policy.get('weekly_limit_minutes') == 2640 and policy.get('special_44h_confirmed') is None:
        policy['weekly_limit_minutes'] = None     # 確認なしの44時間（対応外）にしない
    if (policy.get('weekly_limit_minutes') == 2640 and 'special_44h_confirmed' not in policy):
        policy['special_44h_confirmed'] = True
    shifts = []
    for i in range(rng.randint(1, 3)):
        s, e = simple_day_shift(rng, dn(first) + i)
        shift = make_shift(rng, s, e, sep=rng.choice(['T', ' ']))
        if rng.random() < 0.5:
            shift['breaks'] = None
        shifts.append(shift)
    case = classify_case(policy, shifts, rng, data_from=None)
    if rng.random() < 0.5:
        case['data_from'] = None
    return result(case, 'null-optional-fields', 'valid')


CLASSIFY_SCENARIOS = [
    (0.22, scenario_basic), (0.05, scenario_cross_plain), (0.05, scenario_night_edges),
    (0.06, scenario_day_480), (0.06, scenario_week_limit), (0.07, scenario_over60),
    (0.04, scenario_holiday_cross), (0.04, scenario_week_cross), (0.04, scenario_month_cross),
    (0.06, scenario_incomplete_week),
    (0.03, scenario_holiday_long), (0.03, scenario_week_spans_periods), (0.02, scenario_long_short),
    (0.02, scenario_breaks_edge), (0.02, scenario_range_edges), (0.02, scenario_optional_nulls),
    (0.09, scenario_invalid), (0.05, scenario_unsupported), (0.02, scenario_multi_error),
    (0.02, scenario_json_edges),
]


def gen_classify(rng):
    return pick(rng, CLASSIFY_SCENARIOS)(rng)


# ======================================================================== 36協定

def random_agreement(rng, kind):
    # year_start_month は none でも必須（決着（旧Q9））。
    agreement = {'kind': kind, 'year_start_month': pick(rng, [(5, 4), (2, 1), (3, rng.randint(1, 12))])}
    if kind == 'none':
        if rng.random() < 0.15:        # 使わない限度の項目は、範囲だけ検査して使わない（決着（旧Q16））
            agreement['monthly_limit_minutes'] = rng.randint(0, 2700)
        return agreement
    agreement['monthly_limit_minutes'] = pick(rng, [(5, 2700), (2, 2400), (2, 1800), (1, 0),
                                                    (2, rng.randint(0, 2700))])
    agreement['annual_limit_minutes'] = pick(rng, [(5, 21600), (2, 19200), (2, 18000),
                                                   (2, rng.randint(0, 21600))])
    if kind == 'special':
        agreement['special_monthly_total_limit_minutes'] = pick(rng, [(5, 5999), (2, 5940), (1, 5100),
                                                                      (1, rng.randint(0, 5999))])
        agreement['special_annual_limit_minutes'] = pick(rng, [(5, 43200), (2, 40800), (1, 36000),
                                                               (1, rng.randint(0, 43200))])
        agreement['special_months_max'] = pick(rng, [(6, 6), (1, 3), (1, 4), (1, rng.randint(0, 6))])
    elif rng.random() < 0.1:
        agreement['special_months_max'] = rng.randint(0, 6)      # general では使わない
    return agreement


def random_month_values(rng, profile):
    if profile == 'light':
        overtime = pick(rng, [(3, rng.randint(0, 1800)), (2, rng.randint(1800, 2700)), (1, 0)])
    elif profile == 'heavy':
        overtime = pick(rng, [(3, rng.randint(2400, 5400)), (1, rng.randint(4800, 6200)), (1, 2701)])
    else:
        overtime = pick(rng, [(2, rng.randint(0, 2700)), (2, rng.randint(2700, 6000)), (1, 0), (1, 2700)])
    holiday = 0 if rng.random() < 0.6 else rng.randint(0, 2400)
    return overtime, holiday


def case36(agreement, first_start, values, history):
    months = []
    start = first_start
    for overtime, holiday in values:
        months.append({'start': start.isoformat(), 'overtime': overtime, 'holiday': holiday})
        start = add_months_same_day(start, 1)
    return {'schema': 'kintai.case36.v1', 'agreement': agreement, 'months': months,
            'history': [{'start': s.isoformat(), 'total': t} for s, t in history]}


def random_history(rng, first_start, mode=None):
    mode = mode or pick(rng, [(6, 'full'), (2, 'gaps'), (2, 'none')])
    history = []
    for k in range(5, 0, -1):
        start = add_months_same_day(first_start, -k)
        if start.year < 2000:
            continue
        if mode == 'full' or (mode == 'gaps' and rng.random() < 0.5):
            history.append((start, pick(rng, [(2, rng.randint(0, 4800)), (1, rng.randint(4000, 6000)), (1, 0)])))
    if rng.random() < 0.2:
        rng.shuffle(history)      # 並びは問わない（決着（旧Q14））
    return history, mode


def base36(rng, kind=None, n_months=None, at_year_start=True, profile=None):
    kind = kind or pick(rng, [(4, 'special'), (3, 'general'), (1, 'none')])
    agreement = random_agreement(rng, kind)
    day = pick(rng, [(6, 1), (1, 16), (2, rng.randint(1, 28))])
    year = rng.randint(2001, 2096)
    month = agreement['year_start_month'] if at_year_start else rng.randint(1, 12)
    first = dt.date(year, month, day)
    n_months = n_months or pick(rng, [(1, 1), (1, 2), (1, 3), (2, 6), (4, 12), (2, 13), (2, 18), (1, 24)])
    profile = profile or rng.choice(['light', 'heavy', 'mixed'])
    values = [random_month_values(rng, profile) for _ in range(n_months)]
    history, mode = random_history(rng, first)
    return agreement, first, values, history, mode


def s36_random(rng):
    agreement, first, values, history, mode = base36(rng, at_year_start=rng.random() < 0.8)
    tag = f'random-{agreement["kind"]}-history-{mode}'
    if first.month != agreement['year_start_month']:
        tag += '-starts-mid-year'      # year_overtime_known が偽（決着（旧Q10））
    return result(case36(agreement, first, values, history), tag, 'valid')


def split_total(rng, total):
    holiday = 0 if rng.random() < 0.5 else rng.randint(0, total)
    return total - holiday, holiday


def s36_total_6000(rng):
    agreement, first, values, history, mode = base36(rng)
    i = rng.randrange(len(values))
    total = pick(rng, [(3, 6000), (3, 5999), (2, 6001), (1, 5940), (1, 5941)])
    values[i] = split_total(rng, total)
    return result(case36(agreement, first, values, history), f'month-total-{total}', 'valid')


def s36_sum_boundary(rng):
    """直近 n か月の T の合計を 4800×n ちょうど・1分前・1分超にする（履歴の月も使う）。"""
    agreement, first, values, _, _ = base36(rng)
    history, _ = random_history(rng, first, mode='full')
    n = rng.randint(2, 6)
    i = rng.randrange(len(values))
    delta = pick(rng, [(3, 0), (3, 1), (2, -1)])
    target = 4800 * n + delta
    window = list(range(i - n + 1, i + 1))
    hist = dict(history)
    totals = {}
    for k in window[:-1]:
        totals[k] = rng.randint(3600, 5600)
    rest = target - sum(totals.values())
    if not 0 <= rest <= 9000:
        for k in window[:-1]:
            totals[k] = 4800
        rest = target - 4800 * (n - 1)
    totals[window[-1]] = rest
    usable = True
    for k, total in totals.items():
        if k >= 0:
            values[k] = split_total(rng, total)
        else:
            start = add_months_same_day(first, k)
            if start.year < 2000:
                usable = False
            hist[start] = total
    history = sorted(hist.items())
    tag = f'sum{n}-{"eq" if delta == 0 else ("plus1" if delta > 0 else "minus1")}'
    return result(case36(agreement, first, values, history), tag if usable else tag + '-partly-unknown', 'valid')


def s36_monthly_edges(rng):
    """O が2700分（発動月の境）と、協定の月限度の前後。月限度が2700より小さいときの一般条項の超過。"""
    agreement, first, values, history, _ = base36(rng, kind=rng.choice(['general', 'special', 'none']))
    limit = agreement.get('monthly_limit_minutes', 2700)
    for _ in range(rng.randint(1, 4)):
        i = rng.randrange(len(values))
        overtime = rng.choice([2700, 2701, 2699, limit, limit + 1, max(0, limit - 1)])
        values[i] = (overtime, values[i][1])
    return result(case36(agreement, first, values, history), f'monthly-edges-limit{limit}', 'valid')


def s36_annual_edge(rng):
    """協定年の ΣO を、協定の年限度・法定の21600分の、ちょうどと1分超にする。"""
    kind = rng.choice(['general', 'general', 'special'])
    agreement, first, values, history, _ = base36(rng, kind=kind, n_months=rng.choice([12, 13, 18, 24]),
                                                  profile='light')
    if kind == 'general':
        limit = rng.choice([agreement['annual_limit_minutes'], 21600])
    else:
        limit = agreement['special_annual_limit_minutes']
    k = rng.randint(1, 11)
    delta = pick(rng, [(2, 0), (2, 1), (1, -1)])
    per_month = max(0, (limit + delta) // (k + 1))
    for i in range(k):
        values[i] = (per_month, values[i][1])
    values[k] = (max(0, limit + delta - per_month * k), values[k][1])
    for i in range(k + 1, min(12, len(values))):
        if rng.random() < 0.5:
            values[i] = (0, values[i][1])     # O が0の月にも年の累計の違反が付く（判定8）
    return result(case36(agreement, first, values, history), f'annual-{limit}{delta:+d}', 'valid')


def s36_special_count_edge(rng):
    """発動月（O>2700）の数を協定の回数ちょうど・1回超に。超えたあとの発動しない月も作る。"""
    agreement, first, values, history, _ = base36(rng, kind='special', n_months=rng.choice([12, 12, 13, 24]),
                                                  profile='light')
    count = agreement['special_months_max'] + rng.choice([0, 1, 1, 2])
    chosen = rng.sample(range(min(12, len(values))), min(count, 12, len(values)))
    for i in range(len(values)):
        overtime, holiday = values[i]
        if i in chosen:
            values[i] = (2700 + rng.randint(1, 300), holiday)
        elif overtime > 2700:
            values[i] = (2700, holiday)
    return result(case36(agreement, first, values, history), f'special-count-{len(chosen)}', 'valid')


def s36_history_gaps(rng):
    agreement, first, values, _, _ = base36(rng, n_months=rng.choice([1, 2, 3, 4, 6]))
    history, mode = random_history(rng, first, mode=rng.choice(['gaps', 'none']))
    return result(case36(agreement, first, values, history), f'history-{mode}', 'valid')


def s36_none(rng):
    agreement, first, values, history, _ = base36(rng, kind='none', at_year_start=rng.random() < 0.7)
    for i in range(len(values)):
        if rng.random() < 0.5:
            values[i] = (0, 0)
    return result(case36(agreement, first, values, history), 'kind-none', 'valid')


def s36_valid_edges(rng):
    """第2版で決まった有効な入力: months が空、限度が0、使わない限度の項目、古い履歴、2000年の初め。"""
    variant = rng.randrange(5)
    if variant == 0:
        agreement = random_agreement(rng, rng.choice(['general', 'special', 'none']))
        history = [] if rng.random() < 0.5 else [(dt.date(2021, rng.randint(1, 12), 1), 600)]
        return result(case36(agreement, dt.date(2021, 4, 1), [], history), 'empty-months', 'valid')
    if variant == 1:
        agreement, first, values, history, _ = base36(rng, kind=rng.choice(['general', 'special']))
        agreement[rng.choice(['monthly_limit_minutes', 'annual_limit_minutes'])] = 0
        return result(case36(agreement, first, values, history), 'limit-zero', 'valid')
    if variant == 2:
        agreement, first, values, history, _ = base36(rng, kind='general')
        agreement['special_months_max'] = rng.randint(0, 6)
        agreement['special_annual_limit_minutes'] = rng.randint(0, 43200)
        return result(case36(agreement, first, values, history), 'special-fields-in-general', 'valid')
    if variant == 3:
        agreement, _, values, _, _ = base36(rng)
        first = dt.date(2000, rng.randint(1, 5), 1)
        history = [(add_months_same_day(first, -k), 1000) for k in range(1, 6)
                   if add_months_same_day(first, -k).year >= 2000]
        return result(case36(agreement, first, values, history), 'history-before-2000', 'valid')
    agreement, first, values, history, _ = base36(rng)
    old = [(add_months_same_day(first, -k), rng.randint(0, 6000)) for k in (6, 7, 12)]
    if old[-1][0].year >= 2000:
        history = history + old
    return result(case36(agreement, first, values, history), 'history-older-than-5-months', 'valid')


def s36_invalid(rng):
    agreement, first, values, history, _ = base36(rng, n_months=rng.choice([2, 3, 6, 12]))
    case = case36(agreement, first, values, history)
    months = case['months']
    variant = rng.randrange(37)
    if variant == 34:     # JSON の整数の欄（-0、小数点、指数は型の誤り）
        i = rng.randrange(len(months))
        key = rng.choice(['overtime', 'holiday'])
        literal = rng.choice(['-0', '1e2', '60.0', '-0.0'])
        return result(with_literal(case, lambda v: months[i].__setitem__(key, v), literal),
                      f'json-integer-field-{literal}', 'InvalidInput')
    if variant == 35:
        literal = rng.choice(['4.0', '4e0', '-0'])
        return result(with_literal(case, lambda v: case['agreement'].__setitem__('year_start_month', v), literal),
                      f'json-year-start-month-{literal}', 'InvalidInput')
    if variant == 36:     # 孤立したサロゲート
        months[0]['start'] = months[0]['start'] + '\udc00'
        return result(json.dumps(case, ensure_ascii=True), 'json-lone-surrogate', 'InvalidInput')
    if variant == 0:
        return result(json.dumps(case)[:-rng.randint(1, 30)], 'bad-json-truncated', 'InvalidInput')
    if variant == 1:
        case['schema'] = rng.choice(['kintai.case36.v2', 'kintai.case.v1', None, 36])
        return result(case, 'wrong-schema', 'InvalidInput')
    if variant == 2:
        del case[rng.choice(['schema', 'agreement', 'months', 'history'])]
        return result(case, 'missing-top-field', 'InvalidInput')
    if variant == 3:
        case[rng.choice(['extra', 'policy', 'notices'])] = 1
        return result(case, 'unknown-top-field', 'InvalidInput')
    if variant == 4:
        case['agreement'][rng.choice(['limit', 'month_start_day', 'kinds'])] = 1
        return result(case, 'unknown-agreement-field', 'InvalidInput')
    if variant == 5:
        months[0][rng.choice(['total', 'note'])] = 1
        return result(case, 'unknown-month-field', 'InvalidInput')
    if variant == 6:
        case['agreement']['kind'] = rng.choice(['special_clause', 'None', '', 1, None])
        return result(case, 'enum-kind', 'InvalidInput')
    if variant == 7:
        case['agreement'] = {'kind': 'general', 'year_start_month': 4, 'monthly_limit_minutes': 2700,
                             'annual_limit_minutes': 21600}
        del case['agreement'][rng.choice(['year_start_month', 'monthly_limit_minutes', 'annual_limit_minutes'])]
        return result(case, 'general-missing-required', 'InvalidInput')
    if variant == 8:
        case['agreement'] = random_agreement(rng, 'special')
        key = rng.choice(['special_monthly_total_limit_minutes', 'special_annual_limit_minutes',
                          'special_months_max', 'monthly_limit_minutes', 'annual_limit_minutes',
                          'year_start_month'])
        if rng.random() < 0.5:
            del case['agreement'][key]
        else:
            case['agreement'][key] = None
        return result(case, 'special-missing-required', 'InvalidInput')
    if variant == 9:
        case['agreement'] = random_agreement(rng, 'special')
        key, value = rng.choice([('monthly_limit_minutes', 2701), ('annual_limit_minutes', 21601),
                                 ('special_monthly_total_limit_minutes', 6000),
                                 ('special_annual_limit_minutes', 43201), ('special_months_max', 7),
                                 ('monthly_limit_minutes', -1), ('special_months_max', 12)])
        case['agreement'][key] = value
        return result(case, f'limit-out-of-range-{key}', 'InvalidInput')
    if variant == 10:
        case['agreement'] = random_agreement(rng, 'general')
        case['agreement']['year_start_month'] = rng.choice([0, 13, -4])
        return result(case, 'range-year-start-month', 'InvalidInput')
    if variant == 11:
        case['agreement'] = random_agreement(rng, 'general')
        case['agreement'][rng.choice(['monthly_limit_minutes', 'annual_limit_minutes'])] = \
            rng.choice(['2700', 2700.0, True])
        return result(case, 'wrong-type-limit', 'InvalidInput')
    if variant == 12:
        months[rng.randrange(len(months))][rng.choice(['overtime', 'holiday'])] = rng.choice(['60', 60.5, None, True])
        return result(case, 'wrong-type-month-value', 'InvalidInput')
    if variant == 13:
        months[rng.randrange(len(months))][rng.choice(['overtime', 'holiday'])] = -rng.randint(1, 100)
        return result(case, 'negative-month-value', 'InvalidInput')
    if variant == 14:
        del months[rng.randrange(len(months))][rng.choice(['start', 'overtime', 'holiday'])]
        return result(case, 'missing-month-field', 'InvalidInput')
    if variant == 15:
        if len(months) >= 3:
            del months[rng.randrange(1, len(months) - 1)]     # 途中の月を抜く（隙間）
            return result(case, 'months-gap', 'InvalidInput')
        months.append(dict(months[-1]))                       # 同じ月をくり返す
        return result(case, 'months-repeat', 'InvalidInput')
    if variant == 16:
        months.reverse()
        return result(case, 'months-reversed', 'InvalidInput')
    if variant == 17:
        i = rng.randrange(1, len(months))
        d = dt.date.fromisoformat(months[i]['start'])
        months[i]['start'] = d.replace(day=d.day % 28 + 1).isoformat()
        return result(case, 'months-day-differs', 'InvalidInput')
    if variant == 18:      # 協定月の初日が29〜31日は不正な入力（決着（旧Q13））
        day = rng.choice([29, 30, 31])
        start = dt.date(rng.randint(2001, 2090), 1, day)
        case['months'] = [{'start': start.isoformat(), 'overtime': 600, 'holiday': 0}]
        case['history'] = []
        return result(case, 'start-day-29-31', 'InvalidInput')
    if variant == 19:
        months[0]['start'] = rng.choice(['2021-02-30', '2021-13-01', '2021/04/01', '2021-4-1', '1999-12-01',
                                         '2100-01-01', '２０２１-04-01'])
        return result(case, 'bad-month-date', 'InvalidInput')
    if variant == 20:
        case['history'] = [{'start': months[rng.randrange(len(months))]['start'], 'total': 100}]
        return result(case, 'history-not-before-months', 'InvalidInput')
    if variant == 21:      # 同じ月の2回（決着（旧Q14））
        d = add_months_same_day(dt.date.fromisoformat(months[0]['start']), -1)
        case['history'] = [{'start': d.isoformat(), 'total': 100}, {'start': d.isoformat(), 'total': 200}]
        return result(case, 'history-duplicated', 'InvalidInput')
    if variant == 22:      # 協定月の初日でない日付（決着（旧Q14））
        d = add_months_same_day(dt.date.fromisoformat(months[0]['start']), -2)
        case['history'] = [{'start': d.replace(day=d.day % 28 + 1).isoformat(), 'total': 100}]
        return result(case, 'history-day-differs', 'InvalidInput')
    if variant == 23:
        d = add_months_same_day(dt.date.fromisoformat(months[0]['start']), -1)
        case['history'] = [{'start': d.isoformat(), 'total': rng.choice([-1, '100', None, 1.5])}]
        return result(case, 'history-bad-total', 'InvalidInput')
    if variant == 24:      # history は必須の配列。null は不正（決着（旧Q14））
        case['history'] = rng.choice([None, {}, 'none'])
        return result(case, 'history-not-array', 'InvalidInput')
    if variant == 25:
        case['months'] = rng.choice([None, {}, [1]])
        return result(case, 'months-not-array', 'InvalidInput')
    if variant == 26:
        d = add_months_same_day(dt.date.fromisoformat(months[0]['start']), -1)
        case['history'] = [{'start': d.isoformat(), 'total': 100, 'overtime': 50}]
        return result(case, 'unknown-history-field', 'InvalidInput')
    if variant == 27:
        case['agreement'] = random_agreement(rng, 'special')
        case['agreement']['special_months_max'] = rng.choice([6.0, '6', True])
        return result(case, 'wrong-type-special-months-max', 'InvalidInput')
    if variant == 28:
        months[0]['start'] = rng.choice([20210401, None])
        return result(case, 'wrong-type-month-start', 'InvalidInput')
    if variant == 29:      # none でも year_start_month は必須（決着（旧Q9））
        case['agreement'] = {'kind': 'none'}
        if rng.random() < 0.5:
            case['agreement']['year_start_month'] = None
        return result(case, 'none-without-year-start-month', 'InvalidInput')
    if variant == 30:
        return result(duplicate_key_text(case, rng), 'duplicate-key', 'InvalidInput')
    if variant == 31:      # 使わない限度の項目も範囲は検査する（決着（旧Q16））
        case['agreement'] = random_agreement(rng, rng.choice(['none', 'general']))
        case['agreement']['special_months_max'] = rng.choice([7, -1])
        return result(case, 'unused-limit-out-of-range', 'InvalidInput')
    if variant == 32:      # 履歴の初日が29〜31日（1月は31日まであるので、必ず実在する日付）
        year = dt.date.fromisoformat(months[0]['start']).year - 1
        case['history'] = [{'start': dt.date(year, 1, rng.choice([29, 30, 31])).isoformat(), 'total': 100}]
        return result(case, 'history-day-29-31', 'InvalidInput')
    return result(rng.choice(['', '{}', '[]', 'null', '{"schema":"kintai.case36.v1"}']), 'bad-json-text',
                  'InvalidInput')


def s36_lower_bound_edges(rng):
    """協定年の途中から始まる下限値の判定（決着（旧Q39））と、months が空の履歴（決着（旧Q40））。"""
    if rng.random() < 0.6:
        # year_overtime_known が偽で、下限値が協定の年限度を超える（21600分以下か超えるか）。
        agreement = random_agreement(rng, 'general')
        agreement['annual_limit_minutes'] = rng.choice([12000, 15000, 18000])
        start_month = (agreement['year_start_month'] + rng.randint(1, 8) - 1) % 12 + 1
        first = dt.date(rng.randint(2001, 2096), start_month, 1)
        months_left = (agreement['year_start_month'] - start_month) % 12 or 12
        target = agreement['annual_limit_minutes'] + rng.randint(1, 21600 - agreement['annual_limit_minutes'])
        per_month = min(2700, target // max(1, months_left) + 1)
        values = [(per_month, 0) for _ in range(months_left)]
        history, _ = random_history(rng, first)
        return result(case36(agreement, first, values, history), 'lower-bound-general-annual', 'valid')
    # months が空のとき、history は日が1〜28であることだけを見る（日がそろっていなくてもよい）。
    agreement = random_agreement(rng, rng.choice(['general', 'special', 'none']))
    history = [(dt.date(2021, 1, 1), 100), (dt.date(2021, 2, rng.randint(2, 28)), 200)]
    return result(case36(agreement, dt.date(2021, 4, 1), [], history), 'empty-months-history-days-differ',
                  'valid')


def s36_minutes_cap(rng):
    """O＋H と history の total の上限44640分（決着（旧Q46））の、ちょうどと1分超。"""
    agreement, first, values, history, _ = base36(rng, n_months=rng.choice([1, 2, 3, 6]))
    i = rng.randrange(len(values))
    variant = rng.randrange(8)
    if variant == 0:
        values[i] = (44640, 0)
        tag, intended = 'cap-overtime-44640', 'valid'
    elif variant == 1:
        values[i] = (44641, 0)
        tag, intended = 'cap-overtime-44641', 'InvalidInput'
    elif variant == 2:
        values[i] = (0, 44640)
        tag, intended = 'cap-holiday-44640', 'valid'
    elif variant == 3:
        values[i] = (0, 44641)
        tag, intended = 'cap-holiday-44641', 'InvalidInput'
    elif variant == 4:
        overtime = rng.randint(1, 44639)
        values[i] = (overtime, 44640 - overtime)
        tag, intended = 'cap-sum-44640', 'valid'
    elif variant == 5:
        overtime = rng.randint(1, 44640)
        values[i] = (overtime, 44641 - overtime)
        tag, intended = 'cap-sum-44641', 'InvalidInput'
    else:
        start = add_months_same_day(first, -rng.randint(1, 5))
        total = 44640 if variant == 6 else 44641
        history = [(s, t) for s, t in history if s != start] + [(start, total)]
        tag, intended = f'cap-history-{total}', ('valid' if total == 44640 else 'InvalidInput')
    return result(case36(agreement, first, values, history), tag, intended)


CASE36_SCENARIOS = [
    (0.26, s36_random), (0.07, s36_total_6000), (0.10, s36_sum_boundary), (0.08, s36_monthly_edges),
    (0.07, s36_annual_edge), (0.06, s36_special_count_edge), (0.05, s36_history_gaps), (0.04, s36_none),
    (0.04, s36_valid_edges), (0.15, s36_invalid), (0.02, s36_lower_bound_edges), (0.03, s36_minutes_cap),
]


def gen_case36(rng):
    return pick(rng, CASE36_SCENARIOS)(rng)


# ======================================================================== 年休
# 生成のための簡単な見積もり（オラクルとは別に書く。答えは常にオラクルが決める）。

NORMAL = [10, 11, 12, 14, 16, 18, 20]
PROPORTIONAL = {4: [7, 8, 9, 10, 12, 13, 15], 3: [5, 6, 6, 8, 9, 10, 11], 2: [3, 4, 4, 5, 6, 6, 7],
                1: [1, 2, 2, 2, 3, 3, 3]}


def plan_lots(employee, as_of):
    """生成用: as_of までのロットの見積もり [(付与日, 半日の数, 失効日)]。"""
    if employee.get('grants') is not None:
        # 明示の付与は事実として扱い、attendance は使わない。
        grants = sorted(((dt.date.fromisoformat(g['date']), g['halves']) for g in employee['grants']),
                        key=lambda g: g[0])
        return [[d, h, next_start(d, 24)] for d, h in grants if d <= as_of]
    attendance = employee['attendance']
    days, minutes, annual = (employee['weekly_scheduled_days'], employee['weekly_scheduled_minutes'],
                             employee.get('annual_scheduled_days'))
    if days >= 5 or minutes >= 1800 or (annual is not None and annual >= 217):
        row = NORMAL
    elif annual is not None:
        row = PROPORTIONAL[4 if annual >= 169 else 3 if annual >= 121 else 2 if annual >= 73 else 1]
    else:
        row = PROPORTIONAL[max(1, days)]
    first = next_start(dt.date.fromisoformat(employee['hire_date']), 6)
    lots = []
    k = 0
    while True:
        date = next_start(first, 12 * k)
        if date > as_of:
            break
        status = attendance[k] if k < len(attendance) else 'unknown'
        if status != 'below':
            lots.append([date, 2 * row[min(k, 6)], next_start(date, 24)])
        k += 1
    return lots


def take_records(lots, wishes):
    """生成用: (日付, 単位) の希望を日付の順に見て、残りがあるものだけを取得の記録にする。"""
    remaining = [halves for _, halves, _ in lots]
    records = []
    for date, unit in sorted(wishes, key=lambda w: w[0]):
        need = 2 if unit == 'full' else 1
        available = sum(r for (g, _, e), r in zip(lots, remaining) if g <= date < e)
        if available < need:
            continue
        for i, (g, _, e) in enumerate(lots):
            while need and g <= date < e and remaining[i]:
                remaining[i] -= 1
                need -= 1
        records.append({'date': date.isoformat(), 'unit': unit})
    return records


def take(rng, lots, dates):
    """生成用: 日付の候補に単位を無作為に付けて、残りのあるものだけを記録にする。"""
    return take_records(lots, [(d, pick(rng, [(3, 'full'), (2, 'half')])) for d in dates])


def random_employee(rng, hire, schedule=None):
    schedule = schedule or pick(rng, [(6, 'normal'), (2, 'weekly'), (1, 'annual'), (1, 'hours')])
    if schedule == 'normal':
        days, minutes, annual = rng.choice([5, 5, 5, 6]), rng.choice([2400, 2400, 2250, 1800, 2640]), None
    elif schedule == 'weekly':
        days = rng.randint(1, 4)
        minutes, annual = rng.randint(days * 180, min(1799, days * 450)), None
    elif schedule == 'annual':
        annual = pick(rng, [(1, 48), (1, 72), (1, 73), (1, 120), (1, 121), (1, 168), (1, 169), (1, 216),
                            (1, 217), (3, rng.randint(48, 260))])
        # 年間の日数は週の日数より優先する（決着（旧Q20））。週の日数はわざと食い違わせることもある。
        days = rng.choice([0, 1, 2, 3, 4]) if rng.random() < 0.3 else (min(4, max(1, annual // 50))
                                                                         if annual <= 216 else 5)
        minutes = rng.randint(300, 1799) if annual <= 216 else 2400
    else:     # 週4日以下（週0日も）でも、週1800分以上なら通常付与（決着（旧Q36））
        days, minutes, annual = rng.randint(0, 4), rng.choice([1800, 1801, 2000]), None
    n_attendance = rng.randint(0, 9)
    attendance = [pick(rng, [(12, 'meets'), (1, 'below'), (1, 'unknown')]) for _ in range(n_attendance)]
    employee = {'hire_date': hire.isoformat(), 'weekly_scheduled_days': days,
                'weekly_scheduled_minutes': minutes, 'annual_scheduled_days': annual,
                'attendance': attendance, 'grants': None}
    r = rng.random()
    if annual is None and r < 0.15:        # 指定なしは、項目がなくても null でも同じ（決着（旧Q29））
        del employee['annual_scheduled_days']
    if r > 0.85:
        del employee['grants']
    return employee, schedule


def leave_case(as_of, employee, taken):
    return {'schema': 'kintai.caseleave.v1', 'as_of': as_of.isoformat(), 'employee': employee, 'taken': taken}


def random_taken(rng, lots, as_of, count=None):
    if not lots:
        return []
    low = lots[0][0]
    if low > as_of:
        return []
    count = count if count is not None else pick(rng, [(1, 0), (3, rng.randint(1, 8)), (2, rng.randint(8, 25))])
    span = (as_of - low).days
    dates = [low + dt.timedelta(days=rng.randint(0, span)) for _ in range(count)]
    return take(rng, lots, dates)


def future_records(rng, as_of):
    """as_of より後の取得の記録（数えず、残数の検査もしない。決着（旧Q24））。"""
    return [{'date': (as_of + dt.timedelta(days=rng.randint(1, 400))).isoformat(),
             'unit': rng.choice(['full', 'half'])} for _ in range(rng.randint(1, 30))
            if as_of + dt.timedelta(days=400) <= dt.date(2099, 12, 31)]


def sl_random(rng, schedule=None, hire=None, tag='random'):
    hire = hire or rand_date(rng, high=dt.date(2090, 12, 31))
    employee, schedule = random_employee(rng, hire, schedule)
    as_of = min(hire + dt.timedelta(days=rng.randint(0, 9 * 366)), dt.date(2099, 12, 31))
    lots = plan_lots(employee, as_of)
    taken = random_taken(rng, lots, as_of)
    if rng.random() < 0.15:
        taken = taken + future_records(rng, as_of)
    intended = 'valid'
    if not (employee['weekly_scheduled_days'] >= 5 or employee['weekly_scheduled_minutes'] >= 1800):
        annual = employee.get('annual_scheduled_days')
        if (annual is not None and annual < 48) or (annual is None and employee['weekly_scheduled_days'] == 0):
            intended = 'LeaveOutOfTable'
    return result(leave_case(as_of, employee, taken), f'{tag}-{schedule}', intended)


OUOU_MISSING_HIRE = [(8, 29), (8, 30), (8, 31), (3, 31), (5, 31), (10, 31), (12, 31), (2, 29), (6, 30)]


def sl_ouou_missing(rng):
    """付与日に同じ日付がない月（雇入れが29〜31日、2月29日）。8月29日の雇入れは g0 から数える
    （2月29日に戻る。決着（旧Q18・Q19））。"""
    month, day = rng.choice(OUOU_MISSING_HIRE)
    for _ in range(50):
        year = rng.randint(2000, 2088)
        try:
            hire = dt.date(year, month, day)
            break
        except ValueError:
            continue
    else:
        hire = dt.date(2023, 8, 29)
    return sl_random(rng, schedule='normal', hire=hire, tag=f'hire-{month:02d}-{day:02d}')


def sl_expiry_edge(rng):
    """失効日の前日・当日の取得。as_of が失効日の前日・当日・翌日。"""
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, schedule = random_employee(rng, hire, 'normal')
    employee['attendance'] = ['meets'] * 9
    first = next_start(hire, 6)
    expiry = next_start(first, 24)
    as_of = expiry + dt.timedelta(days=rng.choice([-1, 0, 1, 30]))
    lots = plan_lots(employee, as_of)
    dates = [first + dt.timedelta(days=rng.randint(0, 700)) for _ in range(rng.randint(0, 8))]
    dates.append(expiry - dt.timedelta(days=1))
    if as_of >= expiry:
        dates.append(expiry)
    taken = take(rng, lots, dates)
    return result(leave_case(as_of, employee, taken), 'expiry-edge', 'valid')


def sl_insufficient_on_expiry(rng):
    """失効日の当日に、古いロットしか残りがない（次のロットはまだ無い）→ 不正な入力。"""
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, _ = random_employee(rng, hire, 'normal')
    employee['attendance'] = ['meets', 'below', 'below', 'below']   # 2回目以後は付与しない
    first = next_start(hire, 6)
    expiry = next_start(first, 24)
    taken = [{'date': (expiry - dt.timedelta(days=1)).isoformat(), 'unit': 'full'},
             {'date': expiry.isoformat(), 'unit': rng.choice(['full', 'half'])}]
    return result(leave_case(expiry + dt.timedelta(days=rng.randint(0, 10)), employee, taken),
                  'insufficient-on-expiry-day', 'InvalidInput')


def sl_five_day_edge(rng):
    """5日（10半日）の前後の取得と、as_of が期間の終わりの前日・当日・翌日。"""
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, schedule = random_employee(rng, hire, pick(rng, [(5, 'normal'), (1, 'weekly'), (1, 'annual')]))
    employee['attendance'] = ['meets'] * 9
    k = rng.randint(0, 5)
    first = next_start(hire, 6)
    base = next_start(first, 12 * k)
    window_end = period_end(base, 12)
    as_of = window_end + dt.timedelta(days=pick(rng, [(2, -1), (2, 0), (2, 1), (1, -100), (1, 200)]))
    lots = plan_lots(employee, as_of)
    target = pick(rng, [(2, 9), (3, 10), (2, 11), (1, 8), (1, 12), (1, 0)])
    wishes = []
    halves = 0
    last_day = min(window_end, as_of)
    while halves < target:
        unit = 'half' if target - halves == 1 or rng.random() < 0.4 else 'full'
        wishes.append((base + dt.timedelta(days=rng.randint(0, (last_day - base).days)), unit))
        halves += 2 if unit == 'full' else 1
    if rng.random() < 0.4:      # 期間の境界の前日・当日・翌日にも取得を置く
        edge = rng.choice([base - dt.timedelta(days=1), window_end, window_end + dt.timedelta(days=1)])
        if edge <= as_of:
            wishes.append((edge, 'half'))
    taken = take_records(lots, wishes)
    tag = f'five-day-target{target}-{schedule}'
    intended = 'valid'
    if not (employee['weekly_scheduled_days'] >= 5 or employee['weekly_scheduled_minutes'] >= 1800):
        annual = employee.get('annual_scheduled_days')
        if (annual is not None and annual < 48) or (annual is None and employee['weekly_scheduled_days'] == 0):
            intended = 'LeaveOutOfTable'
    return result(leave_case(as_of, employee, taken), tag, intended)


def sl_proportional(rng):
    return sl_random(rng, schedule=pick(rng, [(2, 'weekly'), (2, 'annual'), (1, 'hours')]), tag='proportional')


def explicit_grants(rng, hire, first_halves=None, gap_months=None):
    """明示の付与。halves は1〜200（決着（旧Q41））。20半日未満は最後の付与にだけ使う
    （途中に置くと、次の付与までが12か月未満なら分割付与になるため）。"""
    grants = []
    date = hire
    count = rng.randint(1, 4)
    for i in range(count):
        halves = pick(rng, [(3, 20 + 2 * min(i, 3)), (1, rng.randint(20, 40)), (1, 200)])
        if i == count - 1 and rng.random() < 0.2:
            halves = rng.choice([1, rng.randint(1, 19)])
        if i == 0 and first_halves is not None:
            halves = first_halves
        grants.append({'date': date.isoformat(), 'halves': halves})
        date = next_start(date, gap_months if gap_months is not None else rng.choice([6, 12, 12]))
    return grants


def sl_explicit(rng):
    """前倒しの付与を明示する。as_of より後の明示の付与は出さない（決着（旧Q31））。"""
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, _ = random_employee(rng, hire, 'normal')
    employee['grants'] = explicit_grants(rng, hire)
    if rng.random() < 0.3:
        employee['attendance'] = ['below', 'unknown']     # 明示したときは使わない
    last = dt.date.fromisoformat(employee['grants'][-1]['date'])
    if rng.random() < 0.6:
        as_of = last + dt.timedelta(days=rng.randint(0, 900))
    else:
        as_of = hire + dt.timedelta(days=rng.randint(0, max(0, (last - hire).days - 1)))
    lots = plan_lots(employee, as_of)
    taken = random_taken(rng, lots, as_of)
    tag = 'explicit-grants' + ('-some-after-as-of' if as_of < last else '')
    return result(leave_case(as_of, employee, taken), tag, 'valid')


def sl_split_grant(rng):
    """分割付与（LeaveSplitGrant）: 20半日未満の付与の次の付与が 次の初日(付与日, 12) より前。"""
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, _ = random_employee(rng, hire, 'normal')
    small = rng.randint(1, 19)
    first = hire + dt.timedelta(days=rng.randint(0, 30))
    boundary = next_start(first, 12)
    variant = pick(rng, [(3, 'split'), (1, 'day-before-boundary'), (2, 'at-boundary'), (1, 'big-first'),
                         (2, 'second-after-as-of'), (1, 'second-on-as-of')])
    if variant in ('split', 'second-after-as-of', 'second-on-as-of'):
        second = first + dt.timedelta(days=rng.randint(1, (boundary - first).days - 1))
    elif variant == 'day-before-boundary':
        second = boundary - dt.timedelta(days=1)
    elif variant == 'at-boundary':
        second = boundary
    else:
        second = first + dt.timedelta(days=rng.randint(1, 200))
    grants = [{'date': first.isoformat(), 'halves': 20 if variant == 'big-first' else small},
              {'date': second.isoformat(), 'halves': rng.choice([10, 20, 22])}]
    if rng.random() < 0.3:
        grants.reverse()        # 並びは付与日の順に読む
    employee['grants'] = grants
    if variant == 'second-after-as-of':
        # as_of より後の付与は、対応外の判定に入れない（決着（旧Q37））。2つ目が除かれて分割付与にならない。
        as_of = first + dt.timedelta(days=rng.randint(0, (second - first).days - 1))
        return result(leave_case(as_of, employee, random_taken(rng, plan_lots(employee, as_of), as_of)),
                      'split-grant-second-after-as-of', 'valid')
    if variant == 'second-on-as-of':      # as_of の当日の付与は判定に入る
        return result(leave_case(second, employee, []), 'split-grant-second-on-as-of', 'LeaveSplitGrant')
    intended = 'LeaveSplitGrant' if variant in ('split', 'day-before-boundary') else 'valid'
    if intended == 'LeaveSplitGrant':
        as_of = second + dt.timedelta(days=rng.randint(0, 500))
        taken = []
    else:
        as_of = max(first, second + dt.timedelta(days=rng.randint(-60, 500)))
        taken = random_taken(rng, plan_lots(employee, as_of), as_of)
    return result(leave_case(as_of, employee, taken), f'split-grant-{variant}', intended)


def sl_attendance(rng):
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, schedule = random_employee(rng, hire, pick(rng, [(3, 'normal'), (1, 'weekly')]))
    employee['attendance'] = [pick(rng, [(2, 'meets'), (2, 'below'), (2, 'unknown')])
                              for _ in range(rng.randint(0, 8))]
    as_of = hire + dt.timedelta(days=rng.randint(180, 8 * 366))
    lots = plan_lots(employee, as_of)
    taken = random_taken(rng, lots, as_of)
    return result(leave_case(as_of, employee, taken), f'attendance-mix-{schedule}', 'valid')


def sl_invalid(rng):
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, _ = random_employee(rng, hire, 'normal')
    employee['annual_scheduled_days'] = None
    employee['grants'] = None
    as_of = hire + dt.timedelta(days=rng.randint(200, 2000))
    lots = plan_lots(employee, as_of)
    taken = random_taken(rng, lots, as_of, count=rng.randint(1, 5)) or []
    case = leave_case(as_of, employee, taken)
    variant = rng.randrange(29)
    if variant == 26:     # JSON の整数の欄（-0、小数点、指数は型の誤り）
        key = rng.choice(['weekly_scheduled_days', 'weekly_scheduled_minutes'])
        literal = rng.choice(['-0', '5.0', '5e0', '2.4e3'])
        return result(with_literal(case, lambda v: case['employee'].__setitem__(key, v), literal),
                      f'json-integer-field-{literal}', 'InvalidInput')
    if variant == 27:
        case['employee']['grants'] = [{'date': hire.isoformat(), 'halves': SENTINEL}]
        literal = rng.choice(['2e1', '20.0', '-0'])
        return result(json.dumps(case, ensure_ascii=True).replace(str(SENTINEL), literal, 1),
                      f'json-grant-halves-{literal}', 'InvalidInput')
    if variant == 28:     # 孤立したサロゲート
        case['employee']['hire_date'] = case['employee']['hire_date'] + '\ud800'
        return result(json.dumps(case, ensure_ascii=True), 'json-lone-surrogate', 'InvalidInput')
    if variant == 0:
        return result(json.dumps(case)[:-rng.randint(1, 30)], 'bad-json-truncated', 'InvalidInput')
    if variant == 1:
        case['schema'] = rng.choice(['kintai.caseleave.v2', 'kintai.case.v1', None])
        return result(case, 'wrong-schema', 'InvalidInput')
    if variant == 2:      # トップレベルの必須は schema・as_of・employee・taken（決着（旧Q42））
        key = rng.choice(['schema', 'as_of', 'employee', 'taken'])
        del case[key]
        return result(case, f'missing-top-{key}', 'InvalidInput')
    if variant == 3:
        case['employee'][rng.choice(['name', 'hire', 'weekly_days'])] = 'x'
        return result(case, 'unknown-employee-field', 'InvalidInput')
    if variant == 4:      # 必須の項目の欠け（attendance も必須。決着（旧Q29））
        key = rng.choice(['hire_date', 'weekly_scheduled_days', 'weekly_scheduled_minutes', 'attendance'])
        if rng.random() < 0.5:
            del case['employee'][key]
        else:
            case['employee'][key] = None
        return result(case, f'missing-or-null-employee-{key}', 'InvalidInput')
    if variant == 5:
        case['employee']['weekly_scheduled_days'] = rng.choice([8, -1, '5', 5.0])
        return result(case, 'bad-weekly-days', 'InvalidInput')
    if variant == 6:
        case['employee']['weekly_scheduled_minutes'] = rng.choice([10081, -1, '2400', 2400.5])
        return result(case, 'bad-weekly-minutes', 'InvalidInput')
    if variant == 7:
        case['employee']['annual_scheduled_days'] = rng.choice([367, -1, '200', 200.0])
        return result(case, 'bad-annual-days', 'InvalidInput')
    if variant == 8:
        case['employee']['attendance'] = rng.choice([['ok'], ['Meets'], [True], 'meets', [None], {}])
        return result(case, 'bad-attendance', 'InvalidInput')
    if variant == 9:      # halves は1〜200（決着（旧Q41））
        case['employee']['grants'] = rng.choice([[{'date': hire.isoformat(), 'halves': -2}],
                                                 [{'date': hire.isoformat(), 'halves': 0}],
                                                 [{'date': hire.isoformat(), 'halves': 201}],
                                                 [{'date': hire.isoformat()}],
                                                 [{'date': hire.isoformat(), 'halves': 20, 'days': 10}],
                                                 [{'date': hire.isoformat(), 'halves': '20'}],
                                                 {'date': hire.isoformat(), 'halves': 20}, 'none'])
        return result(case, 'bad-grants', 'InvalidInput')
    if variant == 10:
        case['taken'] = [{'date': as_of.isoformat(), 'unit': rng.choice(['day', 'Full', '', 2, None, 'quarter'])}]
        return result(case, 'bad-unit', 'InvalidInput')
    if variant == 11:
        case['taken'] = [{'date': as_of.isoformat()}]
        return result(case, 'taken-missing-unit', 'InvalidInput')
    if variant == 12:     # taken の null も型の誤り（決着（旧Q42））
        case['taken'] = rng.choice([{}, 'none', 1, None])
        return result(case, 'taken-not-array-or-null', 'InvalidInput')
    if variant == 13:
        case['as_of'] = rng.choice(['2025-02-29', '2026-13-01', '2026/01/01', '1999-12-31', '2100-01-01', None, 20260101])
        return result(case, 'bad-as-of', 'InvalidInput')
    if variant == 14:
        case['employee']['hire_date'] = rng.choice(['2023-02-29', '1999-04-01', '2023-4-1', '2023-04-01T09:00'])
        return result(case, 'bad-hire-date', 'InvalidInput')
    if variant == 15:
        case['taken'] = [{'date': rng.choice(['2026-02-30', '2026-06-31', '2100-01-01', '1999-12-31']), 'unit': 'full'}]
        return result(case, 'bad-taken-date', 'InvalidInput')
    if variant == 16:      # 最初の付与より前の取得（残数なし）
        case['taken'] = [{'date': (hire + dt.timedelta(days=rng.randint(0, 150))).isoformat(), 'unit': 'full'}]
        return result(case, 'taken-before-first-grant', 'InvalidInput')
    if variant == 17:      # 付与より多く取る
        if not lots:
            case['taken'] = [{'date': as_of.isoformat(), 'unit': 'half'}]
            return result(case, 'taken-without-lots', 'InvalidInput')
        g, halves, e = lots[0]
        last_day = min(e - dt.timedelta(days=1), as_of)
        case['taken'] = [{'date': (g + dt.timedelta(days=rng.randint(0, max(0, (last_day - g).days)))).isoformat(),
                          'unit': 'full'} for _ in range(halves // 2 + 1 + sum(h for _, h, _ in lots))]
        return result(case, 'taken-more-than-granted', 'InvalidInput')
    if variant == 18:
        case['extra'] = 1
        return result(case, 'unknown-top-field', 'InvalidInput')
    if variant == 19:
        case['taken'] = [{'date': as_of.isoformat(), 'unit': 'full', 'reason': 'x'}]
        return result(case, 'unknown-taken-field', 'InvalidInput')
    if variant == 20:
        case['employee'] = rng.choice([None, [], 'x'])
        return result(case, 'employee-not-object', 'InvalidInput')
    if variant == 21:
        case['employee']['annual_scheduled_days'] = rng.choice([True, [100]])
        return result(case, 'wrong-type-annual', 'InvalidInput')
    if variant == 22:
        case['employee']['grants'] = [{'date': '2023-02-29', 'halves': 20}]
        return result(case, 'bad-grant-date', 'InvalidInput')
    if variant == 23:
        return result(duplicate_key_text(case, rng), 'duplicate-key', 'InvalidInput')
    if variant == 24:      # as_of より後の記録でも、日付の書式の誤りは不正な入力
        case['taken'] = taken + [{'date': '2099-02-30', 'unit': 'half'}]
        return result(case, 'bad-date-in-future-record', 'InvalidInput')
    return result(rng.choice(['', '[]', 'null', '{"schema":"kintai.caseleave.v1"']), 'bad-json-text', 'InvalidInput')


def sl_unsupported(rng):
    """対応外と、その順（LeaveHourly → LeaveSplitGrant → LeaveOutOfTable）。"""
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, _ = random_employee(rng, hire, 'normal')
    as_of = hire + dt.timedelta(days=rng.randint(200, 2000))
    lots = plan_lots(employee, as_of)
    taken = random_taken(rng, lots, as_of, count=rng.randint(0, 4))
    variant = pick(rng, [(4, 'hourly'), (2, 'hourly-after-as-of'), (4, 'out-of-table-annual'),
                         (2, 'out-of-table-weekly-zero'), (1, 'hourly-and-split'), (1, 'split-and-out-of-table'),
                         (1, 'hourly-and-out-of-table'), (1, 'unsupported-before-date-format'),
                         (1, 'hourly-bad-date'), (1, 'bad-as-of-hourly-future'), (1, 'bad-as-of-split')])
    small_split = [{'date': hire.isoformat(), 'halves': 10},
                   {'date': (hire + dt.timedelta(days=90)).isoformat(), 'halves': 10}]
    if variant == 'hourly':
        # as_of の当日の記録は「as_of より後」ではないので、判定に入る。
        day = as_of - dt.timedelta(days=rng.choice([0, 0, rng.randint(1, 100)]))
        taken.insert(rng.randint(0, len(taken)), {'date': day.isoformat(), 'unit': 'hours'})
        return result(leave_case(as_of, employee, taken), 'unsupported-LeaveHourly', 'LeaveHourly')
    if variant == 'hourly-after-as-of':
        # as_of より後の取得は、対応外の判定に入れず、数えもしない（決着（旧Q37））。
        taken.append({'date': (as_of + dt.timedelta(days=rng.randint(1, 60))).isoformat(), 'unit': 'hours'})
        return result(leave_case(as_of, employee, taken), 'asof-hourly-after-as-of-ignored', 'valid')
    if variant == 'hourly-bad-date':
        # 書式の誤った日付は判定に入れたまま → LeaveHourly（手順4より先）。
        taken.append({'date': rng.choice(['2026-02-30', '2026/01/01', '2099-13-01']), 'unit': 'hours'})
        return result(leave_case(as_of, employee, taken), 'asof-hourly-bad-date', 'LeaveHourly')
    if variant == 'bad-as-of-hourly-future':
        # as_of が書式の誤りなら、何も除かない → 先の日付の hours でも LeaveHourly。
        case = leave_case(as_of, employee, taken + [{'date': '2099-06-01', 'unit': 'hours'}])
        case['as_of'] = rng.choice(['2026-02-30', '2026/01/01', as_of.isoformat() + 'T00:00'])
        return result(case, 'asof-bad-as-of-hourly-future', 'LeaveHourly')
    if variant == 'bad-as-of-split':
        employee['grants'] = small_split
        case = leave_case(as_of, employee, [])
        case['as_of'] = rng.choice(['2026-02-30', '2026/01/01'])
        return result(case, 'asof-bad-as-of-split', 'LeaveSplitGrant')
    if variant in ('out-of-table-annual', 'out-of-table-weekly-zero'):
        employee['weekly_scheduled_days'] = rng.randint(0, 4)
        employee['weekly_scheduled_minutes'] = rng.randint(0, 1799)
        if variant == 'out-of-table-annual':
            employee['annual_scheduled_days'] = rng.choice([0, 1, 30, 47])
        else:
            employee['weekly_scheduled_days'] = 0
            employee['annual_scheduled_days'] = None
        return result(leave_case(as_of, employee, taken), f'unsupported-LeaveOutOfTable-{variant}', 'LeaveOutOfTable')
    if variant == 'hourly-and-split':
        employee['grants'] = small_split
        return result(leave_case(as_of, employee, [{'date': as_of.isoformat(), 'unit': 'hours'}]),
                      'order-hourly-before-split', 'LeaveHourly')
    if variant == 'split-and-out-of-table':
        employee['grants'] = small_split
        employee.update({'weekly_scheduled_days': 1, 'weekly_scheduled_minutes': 300, 'annual_scheduled_days': 40})
        return result(leave_case(as_of, employee, []), 'order-split-before-out-of-table', 'LeaveSplitGrant')
    if variant == 'hourly-and-out-of-table':
        employee.update({'weekly_scheduled_days': 1, 'weekly_scheduled_minutes': 300,
                         'annual_scheduled_days': rng.choice([0, 47])})
        return result(leave_case(as_of, employee, [{'date': as_of.isoformat(), 'unit': 'hours'}]),
                      'order-hourly-before-out-of-table', 'LeaveHourly')
    taken = [{'date': '2026-02-30', 'unit': 'full'}, {'date': as_of.isoformat(), 'unit': 'hours'}]
    return result(leave_case(as_of, employee, taken), 'order-unsupported-before-date-format', 'LeaveHourly')


def sl_valid_edges(rng):
    """第2版で決まった有効な入力: as_of より後の記録、出勤率の欠け、2099年を超える失効日。"""
    variant = rng.randrange(3)
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    if variant == 0:
        employee, _ = random_employee(rng, hire, 'normal')
        as_of = hire + dt.timedelta(days=rng.randint(200, 1500))
        taken = random_taken(rng, plan_lots(employee, as_of), as_of) + future_records(rng, as_of)
        taken += [{'date': (as_of + dt.timedelta(days=1)).isoformat(), 'unit': 'full'}] * rng.randint(1, 40)
        return result(leave_case(as_of, employee, taken), 'taken-after-as-of-not-counted', 'valid')
    if variant == 1:
        employee, _ = random_employee(rng, hire, 'normal')
        employee['attendance'] = []
        as_of = hire + dt.timedelta(days=rng.randint(200, 3000))
        return result(leave_case(as_of, employee, random_taken(rng, plan_lots(employee, as_of), as_of)),
                      'attendance-empty-all-unknown', 'valid')
    hire = dt.date(rng.randint(2096, 2099), rng.randint(1, 6), rng.randint(1, 28))
    employee, _ = random_employee(rng, hire, 'normal')
    as_of = dt.date(2099, 12, 31)
    return result(leave_case(as_of, employee, random_taken(rng, plan_lots(employee, as_of), as_of)),
                  'dates-computed-after-2099', 'valid')


def sl_decided_edges(rng):
    """第3回の回答で決まった境界: ①と LeaveOutOfTable の順（旧Q36）、同じ付与日の付与（旧Q38）。"""
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, _ = random_employee(rng, hire, 'normal')
    if rng.random() < 0.5:
        # ①（週5日以上か週1800分以上）に当たれば、年間48日未満でも、年間なしで週0日でも通常付与。
        if rng.random() < 0.5:
            employee['annual_scheduled_days'] = rng.choice([0, 30, 47])
        else:
            employee.update({'weekly_scheduled_days': 0, 'weekly_scheduled_minutes': rng.choice([1800, 2400]),
                             'annual_scheduled_days': None})
        as_of = hire + dt.timedelta(days=rng.randint(200, 2000))
        return result(leave_case(as_of, employee, []), 'normal-by-week-wins-over-out-of-table', 'valid')
    # 同じ付与日の2つの付与は、入力の順で並ぶ。20半日未満が先なら、次（同じ日）が12か月未満で分割付与。
    grants = [{'date': hire.isoformat(), 'halves': 20}, {'date': hire.isoformat(), 'halves': 10}]
    if rng.random() < 0.5:
        grants.reverse()
    employee['grants'] = grants
    intended = 'LeaveSplitGrant' if grants[0]['halves'] < 20 else 'valid'
    return result(leave_case(hire + dt.timedelta(days=200), employee, []), 'same-date-grants', intended)


def sl_split_with_bad_date(rng):
    """分割付与に当たる付与と、書式の誤った付与日が両方ある → 分割付与の判定はせず、手順4で不正
    （決着（旧Q45））。"""
    hire = rand_date(rng, high=dt.date(2088, 12, 31))
    employee, _ = random_employee(rng, hire, 'normal')
    employee['grants'] = [{'date': hire.isoformat(), 'halves': 10},
                          {'date': (hire + dt.timedelta(days=90)).isoformat(), 'halves': 10},
                          {'date': rng.choice(['2026-02-30', '2026/04/01']), 'halves': 20}]
    return result(leave_case(hire + dt.timedelta(days=200), employee, []), 'split-with-bad-date', 'InvalidInput')


LEAVE_SCENARIOS = [
    (0.31, sl_random), (0.09, sl_ouou_missing), (0.08, sl_expiry_edge), (0.02, sl_insufficient_on_expiry),
    (0.11, sl_five_day_edge), (0.07, sl_proportional), (0.05, sl_explicit), (0.04, sl_split_grant),
    (0.04, sl_attendance), (0.04, sl_valid_edges), (0.07, sl_invalid), (0.05, sl_unsupported),
    (0.02, sl_decided_edges), (0.01, sl_split_with_bad_date),
]


def gen_leave(rng):
    return pick(rng, LEAVE_SCENARIOS)(rng)


# ======================================================================== CLI

def main(argv=None):
    parser = argparse.ArgumentParser(description='無作為のケースを作る')
    parser.add_argument('--seed', type=int, required=True)
    parser.add_argument('--cases', type=int, default=10, help='種類ごとの件数')
    parser.add_argument('--kind', choices=KINDS, action='append', help='作る種類（省略で3種類）')
    parser.add_argument('--out', required=True, help='書き出すディレクトリ（無ければ作る）')
    args = parser.parse_args(argv)
    os.makedirs(args.out, exist_ok=True)
    manifest = []
    for kind in args.kind or KINDS:
        for index in range(args.cases):
            item = generate(kind, args.seed, index)
            name = f'{kind}-{index:05d}.json'
            with open(os.path.join(args.out, name), 'wb') as stream:
                stream.write(item['data'])
            manifest.append({'file': name, 'kind': kind, 'index': index, 'tag': item['tag'],
                             'intended': item['intended']})
    with open(os.path.join(args.out, 'manifest.json'), 'w', encoding='utf-8') as stream:
        json.dump({'seed': args.seed, 'cases': manifest}, stream, ensure_ascii=False, indent=1)
    print(f'{len(manifest)} 件を {args.out} に書いた')
    return 0


if __name__ == '__main__':
    sys.exit(main())
