# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""自己検査: docs/spec/vectors/ の数値例を、オラクルが再現するかを確かめる（仕様 第2版）。

使い方: python3 tools/oracle/selftest.py [--vectors DIR]
全件一致なら終了コード0、1件でも違えば1。

期待値の書式は各JSONのとおりに読む。一部の項目は、期待値に書かれたものだけを比べる。
- 区分: days・periods の全項目。exit_code（既定0）・refused（既定 null）・ambiguities（既定 []）。
- 36協定: findings_by_month は書いた月の findings を比べ、書いていない月は UnknownHistory 以外の
  指摘が無いこと。コードが文字列なら n を比べない。special_count_by_month は書いた月だけ。
- 年休: lots・five_day は書いた項目だけ。"note" は説明なので比べない。
"""
import argparse
import datetime as dt
import json
import os
import sys

sys.dont_write_bytecode = True   # .gitignore に __pycache__ が無いので、作業ツリーを汚さない

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
DEFAULT_VECTOR_DIR = os.path.join(ROOT, 'docs', 'spec', 'vectors')
sys.path.insert(0, HERE)

from classify_oracle import classify          # noqa: E402
from common import next_start, run_bytes       # noqa: E402
from leave_oracle import leave                # noqa: E402
from limits36_oracle import limits36          # noqa: E402


def run(handler, case):
    """CLI と同じ経路で動かし、(終了コード, 出力のJSON, エラーのJSON) を返す。"""
    code, out, err = run_bytes(handler, json.dumps(case, ensure_ascii=False).encode('utf-8'))
    return code, (json.loads(out) if out else None), (json.loads(err) if err else None)


class Checker:
    def __init__(self):
        self.problems = []

    def equal(self, label, actual, expected):
        if actual != expected:
            self.problems.append(f'{label}: 期待 {json.dumps(expected, ensure_ascii=False)}'
                                 f' / 実際 {json.dumps(actual, ensure_ascii=False)}')

    def fields(self, label, actual, expected):
        """expected に書かれた項目だけを比べる（note は除く）。"""
        if not isinstance(actual, dict):
            self.problems.append(f'{label}: 出力がない')
            return
        for key, value in expected.items():
            if key == 'note':
                continue
            self.equal(f'{label}.{key}', actual.get(key, '<なし>'), value)


# ---------------------------------------------------------------- 区分

def check_classify(vector):
    c = Checker()
    expect = vector['expect']
    code, out, err = run(classify, vector['case'])
    c.equal('終了コード', code, expect.get('exit_code', 0))
    if out is None:
        c.problems.append(f'出力なし: {err}')
        return c.problems
    c.equal('refused', out['refused'], expect.get('refused'))
    c.equal('ambiguities', out['ambiguities'], expect.get('ambiguities', []))
    days = {row['date']: row for row in out['days']}
    c.equal('days の日付', sorted(days), sorted(expect['days']))
    for date, expected in expect['days'].items():
        c.fields(f'days[{date}]', days.get(date), expected)
    periods = {row['start']: row for row in out['periods']}
    c.equal('periods の初日', sorted(periods), sorted(expect['periods']))
    for start, expected in expect['periods'].items():
        c.fields(f'periods[{start}]', periods.get(start), expected)
    return c.problems


# ---------------------------------------------------------------- 36協定

def findings_match(actual, expected):
    """期待値の findings は、文字列（n は比べない）か {"code", "n"}。並びも比べる。"""
    if actual is None or len(actual) != len(expected):
        return False
    for got, want in zip(actual, expected):
        if isinstance(want, str):
            if got.get('code') != want:
                return False
        elif got != want:
            return False
    return True


def check_limits36(vector):
    c = Checker()
    code, out, err = run(limits36, vector['case'])
    c.equal('終了コード', code, 0)
    if out is None:
        c.problems.append(f'出力なし: {err}')
        return c.problems
    c.equal('notices', out.get('notices'), ['OwnEmployerOnly'])
    months = out['months']
    by_start = {m['start']: m for m in months}
    last = months[-1]
    expect = vector['expect']
    for key, value in expect.items():
        if key == 'note':
            continue
        if key in ('total', 'special_count', 'year_overtime'):
            c.equal(key, [m[key] for m in months], value)
        elif key == 'sums_at_last':
            for n, total in value.items():
                c.equal(f'最後の月の sums[{n}]', last['sums'].get(n), total)
        elif key == 'sums_at':
            for start, sums in value.items():
                for n, total in sums.items():
                    c.equal(f'{start} の sums[{n}]', by_start.get(start, {}).get('sums', {}).get(n), total)
        elif key == 'findings_at_last':
            if not findings_match(last['findings'], value):
                c.equal('最後の月の findings', last['findings'], value)
        elif key == 'findings_by_month':
            for m in months:
                if m['start'] in value:
                    if not findings_match(m['findings'], value[m['start']]):
                        c.equal(f'{m["start"]} の findings', m['findings'], value[m['start']])
                else:      # 書いていない月は指摘なし（UnknownHistory だけは出てよい）
                    others = [f for f in m['findings'] if f['code'] != 'UnknownHistory']
                    c.equal(f'{m["start"]} の findings（書いていない月）', others, [])
            for start in value:
                if start not in by_start:
                    c.problems.append(f'findings_by_month の月 {start} が出力にない')
        elif key == 'special_count_by_month':
            for start, count in value.items():
                c.equal(f'{start} の special_count', by_start.get(start, {}).get('special_count'), count)
        elif key == 'special_count_last':
            c.equal('最後の月の special_count', last['special_count'], value)
        elif key == 'year_overtime_known_at_last':
            c.equal('最後の月の year_overtime_known', last.get('year_overtime_known'), value)
        else:
            c.problems.append(f'自己検査が知らない期待値の項目: {key}')
    return c.problems


# ---------------------------------------------------------------- 年休

def check_leave(vector):
    c = Checker()
    if vector['id'] == 'derived-first-grant-dates':
        for item in vector['cases']:
            hire = dt.date.fromisoformat(item['hire_date'])
            first = dt.date.fromisoformat(item['first_grant'])
            c.equal(f'{item["hire_date"]} の 次の初日(雇入れ日, 6)', next_start(hire, 6).isoformat(),
                    item['first_grant'])
            # ケースを通しても確かめる: 付与日の当日は付与があり、前日には無い。
            for as_of, expected_dates in ((first, [item['first_grant']]),
                                          (first - dt.timedelta(days=1), [])):
                case = {'schema': 'kintai.caseleave.v1', 'as_of': as_of.isoformat(),
                        'employee': {'hire_date': item['hire_date'], 'weekly_scheduled_days': 5,
                                     'weekly_scheduled_minutes': 2400, 'annual_scheduled_days': None,
                                     'attendance': [], 'grants': None},
                        'taken': []}
                code, out, err = run(leave, case)
                c.equal(f'{item["hire_date"]} as_of={as_of} の終了コード', code, 0)
                if out is not None:
                    c.equal(f'{item["hire_date"]} as_of={as_of} の付与日',
                            [lot['grant_date'] for lot in out['lots']], expected_dates)
        return c.problems

    code, out, err = run(leave, vector['case'])
    c.equal('終了コード', code, 0)
    if out is None:
        c.problems.append(f'出力なし: {err}')
        return c.problems
    for key, value in vector['expect'].items():
        if key == 'note':
            continue
        if key in ('five_day', 'lots'):
            c.equal(f'{key} の件数', len(out[key]), len(value))
            for i, expected in enumerate(value):
                c.fields(f'{key}[{i}]', out[key][i] if i < len(out[key]) else None, expected)
        elif key in ('balance_halves', 'notices'):
            c.equal(key, out[key], value)
        else:
            c.problems.append(f'自己検査が知らない期待値の項目: {key}')
    return c.problems


def main(argv=None):
    parser = argparse.ArgumentParser(description='数値例による自己検査')
    parser.add_argument('--vectors', default=DEFAULT_VECTOR_DIR, help='vectors のディレクトリ')
    args = parser.parse_args(argv)
    plan = [('official-examples.json', check_classify),
            ('limits36-official.json', check_limits36),
            ('leave-official.json', check_leave)]
    matched = 0
    total = 0
    for name, check in plan:
        with open(os.path.join(args.vectors, name), encoding='utf-8') as stream:
            vectors = json.load(stream)['vectors']
        for vector in vectors:
            total += 1
            problems = check(vector)
            if problems:
                print(f'不一致 {name} {vector["id"]}')
                for problem in problems:
                    print(f'    {problem}')
            else:
                matched += 1
                print(f'一致   {name} {vector["id"]}')
    print(f'selftest: 一致 {matched}/{total}')
    return 0 if matched == total else 1


if __name__ == '__main__':
    sys.exit(main())
