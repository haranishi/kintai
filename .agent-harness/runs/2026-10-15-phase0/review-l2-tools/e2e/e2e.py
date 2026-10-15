"""L2 査読: 打刻CSV → report の道筋を、仕様の文から書いたモデルと突き合わせる。

対・秒の丸め・同じ1分の重なりの切り分け・休憩の不足・台帳の集計は、このファイルで仕様
（docs/spec/punch.md・report.md・cli.md）から書いた。区分そのもの（classify.md）は、差分試験で
本実装と一致を確かめた別実装 tools/oracle/classify_oracle.py を借りる。
使い方: python3 -I e2e.py WS BIN SEED N
"""
import datetime as dt
import json
import os
import random
import subprocess
import sys

WS = os.environ.get('L2_WS') or sys.argv[1]
if __name__ == '__main__':
    BIN, SEED, N = sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
sys.path.insert(0, os.path.join(WS, 'tools', 'oracle'))
sys.dont_write_bytecode = True
from classify_oracle import classify  # noqa: E402

WD = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun']
WORK = os.path.join(WS, '.tmp', 'e2e', 'work')
STATS = {}
ANOMALY_RATE = float(os.environ.get('ANOMALY_RATE', '0.004'))
os.makedirs(WORK, exist_ok=True)
ORDER = {'break_end': 0, 'out': 1, 'in': 2, 'break_start': 3}


def floor_min(t):
    return t.replace(second=0)


def ceil_min(t):
    return t if t.second == 0 else t.replace(second=0) + dt.timedelta(minutes=1)


def fmt_min(t):
    return t.strftime('%Y-%m-%dT%H:%M')


def pair(punches):
    """punches: [(time, kind, line)] 1人分。仕様の状態遷移。異常があれば ('refused', anomalies)。"""
    items = sorted(punches, key=lambda p: (p[0], ORDER[p[1]], p[2]))
    anomalies = []
    # 同じ秒の打刻
    i = 0
    while i < len(items):
        j = i
        while j + 1 < len(items) and items[j + 1][0] == items[i][0]:
            j += 1
        if j > i:
            anomalies.append(('SameTimestamp', items[i][0].date(), min(x[2] for x in items[i:j + 1])))
        i = j + 1
    state = 'off'
    cur = None
    closed = []
    for t, kind, line in items:
        if state == 'off':
            if kind == 'in':
                state, cur = 'on', {'in': (t, line), 'breaks': [], 'bs': None}
            elif kind == 'out':
                anomalies.append(('OutWithoutIn', t.date(), line))
            elif kind == 'break_start':
                anomalies.append(('BreakStartWithoutIn', t.date(), line))
            else:
                anomalies.append(('BreakEndWithoutStart', t.date(), line))
        elif state == 'on':
            if kind == 'in':
                anomalies.append(('DuplicateIn', t.date(), line))
                cur = {'in': (t, line), 'breaks': [], 'bs': None}
            elif kind == 'break_start':
                state, cur['bs'] = 'break', t
            elif kind == 'break_end':
                anomalies.append(('BreakEndWithoutStart', t.date(), line))
            else:  # out
                closed.append((cur['in'], t, cur['breaks']))
                state, cur = 'off', None
        else:  # break
            if kind == 'in':
                anomalies.append(('DuplicateIn', t.date(), line))
                state, cur = 'on', {'in': (t, line), 'breaks': [], 'bs': None}
            elif kind == 'break_start':
                anomalies.append(('BreakStartWithoutIn', t.date(), line))
            elif kind == 'break_end':
                cur['breaks'].append((cur['bs'], t))
                state, cur['bs'] = 'on', None
            else:
                anomalies.append(('OutDuringBreak', t.date(), line))
                state, cur = 'off', None
    if state != 'off':
        anomalies.append(('OpenShift', cur['in'][0].date(), cur['in'][1]))
    for (tin, line), tout, _ in closed:
        if (ceil_min(tout) - floor_min(tin)) >= dt.timedelta(minutes=1440):
            anomalies.append(('LongShift', tin.date(), line))
    if anomalies:
        return 'refused', anomalies
    # 丸め（開始側は切り捨て、終了側は切り上げ。休憩は開始が終了側の扱いで切り上げ、終了は切り捨て）
    shifts = []
    for (tin, _), tout, breaks in closed:
        s, e = floor_min(tin), ceil_min(tout)
        bs = [(ceil_min(a), floor_min(b)) for a, b in breaks]
        bs = [(a, b) for a, b in bs if a < b]
        nxt = {'start': s, 'end': e, 'breaks': bs}
        # 前の勤務の終わりが次の始業を越えるときは、前の終わりを次の始業に合わせて切る
        while shifts and shifts[-1]['end'] > nxt['start']:
            STATS['clip'] = STATS.get('clip', 0) + 1
            prev = shifts[-1]
            end = min(prev['end'], nxt['start'])
            while prev['breaks'] and prev['breaks'][-1][1] >= end:
                end = min(end, prev['breaks'][-1][0])
                prev['breaks'].pop()
            prev['end'] = end
            if prev['start'] < prev['end']:
                break
            shifts.pop()
        shifts.append(nxt)
    return 'paired', shifts


def work_minutes(sh):
    total = (sh['end'] - sh['start']).total_seconds() // 60
    for a, b in sh['breaks']:
        total -= (b - a).total_seconds() // 60
    return int(total)


def break_shortages(shifts):
    out = []
    by_day = {}
    for sh in shifts:
        by_day.setdefault(sh['start'].date(), []).append(sh)
    for day in sorted(by_day):
        group = by_day[day]
        L = sum(work_minutes(s) for s in group)
        rest = sum(int((b - a).total_seconds() // 60) for s in group for a, b in s['breaks'])
        for a, b in zip(group, group[1:]):
            rest += int((b['start'] - a['end']).total_seconds() // 60)
        need = 60 if L > 480 else 45 if L > 360 else 0
        if rest < need:
            out.append({'date': str(day), 'work_minutes': L, 'break_minutes': rest,
                        'required_minutes': need, 'shortage_minutes': need - rest})
    return out


def gen(rng):
    week_start = rng.choice(WD)
    holiday = rng.choice(WD)
    msd = rng.choice([1, 1, 16, rng.randint(1, 28)])
    weekly = 2400 if rng.random() < 0.8 else 2640
    policy = {
        'work_system': 'standard', 'scheduled_minutes_per_day': rng.choice([420, 480, 450, 0, 240]),
        'week_start': week_start, 'weekly_limit_minutes': weekly,
        'special_44h_confirmed': weekly == 2640, 'legal_holiday_weekday': holiday,
        'month_start_day': msd, 'ambiguity': rng.choice(['use_default', 'use_default', 'reject']),
    }
    # 期間: 2026-08-02 から6週ほど。data_from は最初の週の起算日。
    base = dt.date(2026, 7, 20) + dt.timedelta(days=rng.randint(0, 60))
    first = base - dt.timedelta(days=(base.weekday() - WD.index(week_start)) % 7)
    days = rng.randint(20, 50)
    emps = {}
    for e in range(3):
        eid = f'E{e:03d}'
        punches = []
        t = dt.datetime.combine(first, dt.time(0, 0)) + dt.timedelta(minutes=rng.randint(0, 600))
        same_minute = False
        end_limit = dt.datetime.combine(first + dt.timedelta(days=days), dt.time(0, 0))
        while t < end_limit:
            if rng.random() < 0.25:
                t += dt.timedelta(hours=rng.randint(8, 40), seconds=rng.randint(0, 59))
                same_minute = False
                continue
            sec = lambda: rng.choice([0, 0, rng.randint(1, 59)])
            tin = t if same_minute else t.replace(second=sec())
            dur = dt.timedelta(minutes=rng.choice([rng.randint(30, 900), rng.randint(300, 600), rng.randint(1300, 1439)]))
            tout = tin + dur
            tout = tout.replace(second=sec()) if tout.replace(second=sec()) > tin else tout
            seq = [(tin, 'in')]
            nb = rng.choice([0, 0, 1, 1, 2])
            cursor = tin
            for _ in range(nb):
                a = cursor + dt.timedelta(minutes=rng.randint(1, 240), seconds=rng.randint(0, 59))
                b = a + dt.timedelta(minutes=rng.randint(0, 90), seconds=rng.randint(1, 59))
                if b >= tout:
                    break
                seq += [(a, 'break_start'), (b, 'break_end')]
                cursor = b
            seq.append((tout, 'out'))
            if rng.random() < ANOMALY_RATE:  # 異常を混ぜる
                k = rng.randrange(len(seq))
                if rng.random() < 0.5:
                    del seq[k]
                else:
                    seq.insert(k, (seq[k][0] + dt.timedelta(seconds=rng.choice([0, 1])), rng.choice(['in', 'out', 'break_start', 'break_end'])))
            punches += seq
            if rng.random() < 0.15 and tout.second < 58:  # 同じ1分の中で次の勤務を始める
                t = tout + dt.timedelta(seconds=rng.randint(1, 59 - tout.second))
                same_minute = True
            else:
                t = tout + dt.timedelta(minutes=rng.randint(1, 900), seconds=rng.randint(0, 59))
                same_minute = False
        emps[eid] = punches
    return policy, first, days, emps


def policy_toml(policy):
    lines = ['schema = "kintai.policy.v1"', '', '[workplace]']
    for k, v in policy.items():
        lines.append(f'{k} = ' + (('true' if v else 'false') if isinstance(v, bool) else str(v) if isinstance(v, int) else f'"{v}"'))
    lines += ['rounding = "none"', '', '[agreement36]', 'kind = "none"', 'year_start_month = 4', '']
    for e in range(3):
        lines += ['[[employee]]', f'id = "E{e:03d}"', 'hire_date = "2024-04-01"', 'weekly_scheduled_days = 5',
                  'weekly_scheduled_minutes = 2400', 'attendance = []', '']
    return '\n'.join(lines)


def main():
    mismatches = 0
    checked = 0
    for case_no in range(N):
        rng = random.Random(SEED * 100003 + case_no)
        policy, first, days, emps = gen(rng)
        rows = ['employee_id,timestamp,kind']
        lines = {}
        flat = []
        for eid, ps in emps.items():
            for t, k in ps:
                flat.append((eid, t, k))
        rng.shuffle(flat)
        for idx, (eid, t, k) in enumerate(flat):
            rows.append(f'{eid},{t:%Y-%m-%d %H:%M:%S},{k}')
            lines.setdefault(eid, []).append((t, k, idx + 2))
        csv = '\n'.join(rows) + '\n'
        open(os.path.join(WORK, 'p.csv'), 'w').write(csv)
        open(os.path.join(WORK, 'policy.toml'), 'w').write(policy_toml(policy))
        month_day = first + dt.timedelta(days=rng.randint(0, days))
        month = f'{month_day.year:04d}-{month_day.month:02d}'
        r = subprocess.run([BIN, 'report', '--policy', os.path.join(WORK, 'policy.toml'), '--punches',
                            os.path.join(WORK, 'p.csv'), '--month', month, '--format', 'json',
                            '--data-from', str(first)], capture_output=True, text=True)
        if r.returncode not in (0, 4):
            # 範囲外（2100年）などは起きない設定。
            print('UNEXPECTED', case_no, r.returncode, r.stderr[:200])
            mismatches += 1
            continue
        got = json.loads(r.stdout)
        msd = policy['month_start_day']
        y, m = month_day.year, month_day.month
        p_start = dt.date(y, m, msd)
        p_end = (dt.date(y + (m == 12), m % 12 + 1, msd))
        for emp in got['employees']:
            eid = emp['employee_id']
            status, res = pair(lines.get(eid, []))
            exp_refused = None
            exp_ledger = None
            exp_bs = []
            if status == 'refused':
                STATS['refused_anomaly'] = STATS.get('refused_anomaly', 0) + 1
                for k, _, _ in res:
                    STATS['an_' + k] = STATS.get('an_' + k, 0) + 1
                exp_refused = sorted((k, str(d), ln) for k, d, ln in res)
            else:
                shifts = res
                case = {'schema': 'kintai.case.v1', 'data_from': str(first), 'policy': dict(policy, ambiguity=policy['ambiguity']),
                        'shifts': [{'start': fmt_min(s['start']), 'end': fmt_min(s['end']),
                                    'breaks': [[fmt_min(a), fmt_min(b)] for a, b in s['breaks']]} for s in shifts]}
                out = classify(case)
                if any(s['start'].date() != (s['end'] - dt.timedelta(minutes=1)).date() for s in shifts):
                    STATS['cross_midnight_emp'] = STATS.get('cross_midnight_emp', 0) + 1
                if out.get('ambiguities'):
                    STATS['ambiguities_use_default'] = STATS.get('ambiguities_use_default', 0) + 1
                if out.get('refused'):
                    STATS['refused_ambiguity'] = STATS.get('refused_ambiguity', 0) + 1
                    exp_refused = ('AmbiguousBoundary', out['refused']['dates'])
                else:
                    led = dict(work_days=0, work_minutes=0, overtime_minutes=0, over60_minutes=0, holiday_minutes=0,
                               night_minutes=0, within_minutes=0, extra_within_minutes=0)
                    for d in out['days']:
                        dd = dt.date.fromisoformat(d['date'])
                        if p_start <= dd < p_end:
                            led['work_days'] += 1 if d['total'] > 0 else 0
                            led['work_minutes'] += d['total']
                            led['overtime_minutes'] += d['overtime']
                            led['over60_minutes'] += d['over60']
                            led['holiday_minutes'] += d['holiday']
                            led['night_minutes'] += d['night']
                            led['within_minutes'] += d['within']
                            led['extra_within_minutes'] += d['extra_within']
                    exp_ledger = led
                exp_bs = [b for b in break_shortages(shifts) if p_start <= dt.date.fromisoformat(b['date']) < p_end]
                if exp_bs:
                    STATS['with_break_shortage'] = STATS.get('with_break_shortage', 0) + 1
                if exp_ledger and exp_ledger['over60_minutes'] > 0:
                    STATS['over60'] = STATS.get('over60', 0) + 1
                if exp_ledger and exp_ledger['holiday_minutes'] > 0:
                    STATS['holiday'] = STATS.get('holiday', 0) + 1
            # 比べる
            got_refused = emp['refused']
            problems = []
            if exp_refused is None and got_refused is not None:
                problems.append(f'refused unexpectedly: {got_refused}')
            elif isinstance(exp_refused, list):
                if not got_refused or got_refused['code'] != 'PunchAnomaly':
                    problems.append(f'expected PunchAnomaly {exp_refused}, got {got_refused}')
                else:
                    g = sorted((a['kind'], a['date'], a['line']) for a in got_refused['anomalies'])
                    if g != exp_refused:
                        problems.append(f'anomalies differ: expected {exp_refused} got {g}')
            elif isinstance(exp_refused, tuple):
                if not got_refused or got_refused['code'] != 'AmbiguousBoundary' or got_refused['dates'] != exp_refused[1]:
                    problems.append(f'expected ambiguity refusal {exp_refused}, got {got_refused}')
            if exp_ledger is not None:
                if not emp['ledger']:
                    problems.append('ledger missing')
                else:
                    g = {k: emp['ledger'][0][k] for k in exp_ledger}
                    if g != exp_ledger:
                        problems.append(f'ledger differs: expected {exp_ledger} got {g}')
            if exp_refused is None or isinstance(exp_refused, tuple):
                if emp['break_shortages'] != exp_bs:
                    problems.append(f'break shortages differ: expected {exp_bs} got {emp["break_shortages"]}')
            checked += 1
            if problems:
                mismatches += 1
                if mismatches <= 8:
                    print(f'MISMATCH case={case_no} emp={eid} month={month} policy={policy}')
                    for p in problems:
                        print('   ', p[:600])
                    with open(os.path.join(WORK, f'mismatch-{case_no}-{eid}.csv'), 'w') as f:
                        f.write(csv)
    print(f'checked employees={checked} mismatches={mismatches}')
    print('stats', STATS)


if __name__ == '__main__':
    main()
