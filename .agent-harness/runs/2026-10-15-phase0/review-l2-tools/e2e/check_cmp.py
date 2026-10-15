"""L2 査読: kintai check（打刻CSV＋方針＋履歴CSV → 36協定の表）を、モデルと kintai case で突き合わせる。

月ごとの O・H は、e2e.py のモデル（仕様から書いた対と丸め）と別実装の区分から求め、
36協定の判定そのものは kintai case（差分試験で別実装と一致を確かめた道筋）に任せる。
比べるのは、協定月の範囲・O・H・T・直近の合計・発動月の数・年の時間外・判定・拒否・終了コード。
使い方: L2_WS=WS python3 -I check_cmp.py WS BIN SEED N
"""
import datetime as dt
import json
import os
import random
import re
import subprocess
import sys

WS, BIN, SEED, N = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
os.environ['L2_WS'] = WS
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import e2e  # noqa: E402

WORK = os.path.join(WS, '.tmp', 'e2e', 'check')
os.makedirs(WORK, exist_ok=True)


def period_start(d, msd):
    if d.day >= msd:
        return dt.date(d.year, d.month, msd)
    y, m = (d.year - 1, 12) if d.month == 1 else (d.year, d.month - 1)
    return dt.date(y, m, msd)


def next_period(p):
    y, m = (p.year + 1, 1) if p.month == 12 else (p.year, p.month + 1)
    return dt.date(y, m, p.day)


def parse_check(text):
    res = {}
    lines = text.split('\n')
    i = 0
    while i < len(lines):
        line = lines[i]
        m = re.match(r'^(E\d{3})(: (.*))?$', line)
        if m:
            eid = m.group(1)
            if m.group(3):
                res[eid] = {'refused': m.group(3)}
                i += 1
                continue
            rows = []
            i += 2  # 見出しの行を飛ばす
            while i < len(lines) and lines[i].startswith('  ') and not lines[i].startswith('    '):
                cells = re.split(r'\s{2,}', lines[i].strip())
                rows.append(cells)
                i += 1
            res[eid] = {'rows': rows}
            continue
        i += 1
    return res


def finding_text(f):
    return f"{f['code']}(n={f['n']})" if 'n' in f else f['code']


def main():
    diffs = 0
    stats = {}
    e2e.ANOMALY_RATE = 0.002
    for case_no in range(N):
        rng = random.Random(SEED * 31337 + case_no)
        policy, first, days, emps = e2e.gen(rng)
        msd = policy['month_start_day']
        kind = rng.choice(['none', 'general', 'special', 'special'])
        agreement = {'kind': kind, 'year_start_month': rng.randint(1, 12)}
        if kind != 'none':
            agreement['month_start_day'] = msd
            agreement['monthly_limit_minutes'] = rng.choice([2700, 2400, 1800, 0])
            agreement['annual_limit_minutes'] = rng.choice([21600, 18000, 6000])
        if kind == 'special':
            agreement['special_monthly_total_limit_minutes'] = rng.choice([5999, 5100, 4800, 3000])
            agreement['special_annual_limit_minutes'] = rng.choice([43200, 40800, 10000])
            agreement['special_months_max'] = rng.choice([6, 3, 0, 1])
        # 方針（e2e の方針に協定を足す）
        toml = e2e.policy_toml(policy)
        toml = toml.replace('[agreement36]\nkind = "none"\nyear_start_month = 4\n',
                            '[agreement36]\n' + ''.join(f'{k} = ' + (f'"{v}"' if isinstance(v, str) else str(v)) + '\n' for k, v in agreement.items()))
        open(os.path.join(WORK, 'policy.toml'), 'w').write(toml)
        rows = ['employee_id,timestamp,kind']
        lines = {}
        flat = [(eid, t, k) for eid, ps in emps.items() for t, k in ps]
        for idx, (eid, t, k) in enumerate(flat):
            rows.append(f'{eid},{t:%Y-%m-%d %H:%M:%S},{k}')
            lines.setdefault(eid, []).append((t, k, idx + 2))
        open(os.path.join(WORK, 'p.csv'), 'w').write('\n'.join(rows) + '\n')
        # 協定月の範囲
        p0 = period_start(first, msd)
        first_month = p0 if p0 == first else next_period(p0)
        target_day = first + dt.timedelta(days=rng.randint(0, days + 20))
        month = f'{target_day.year:04d}-{target_day.month:02d}'
        last_month = dt.date(target_day.year, target_day.month, msd)
        # 履歴（最初の協定月より前の5か月から、歯抜けで）
        hist = {}
        hrows = ['employee_id,month_start,total_minutes']
        for eid in emps:
            hist[eid] = []
            p = first_month
            back = []
            for _ in range(rng.randint(0, 6)):
                y, m = (p.year - 1, 12) if p.month == 1 else (p.year, p.month - 1)
                p = dt.date(y, m, msd)
                back.append(p)
            for p in back:
                if rng.random() < 0.85:
                    t = rng.choice([0, 1200, 2400, 4800, 5999, 6000, rng.randint(0, 9000)])
                    hist[eid].append((p, t))
                    hrows.append(f'{eid},{p},{t}')
        open(os.path.join(WORK, 'h.csv'), 'w').write('\n'.join(hrows) + '\n')
        r = subprocess.run([BIN, 'check', '--policy', os.path.join(WORK, 'policy.toml'), '--punches',
                            os.path.join(WORK, 'p.csv'), '--month', month, '--data-from', str(first),
                            '--history', os.path.join(WORK, 'h.csv')], capture_output=True, text=True)
        if first_month > last_month:
            ok = r.returncode == 2
            stats['range_invalid'] = stats.get('range_invalid', 0) + 1
            if not ok:
                diffs += 1
                print('DIFF range', case_no, r.returncode, r.stderr[:200])
            continue
        if r.returncode not in (0, 4):
            diffs += 1
            print('UNEXPECTED', case_no, r.returncode, r.stderr[:300])
            continue
        got = parse_check(r.stdout)
        any_refused = False
        for eid in emps:
            status, res = e2e.pair(lines.get(eid, []))
            if status == 'refused':
                any_refused = True
                if 'refused' not in got.get(eid, {}) or 'PunchAnomaly' not in got[eid]['refused']:
                    diffs += 1
                    print('DIFF anomaly', case_no, eid, got.get(eid))
                continue
            shifts = res
            case = {'schema': 'kintai.case.v1', 'data_from': str(first), 'policy': policy,
                    'shifts': [{'start': e2e.fmt_min(s['start']), 'end': e2e.fmt_min(s['end']),
                                'breaks': [[e2e.fmt_min(a), e2e.fmt_min(b)] for a, b in s['breaks']]} for s in shifts]}
            out = e2e.classify(case)
            if out.get('refused'):
                any_refused = True
                if 'refused' not in got.get(eid, {}) or 'AmbiguousBoundary' not in got[eid]['refused']:
                    diffs += 1
                    print('DIFF ambiguity', case_no, eid, got.get(eid))
                continue
            periods = {p['start']: p for p in out['periods']}
            months = []
            p = first_month
            while p <= last_month:
                pr = periods.get(str(p))
                months.append({'start': str(p), 'overtime': pr['overtime'] if pr else 0, 'holiday': pr['holiday'] if pr else 0})
                p = next_period(p)
            c36 = {'schema': 'kintai.case36.v1', 'agreement': {k: v for k, v in agreement.items() if k != 'month_start_day'},
                   'months': months, 'history': [{'start': str(p), 'total': t} for p, t in hist[eid]]}
            open(os.path.join(WORK, 'c36.json'), 'w').write(json.dumps(c36))
            c = subprocess.run([BIN, 'case', os.path.join(WORK, 'c36.json')], capture_output=True, text=True)
            if c.returncode != 0:
                diffs += 1
                print('case36 failed', case_no, eid, c.stderr[:200])
                continue
            exp = json.loads(c.stdout)['months']
            g = got.get(eid, {}).get('rows')
            if g is None or len(g) != len(exp):
                diffs += 1
                print('DIFF rows', case_no, eid, 'expected', len(exp), 'got', None if g is None else len(g))
                continue
            for row, mo, em in zip(g, months, exp):
                lower = '' if em['year_overtime_known'] else '（下限）'
                want = [em['start'], str(mo['overtime']), str(mo['holiday']), str(em['total'])]
                want += ['不明' if em['sums'][k] is None else str(em['sums'][k]) for k in ('2', '3', '4', '5', '6')]
                want += [f"{em['special_count']}{lower}", f"{em['year_overtime']}{lower}"]
                want += [' '.join(finding_text(f) for f in em['findings']) or 'なし']
                if row != want:
                    diffs += 1
                    if diffs <= 6:
                        print('DIFF row', case_no, eid, '\n   got ', row, '\n   want', want)
                for f in em['findings']:
                    stats['f_' + f['code']] = stats.get('f_' + f['code'], 0) + 1
            stats['emp_compared'] = stats.get('emp_compared', 0) + 1
        want_exit = 4 if any_refused else 0
        if r.returncode != want_exit:
            diffs += 1
            print('DIFF exit', case_no, r.returncode, want_exit)
    print('diffs', diffs, 'stats', stats)


main()
