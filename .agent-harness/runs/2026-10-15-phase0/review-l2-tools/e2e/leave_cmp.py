"""L2 査読: kintai leave（TOML＋CSV）と kintai case（caseleave JSON）の答えの突き合わせ。

使い方: python3 -I leave_cmp.py WS BIN SEED N
"""
import datetime as dt
import json
import os
import random
import subprocess
import sys

WS, BIN, SEED, N = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
WORK = os.path.join(WS, '.tmp', 'e2e', 'leave')
os.makedirs(WORK, exist_ok=True)


def rand_date(rng, lo, hi):
    return lo + dt.timedelta(days=rng.randint(0, (hi - lo).days))


def main():
    diffs = 0
    stats = {}
    for case_no in range(N):
        rng = random.Random(SEED * 7919 + case_no)
        as_of = rand_date(rng, dt.date(2024, 1, 1), dt.date(2030, 12, 31))
        emps = []
        for e in range(4):
            hire = rand_date(rng, dt.date(2015, 1, 1), as_of)
            days = rng.choice([5, 5, 4, 3, 2, 1, 0, 6, 7])
            minutes = rng.choice([2400, 1800, 1799, 1200, 600, 0, 2100])
            annual = rng.choice([None, None, None, 217, 216, 169, 168, 121, 120, 73, 72, 48, 47, 0])
            att = [rng.choice(['meets', 'meets', 'below', 'unknown']) for _ in range(rng.randint(0, 12))]
            grants = None
            if rng.random() < 0.2:
                grants = sorted([(rand_date(rng, hire, as_of + dt.timedelta(days=400)), rng.choice([20, 22, 10, 30, 40]))
                                 for _ in range(rng.randint(1, 4))])
            taken = []
            for _ in range(rng.randint(0, 9)):
                taken.append((rand_date(rng, hire + dt.timedelta(days=200), as_of + dt.timedelta(days=200)), rng.choice(['full', 'half', 'half', 'full', 'hours' if rng.random() < 0.05 else 'half'])))
            emps.append(dict(id=f'E{e:02d}', hire=hire, days=days, minutes=minutes, annual=annual, att=att, grants=grants, taken=taken))
        # TOML
        lines = ['schema = "kintai.policy.v1"', '', '[workplace]', 'work_system = "standard"',
                 'scheduled_minutes_per_day = 420', 'legal_holiday_weekday = "sun"', 'month_start_day = 1', '',
                 '[agreement36]', 'kind = "none"', 'year_start_month = 4', '']
        for e in emps:
            lines += ['[[employee]]', f'id = "{e["id"]}"', f'hire_date = "{e["hire"]}"',
                      f'weekly_scheduled_days = {e["days"]}', f'weekly_scheduled_minutes = {e["minutes"]}',
                      'attendance = [' + ', '.join(f'"{a}"' for a in e['att']) + ']']
            if e['annual'] is not None:
                lines.append(f'annual_scheduled_days = {e["annual"]}')
            if e['grants'] is not None:
                lines.append('grants = [' + ', '.join(f'{{date = "{d}", halves = {h}}}' for d, h in e['grants']) + ']')
            lines.append('')
        open(os.path.join(WORK, 'policy.toml'), 'w').write('\n'.join(lines))
        rows = ['employee_id,date,unit']
        for e in emps:
            for d, u in e['taken']:
                rows.append(f'{e["id"]},{d},{u}')
        open(os.path.join(WORK, 'leaves.csv'), 'w').write('\n'.join(rows) + '\n')
        for e in emps:
            r = subprocess.run([BIN, 'leave', '--policy', os.path.join(WORK, 'policy.toml'), '--leaves',
                                os.path.join(WORK, 'leaves.csv'), '--as-of', str(as_of), '--employee', e['id'],
                                '--format', 'json'], capture_output=True, text=True)
            case = {'schema': 'kintai.caseleave.v1', 'as_of': str(as_of),
                    'employee': {'hire_date': str(e['hire']), 'weekly_scheduled_days': e['days'],
                                 'weekly_scheduled_minutes': e['minutes'], 'annual_scheduled_days': e['annual'],
                                 'attendance': e['att'],
                                 'grants': None if e['grants'] is None else [{'date': str(d), 'halves': h} for d, h in e['grants']]},
                    'taken': [{'date': str(d), 'unit': u} for d, u in e['taken']]}
            open(os.path.join(WORK, 'case.json'), 'w').write(json.dumps(case))
            c = subprocess.run([BIN, 'case', os.path.join(WORK, 'case.json')], capture_output=True, text=True)
            key = f'{r.returncode}/{c.returncode}'
            stats[key] = stats.get(key, 0) + 1
            same = r.returncode == c.returncode
            if same and r.returncode in (2, 3):
                same = json.loads(r.stderr)['error']['code'] == json.loads(c.stderr)['error']['code']
            elif same and r.returncode == 0:
                emp = json.loads(r.stdout)['employees'][0]
                del emp['employee_id']
                leave = json.loads(c.stdout)
                del leave['schema']
                same = list(emp.items()) == list(leave.items())
            if not same:
                diffs += 1
                if diffs <= 6:
                    print('DIFF', case_no, e['id'], 'leave:', r.returncode, r.stderr[:200], '| case:', c.returncode, c.stderr[:200])
                    if r.returncode == 0 and c.returncode == 0:
                        print('   leave', r.stdout[:400])
                        print('   case ', c.stdout[:400])
    print('compared', sum(stats.values()), 'diffs', diffs, 'exit pairs', stats)


main()
