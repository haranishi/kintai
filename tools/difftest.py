# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""差分試験: 無作為のケースを、Python のオラクルと Rust の本実装の両方に通して比べる。

使い方:
  python3 tools/difftest.py --cases N --seed S [--bin PATH] [--stats] [--show K] [--kind KIND]

- ケースは tools/oracle/gen_cases.py が、種類（classify・case36・leave）ごとに N 件ずつ作る。
- 本実装は `PATH case FILE` で呼ぶ。--bin が無ければ .cargo-target/release/kintai、
  それも無ければ target/ と .cargo-target/ の release・debug。どれも無ければ失敗にする。
  --oracle-only を付けたときだけ、オラクルだけを全ケースで動かす（オラクルが例外を出さないことの確認）。
- 比べるのは、①終了コード、②終了コードが0か4なら標準出力のJSONの全フィールド、
  ③終了コードが2か3なら error.code（message は比べない）。
- 不一致があれば、ケースを tools/difftest-out/ に残す（.gitignore にその行が無ければ作らず、
  .cargo-target/difftest-out/ を使う）。先頭の数件を詳しく表示して、終了コード1。


対応する受け入れ条件: AC-03.14（区分）、AC-05.11（36協定）、AC-06.11（年休）、AC-13.6（エラーのコードと終了コード）
"""
import argparse
import collections
import datetime as dt
import json
import os
import subprocess
import sys

sys.dont_write_bytecode = True   # .gitignore に __pycache__ が無いので、作業ツリーを汚さない
import tempfile
import time
import traceback

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'tools', 'oracle'))

import gen_cases                                # noqa: E402
from classify_oracle import classify            # noqa: E402
from common import next_start, run_bytes, week_start_of  # noqa: E402
from leave_oracle import leave                  # noqa: E402
from limits36_oracle import limits36            # noqa: E402

HANDLERS = {'classify': classify, 'case36': limits36, 'leave': leave}
KIND_LABEL = {'classify': '区分（kintai.case.v1）', 'case36': '36協定（kintai.case36.v1）',
              'leave': '年休（kintai.caseleave.v1）'}
DEFAULT_BINS = [os.path.join(ROOT, d, profile, 'kintai')
                for d in ('target', '.cargo-target') for profile in ('release', 'debug')]


def main(argv=None):
    parser = argparse.ArgumentParser(description='オラクルと本実装の差分試験')
    parser.add_argument('--cases', type=int, required=True, help='種類ごとの件数')
    parser.add_argument('--seed', type=int, required=True, help='乱数の種')
    parser.add_argument('--bin', help='本実装のコマンド（PATH case FILE で呼ぶ）')
    parser.add_argument('--oracle-only', action='store_true',
                        help='本実装なしで、別実装だけを動かす（別実装の自己確認用。本実装が見つからないとき、これが無ければ失敗にする）')
    parser.add_argument('--kind', choices=list(HANDLERS), action='append', help='試す種類（省略で3種類）')
    parser.add_argument('--stats', action='store_true', help='生成したケースの分布と、境界を踏んだ件数を表示')
    parser.add_argument('--show', type=int, default=5, help='詳しく表示する不一致の件数')
    parser.add_argument('--timeout', type=float, default=60.0, help='本実装の1件あたりの時間の上限（秒）')
    parser.add_argument('--out-dir', help='不一致のケースを残すディレクトリ（既定は上の説明のとおり）')
    args = parser.parse_args(argv)

    binary = find_binary(args.bin)
    if binary is None and args.bin:
        print(f'--bin に指定したファイルがありません: {args.bin}', file=sys.stderr)
        return 2
    kinds = args.kind or list(HANDLERS)
    if binary is None and not args.oracle_only:
        print('本実装が見つかりません（target/ と .cargo-target/ の release・debug を探した）。'
              '--bin で指定するか、cargo build -p kintai-cli で作ってください。'
              '別実装だけを動かすなら --oracle-only を付けます。', file=sys.stderr)
        return 2
    if binary is None:
        print('本実装なし（--oracle-only）。オラクルだけを全ケースで動かします。')
    else:
        print(f'本実装: {binary}')

    started = time.time()
    crashes = []
    mismatches = []
    records = {kind: [] for kind in kinds}
    with tempfile.TemporaryDirectory(prefix='kintai-difftest-') as workdir:
        for kind in kinds:
            for index in range(args.cases):
                item = gen_cases.generate(kind, args.seed, index)
                try:
                    oracle = run_bytes(HANDLERS[kind], item['data'])
                    broken = check_oracle(kind, item['case'], oracle)
                except Exception:      # オラクルのプログラムの誤り
                    crashes.append((item, traceback.format_exc()))
                    continue
                if broken:             # オラクルの出力が、別の数え方の検算と合わない
                    crashes.append((item, 'オラクルの検算が合わない:\n  ' + '\n  '.join(broken)))
                records[kind].append((item, oracle))
                if binary is not None:
                    path = os.path.join(workdir, f'{kind}-{index:05d}.json')
                    with open(path, 'wb') as stream:
                        stream.write(item['data'])
                    rust = run_binary(binary, path, args.timeout)
                    problems = compare(oracle, rust)
                    if problems:
                        mismatches.append((item, oracle, rust, problems))

    elapsed = time.time() - started
    total = sum(len(v) for v in records.values()) + len(crashes)
    if args.stats:
        print_stats(records, args.seed)

    if crashes:
        print(f'\nオラクルが例外を出した、または検算が合わない: {len(crashes)} 件')
        for item, trace in crashes[:args.show]:
            print(f'--- {item["kind"]} #{item["index"]}（{item["tag"]}）')
            print(trace)
    if binary is None:
        print(f'\n本実装なし: オラクルだけを {total} 件で実行（{elapsed:.1f}秒）。例外 {len(crashes)} 件。')
        return 1 if crashes else 0

    print(f'\n比べた件数: {total}（{elapsed:.1f}秒）。不一致 {len(mismatches)} 件、オラクルの例外 {len(crashes)} 件。')
    if mismatches:
        out_dir = args.out_dir or output_dir()
        os.makedirs(out_dir, exist_ok=True)
        by_tag = collections.Counter(f'{item["kind"]}:{item["tag"]}' for item, _, _, _ in mismatches)
        print('不一致の内訳（種類:生成のタグ。タグの Qn は報告書の疑問の番号）:')
        for tag, count in by_tag.most_common():
            print(f'  {count:5d}  {tag}')
        for item, oracle, rust, problems in mismatches:
            save_mismatch(out_dir, item, oracle, rust, problems)
        print(f'不一致のケースを {out_dir} に残した。先頭の {min(args.show, len(mismatches))} 件:')
        for item, oracle, rust, problems in mismatches[:args.show]:
            print(f'--- {item["kind"]} #{item["index"]}（{item["tag"]}、意図 {item["intended"]}）')
            for problem in problems[:12]:
                print(f'    {problem}')
    return 1 if (mismatches or crashes) else 0


# ---------------------------------------------------------------- 本実装

def find_binary(explicit):
    if explicit:
        return explicit if os.path.isfile(explicit) else None
    for path in DEFAULT_BINS:
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path
    return None


def run_binary(binary, path, timeout):
    try:
        done = subprocess.run([binary, 'case', path], capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return ('timeout', '', '')
    return (done.returncode, done.stdout.decode('utf-8', 'replace'), done.stderr.decode('utf-8', 'replace'))


def output_dir():
    """tools/difftest-out/ が .gitignore にあればそこ、無ければ .cargo-target/difftest-out/。"""
    try:
        with open(os.path.join(ROOT, '.gitignore'), encoding='utf-8') as stream:
            lines = {line.strip() for line in stream}
    except OSError:
        lines = set()
    if lines & {'tools/difftest-out/', '/tools/difftest-out/', 'tools/difftest-out', '/tools/difftest-out'}:
        return os.path.join(ROOT, 'tools', 'difftest-out')
    return os.path.join(ROOT, '.cargo-target', 'difftest-out')


def save_mismatch(out_dir, item, oracle, rust, problems):
    stem = os.path.join(out_dir, f'{item["kind"]}-{item["index"]:05d}')
    with open(stem + '.json', 'wb') as stream:
        stream.write(item['data'])
    with open(stem + '.result.txt', 'w', encoding='utf-8') as stream:
        stream.write(f'kind={item["kind"]} index={item["index"]} tag={item["tag"]} intended={item["intended"]}\n')
        stream.write('\n'.join(problems) + '\n')
        for name, (code, out, err) in (('oracle', oracle), ('rust', rust)):
            stream.write(f'\n== {name}: exit={code}\n-- stdout\n{out}\n-- stderr\n{err}\n')


# ---------------------------------------------------------------- 比べる

def parse_json(text):
    try:
        return True, json.loads(text)
    except ValueError:
        return False, None


def error_code(stderr):
    ok, value = parse_json(stderr)
    if not ok:      # 標準エラーに別の行があるときは、最後のJSONの行を使う
        for line in reversed([line for line in stderr.splitlines() if line.strip()]):
            ok, value = parse_json(line)
            if ok:
                break
    if ok and isinstance(value, dict) and isinstance(value.get('error'), dict):
        return value['error'].get('code')
    return None


def compare(oracle, rust):
    o_code, o_out, o_err = oracle
    r_code, r_out, r_err = rust
    if o_code != r_code:
        detail = [f'終了コード: オラクル {o_code} / 本実装 {r_code}']
        if o_code in (2, 3):
            detail.append(f'  オラクルの error.code: {error_code(o_err)}')
        if r_code in (2, 3):
            detail.append(f'  本実装の error.code: {error_code(r_err)}')
        if r_code not in (0, 2, 3, 4):
            detail.append(f'  本実装の標準エラー: {r_err.strip()[:300]}')
        return detail
    if o_code in (0, 4):
        ok, r_json = parse_json(r_out)
        if not ok:
            return [f'本実装の標準出力がJSONとして読めない: {r_out.strip()[:300]}']
        differences = []
        json_diff(json.loads(o_out), r_json, '$', differences)
        return differences
    if o_code in (2, 3):
        o_error, r_error = error_code(o_err), error_code(r_err)
        if r_error is None:
            return [f'本実装の標準エラーに error.code が無い: {r_err.strip()[:300]}']
        if o_error != r_error:
            return [f'error.code: オラクル {o_error} / 本実装 {r_error}']
    return []


def json_diff(a, b, path, out, limit=40):
    if len(out) >= limit:
        return
    if type(a) is not type(b):
        out.append(f'{path}: オラクル {short(a)} / 本実装 {short(b)}')
    elif isinstance(a, dict):
        for key in sorted(set(a) | set(b)):
            if key not in b:
                out.append(f'{path}.{key}: 本実装に無い（オラクル {short(a[key])}）')
            elif key not in a:
                out.append(f'{path}.{key}: オラクルに無い（本実装 {short(b[key])}）')
            else:
                json_diff(a[key], b[key], f'{path}.{key}', out, limit)
    elif isinstance(a, list):
        if len(a) != len(b):
            out.append(f'{path}: 要素の数 オラクル {len(a)} / 本実装 {len(b)}')
        for i in range(min(len(a), len(b))):
            json_diff(a[i], b[i], f'{path}[{i}]', out, limit)
    elif a != b:
        out.append(f'{path}: オラクル {short(a)} / 本実装 {short(b)}')


def short(value):
    text = json.dumps(value, ensure_ascii=False)
    return text if len(text) <= 120 else text[:117] + '...'


# ---------------------------------------------------------------- オラクルの検算

def check_oracle(kind, case, oracle):
    """オラクルの出力を、出力の約束（case-format*.md の合計の関係）と、別の数え方で検算する。

    区分の実労働と深夜の合計は、1分ずつではなく区間の長さと交わりで数え直す。
    戻り値は問題の一覧（空なら問題なし）。
    """
    code, out, err = oracle
    if code in (2, 3):
        ok, value = parse_json(err)
        if not ok or not isinstance(value.get('error'), dict) or out:
            return ['エラーの形が {"error": {"code", "message"}} でない']
        return []
    output = json.loads(out)
    if case is None:          # 生成器が文字列で作ったケース（読めたもの）
        return ['生成器が文字列で作ったケースが、有効として読まれた（検算できない）']
    if kind == 'classify':
        return check_classify_output(case, code, output)
    if kind == 'case36':
        return check_case36_output(case, output)
    return check_leave_output(case, output)


def to_minutes(text):
    moment = dt.datetime.fromisoformat(text.replace(' ', 'T'))
    return int((moment - dt.datetime(1970, 1, 1)).total_seconds() // 60)


def night_overlap(start, end):
    """[start, end) と、毎日の [22:00, 翌5:00) の交わりの長さ（区間で数える）。"""
    total = 0
    for day in range(start // 1440 - 1, end // 1440 + 2):
        low, high = day * 1440 - 120, day * 1440 + 300      # 前日22:00〜当日5:00
        total += max(0, min(end, high) - max(start, low))
    return total


AMBIGUITY_ORDER = ['HolidayBoundary', 'WeekBoundary', 'MonthBoundary', 'IncompleteWeek']


def expected_incomplete_weeks(case):
    """IncompleteWeek の日付（週の起算日）を、ケースから別に数え直す。"""
    data_from = case.get('data_from')
    if data_from is None:
        return set()
    week_start = case['policy'].get('week_start') or 'sun'
    starts = set()
    for shift in case['shifts']:
        start = week_start_of(dt.date.fromisoformat(shift['start'][:10]), week_start)
        if start < dt.date.fromisoformat(data_from):
            starts.add(start.isoformat())
    return starts


def check_classify_output(case, code, output):
    problems = []
    incomplete = expected_incomplete_weeks(case)
    if code == 4:
        if output['days'] or output['periods'] or output['ambiguities']:
            problems.append('拒否なのに days/periods/ambiguities が空でない')
        dates = output['refused']['dates']
        if dates != sorted(set(dates)) or not dates:
            problems.append('refused.dates が重複のない昇順でない')
        if not incomplete <= set(dates):
            problems.append(f'IncompleteWeek の週の起算日 {sorted(incomplete)} が refused.dates に無い')
        return problems
    records = output['ambiguities']
    keys = [(r['date'], AMBIGUITY_ORDER.index(r['code'])) for r in records]
    if keys != sorted(keys) or len(keys) != len(set(keys)):
        problems.append('ambiguities の並びが（日付、表の順）でない')
    if records and (case['policy'].get('ambiguity') or 'reject') == 'reject':
        problems.append('reject なのに ambiguities がある')
    if {r['date'] for r in records if r['code'] == 'IncompleteWeek'} != incomplete:
        problems.append(f'IncompleteWeek の記録が、数え直した週の起算日 {sorted(incomplete)} と違う')
    days = output['days']
    if [d['date'] for d in days] != sorted({d['date'] for d in days}):
        problems.append('days が日付の昇順でない、または重複')
    for row in days:
        if row['total'] != row['within'] + row['overtime'] + row['holiday'] or row['total'] <= 0:
            problems.append(f'{row["date"]}: total の関係')
        if row['night'] != row['night_within'] + row['night_overtime'] + row['night_holiday']:
            problems.append(f'{row["date"]}: night の関係')
        if not 0 <= row['over60'] <= row['overtime']:
            problems.append(f'{row["date"]}: over60 が overtime の内数でない')
        if not 0 <= row['night_over60'] <= min(row['night_overtime'], row['over60']):
            problems.append(f'{row["date"]}: night_over60 の関係')
        if not 0 <= row['extra_within'] <= row['within'] <= 480:
            problems.append(f'{row["date"]}: within/extra_within の範囲')
    for period in output['periods']:
        if period['over60'] != max(0, period['overtime'] - 3600):
            problems.append(f'{period["start"]}: over60 が max(0, overtime−3600) でない')
    for key in ('total', 'within', 'overtime', 'over60', 'holiday', 'night'):
        if sum(p[key] for p in output['periods']) != sum(d[key] for d in days):
            problems.append(f'periods の {key} の合計が days の合計と違う')
    work = night = 0
    for shift in case['shifts']:
        start, end = to_minutes(shift['start']), to_minutes(shift['end'])
        work += end - start
        night += night_overlap(start, end)
        for b_start, b_end in shift.get('breaks') or []:
            b_start, b_end = to_minutes(b_start), to_minutes(b_end)
            work -= b_end - b_start
            night -= night_overlap(b_start, b_end)
    if sum(d['total'] for d in days) != work:
        problems.append(f'実労働の合計 {sum(d["total"] for d in days)} が、区間で数えた {work} と違う')
    if sum(d['night'] for d in days) != night:
        problems.append(f'深夜の合計 {sum(d["night"] for d in days)} が、区間で数えた {night} と違う')
    policy = case['policy']
    limit = policy.get('weekly_limit_minutes') or 2400
    weeks = collections.Counter()
    for row in days:
        weeks[week_start_of(dt.date.fromisoformat(row['date']), policy.get('week_start') or 'sun')] += row['within']
    if any(within > limit for within in weeks.values()):
        problems.append('週の法定内が上限を超えている')
    return problems


FINDING_ORDER = ['NoAgreement', 'MonthlyOvertimeExceeded', 'GeneralMonthlyLimitExceeded', 'AnnualOvertimeExceeded',
                 'GeneralAnnualLimitExceeded', 'SpecialCountExceeded', 'SpecialMonthlyTotalExceeded',
                 'SpecialAnnualExceeded', 'MonthlyTotalReached100h', 'MultiMonthSumExceeded', 'UnknownHistory']


def check_case36_output(case, output):
    problems = []
    if output.get('notices') != ['OwnEmployerOnly']:
        problems.append('notices が ["OwnEmployerOnly"] でない')
    months = output['months']
    if len(months) != len(case['months']):
        return problems + ['months の数が入力と違う']
    agreement = case['agreement']
    totals = []
    # 年の累計・発動月の数・year_overtime_known を、素朴に数え直す。
    year_sum = count = 0
    known = bool(case['months']) and dt.date.fromisoformat(case['months'][0]['start']).month == \
        agreement['year_start_month']
    for i, (m, given) in enumerate(zip(months, case['months'])):
        totals.append(given['overtime'] + given['holiday'])
        if dt.date.fromisoformat(given['start']).month == agreement['year_start_month']:
            year_sum = count = 0
            known = True
        year_sum += given['overtime']
        count += given['overtime'] > 2700
        if (m['year_overtime'], m['special_count'], m['year_overtime_known']) != (year_sum, count, known):
            problems.append(f'{given["start"]}: year_overtime/special_count/year_overtime_known')
        if m['start'] != given['start'] or m['total'] != totals[-1]:
            problems.append(f'{given["start"]}: start/total')
        if i >= 1 and m['sums']['2'] != totals[-2] + totals[-1]:
            problems.append(f'{given["start"]}: sums[2] が前の月との合計でない')
        keys = [(FINDING_ORDER.index(f['code']), f.get('n', 0)) for f in m['findings']]
        if keys != sorted(keys) or len(keys) != len(set(keys)):
            problems.append(f'{given["start"]}: findings の順が表の順でない')
        codes = {(f['code'], f.get('n')) for f in m['findings']}
        if ('SpecialCountExceeded', None) in codes and given['overtime'] <= 2700:
            problems.append(f'{given["start"]}: 発動月でない月に SpecialCountExceeded')
        statutory = agreement['kind'] == 'general' and given['overtime'] > 2700
        if statutory != (('MonthlyOvertimeExceeded', None) in codes):
            problems.append(f'{given["start"]}: MonthlyOvertimeExceeded')
        for n in range(2, 7):
            total = m['sums'][str(n)]
            if (total is None) != (('UnknownHistory', n) in codes):
                problems.append(f'{given["start"]}: UnknownHistory n={n}')
            if (total is not None and total > 4800 * n) != (('MultiMonthSumExceeded', n) in codes):
                problems.append(f'{given["start"]}: MultiMonthSumExceeded n={n}')
        if (m['total'] >= 6000) != (('MonthlyTotalReached100h', None) in codes):
            problems.append(f'{given["start"]}: MonthlyTotalReached100h')
    return problems


def check_leave_output(case, output):
    problems = []
    lots = output['lots']
    if output['balance_halves'] != sum(lot['remaining_halves'] for lot in lots):
        problems.append('balance_halves が remaining_halves の合計でない')
    for lot in lots:
        if lot['used_halves'] + lot['remaining_halves'] + lot['expired_halves'] != lot['granted_halves']:
            problems.append(f'{lot["grant_date"]}: 使用・残り・失効の合計が付与と違う')
        if lot['remaining_halves'] and lot['expired_halves']:
            problems.append(f'{lot["grant_date"]}: 残りと失効が両方ある')
    as_of = case['as_of']
    counted = [r for r in case['taken'] if r['date'] <= as_of]      # as_of より後の記録は数えない
    taken_halves = sum(2 if r['unit'] == 'full' else 1 for r in counted)
    if sum(lot['used_halves'] for lot in lots) != taken_halves:
        problems.append('消化した半日の合計が、as_of までの取得の記録の合計と違う')
    if any(lot['grant_date'] > as_of for lot in lots):
        problems.append('as_of より後の付与が出ている')
    if [f['base_date'] for f in output['five_day']] != [lot['grant_date'] for lot in lots] or \
            [r['base_date'] for r in output['register']] != [lot['grant_date'] for lot in lots]:
        problems.append('five_day/register の基準日がロットの付与日と並ばない')
    for item in output['five_day']:
        if item['status'] not in ('Met', 'InProgress', 'NotMet', 'NotApplicable'):
            problems.append(f'{item["base_date"]}: 状態名 {item["status"]}')
        if item['status'] in ('Met', 'NotApplicable') and item['needed_halves'] != 0:
            problems.append(f'{item["base_date"]}: needed_halves')
        if item['status'] in ('InProgress', 'NotMet') and item['needed_halves'] != max(0, 10 - item['taken_halves']):
            problems.append(f'{item["base_date"]}: needed_halves')
    for row in output['register']:
        dates = [r['date'] for r in row['taken']]
        if dates != sorted(dates):
            problems.append(f'{row["base_date"]}: register の取得が日付の昇順でない')
    explicit = case['employee'].get('grants') is not None
    if explicit and output['notices']:
        problems.append('grants を明示したのに notices がある')
    if output['notices'] not in ([], ['AttendanceUnconfirmed']):
        problems.append(f'notices {output["notices"]}')
    return problems


# ---------------------------------------------------------------- 統計

def print_stats(records, seed):
    for kind, items in records.items():
        print(f'\n== {KIND_LABEL[kind]}  {len(items)} 件（種 {seed}）')
        intended = collections.Counter(item['intended'] for item, _ in items)
        exits = collections.Counter(oracle[0] for _, oracle in items)
        codes = collections.Counter(error_code(oracle[2]) for _, oracle in items if oracle[0] in (2, 3))
        print('  生成の意図: ' + ', '.join(f'{k} {v}' for k, v in sorted(intended.items(), key=lambda kv: -kv[1])))
        print('  オラクルの終了コード: ' + ', '.join(f'{k}: {v}' for k, v in sorted(exits.items())))
        valid = exits.get(0, 0) + exits.get(4, 0)
        print(f'  有効 {exits.get(0, 0)}（{exits.get(0, 0) / max(1, len(items)):.0%}）・曖昧さの拒否 {exits.get(4, 0)}'
              f'・不正 {exits.get(2, 0)}・対応外 {exits.get(3, 0)}'
              f'（不正と対応外で {(exits.get(2, 0) + exits.get(3, 0)) / max(1, len(items)):.0%}、0か4で {valid}）')
        if codes:
            print('  エラーのコード: ' + ', '.join(f'{k} {v}' for k, v in codes.most_common()))
        disagree = [(item, oracle) for item, oracle in items if not matches_intent(item, oracle)]
        print(f'  生成の意図とオラクルの結果の食い違い（probe を除く）: {len(disagree)} 件')
        for item, oracle in disagree[:5]:
            print(f'    #{item["index"]} {item["tag"]}: 意図 {item["intended"]} / オラクル {oracle[0]}'
                  f' {error_code(oracle[2]) or ""}')
        probes = collections.Counter(item['tag'] for item, _ in items if item['intended'] == 'probe')
        if probes:
            print('  読みで答えが変わる probe: ' + ', '.join(f'{k} {v}' for k, v in sorted(probes.items())))
        for prefix, label in (('json-', 'JSON の読み方の場面'), ('order-', '検査の順序の場面'),
                              ('asof-', 'as_of で対応外の判定から除く・除かない場面'),
                              ('cap-', '上限44640分の場面')):
            outcomes = collections.defaultdict(collections.Counter)
            for item, oracle in items:
                if item['tag'].startswith(prefix):
                    outcomes[item['tag']][error_code(oracle[2]) if oracle[0] in (2, 3) else f'終了{oracle[0]}'] += 1
            if outcomes:
                print(f'  {label}（タグ: オラクルの結果）: ' + ', '.join(
                    f'{tag} {"/".join(f"{k} {v}" for k, v in sorted(c.items()))}' for tag, c in sorted(outcomes.items())))
        hits = collections.Counter()
        analyzer = {'classify': analyze_classify, 'case36': analyze_case36, 'leave': analyze_leave}[kind]
        for item, oracle in items:
            if item['case'] is None:
                continue
            output = json.loads(oracle[1]) if oracle[0] in (0, 4) else None
            analyzer(item['case'], oracle[0], output, hits)
        print('  境界を踏んだ件数（末尾の括弧は数えた単位）:')
        for label in sorted(hits):
            print(f'    {label}: {hits[label]}')


def matches_intent(item, oracle):
    intended, code = item['intended'], oracle[0]
    if intended == 'probe':
        return True
    if intended == 'any':
        return code in (0, 4)
    if intended == 'valid':
        return code == 0
    if intended == 'refused':
        return code == 4
    if intended == 'InvalidInput':
        return code == 2
    return code == 3 and error_code(oracle[2]) == intended


def minute_of(text):
    return int(text[11:13]) * 60 + int(text[14:16])


def analyze_classify(case, exit_code, output, hits):
    if exit_code not in (0, 4):
        return
    policy = case['policy']
    shifts = case['shifts']
    crossing = touching_22 = touching_5 = ends_24 = starts_0 = len1439 = 0
    spans = []
    for s in shifts:
        start = dt.datetime.fromisoformat(s['start'].replace(' ', 'T'))
        end = dt.datetime.fromisoformat(s['end'].replace(' ', 'T'))
        spans.append((start, end))
        last = end - dt.timedelta(minutes=1)
        crossing += last.date() != start.date()
        touching_22 += 22 * 60 in (minute_of(s['start']), minute_of(s['end']))
        touching_5 += 5 * 60 in (minute_of(s['start']), minute_of(s['end']))
        ends_24 += minute_of(s['end']) == 0
        starts_0 += minute_of(s['start']) == 0
        len1439 += (end - start) == dt.timedelta(minutes=1439)
    spans.sort()
    touching_shifts = sum(1 for a, b in zip(spans, spans[1:]) if a[1] == b[0])
    flag(hits, '勤務が暦日をまたぐ（ケース）', crossing)
    flag(hits, '勤務の端が22:00ちょうど（ケース）', touching_22)
    flag(hits, '勤務の端が5:00ちょうど（ケース）', touching_5)
    flag(hits, '勤務が24:00ちょうど（翌0:00）に終わる（ケース）', ends_24)
    flag(hits, '勤務が0:00に始まる（ケース）', starts_0)
    flag(hits, '勤務どうしが接する（ケース）', touching_shifts)
    flag(hits, '1439分の勤務（ケース）', len1439)
    flag(hits, '月の起算日が1以外（ケース）', policy.get('month_start_day') != 1)
    flag(hits, '週44時間（ケース）', policy.get('weekly_limit_minutes') == 2640)
    flag(hits, '空白の区切りの日時（ケース）', any(' ' in s['start'] or ' ' in s['end'] for s in shifts))
    data_from = case.get('data_from')
    flag(hits, 'data_from あり（ケース）', data_from is not None)
    if data_from is not None:
        week_starts = {week_start_of(dt.date.fromisoformat(s['start'][:10]), policy.get('week_start') or 'sun')
                       for s in shifts}
        flag(hits, 'data_from が最初の週の起算日ちょうど（ケース）',
             bool(week_starts) and min(week_starts).isoformat() == data_from)
        flag(hits, 'IncompleteWeek の週の起算日が2000年より前（ケース）',
             any(start < dt.date.fromisoformat(data_from) and start.year < 2000 for start in week_starts))
    if exit_code == 4:
        hits['曖昧さの拒否 refused（ケース）'] += 1
        flag(hits, '曖昧さの拒否のうち IncompleteWeek を含む（ケース）', bool(expected_incomplete_weeks(case)))
        return
    for record in output['ambiguities']:
        hits[f'曖昧さの記録 {record["code"]}（use_default、件）'] += 1
    days = output['days']
    weekday_minutes = [row['total'] - row['holiday'] for row in days]
    for n in (479, 480, 481):
        flag(hits, f'1日の平日型がちょうど{n}分（ケース）', weekday_minutes.count(n))
    flag(hits, '1日の法定休日の労働が480分超（ケース）', sum(1 for row in days if row['holiday'] > 480))
    flag(hits, '日の中で60時間超に入る（0<over60<overtime、ケース）',
         sum(1 for row in days if 0 < row['over60'] < row['overtime']))
    flag(hits, '60時間超の深夜 night_over60>0（ケース）', sum(1 for row in days if row['night_over60'] > 0))
    flag(hits, '所定外法定内 extra_within>0（ケース）', sum(1 for row in days if row['extra_within'] > 0))
    week_start = policy.get('week_start') or 'sun'
    limit = policy.get('weekly_limit_minutes') or 2400
    weeks = collections.defaultdict(lambda: [0, 0])
    for row in days:
        key = week_start_of(dt.date.fromisoformat(row['date']), week_start)
        weeks[key][0] += row['within']
        weeks[key][1] += row['overtime']
    flag(hits, '週の法定内が上限ちょうど（ケース）', sum(1 for w, o in weeks.values() if w == limit))
    flag(hits, '週の法定内が上限ちょうどで、時間外もある（ケース）',
         sum(1 for w, o in weeks.values() if w == limit and o > 0))
    for period in output['periods']:
        overtime = period['overtime']
        if overtime in (3599, 3600, 3601):
            hits[f'月の期間の時間外がちょうど{overtime}分（期間）'] += 1
        if 3500 <= overtime <= 3700:
            hits['月の期間の時間外が3500〜3700分（期間）'] += 1
        if period['over60'] > 0:
            hits['月の期間に60時間超がある（期間）'] += 1
    flag(hits, '同じケースに月の期間が2つ以上（ケース）', len(output['periods']) >= 2)


def flag(hits, label, value):
    if value:
        hits[label] += 1


def analyze_case36(case, exit_code, output, hits):
    if exit_code != 0:
        return
    agreement = case['agreement']
    kind = agreement['kind']
    hits[f'協定の種類 {kind}（ケース）'] += 1
    months = output['months']
    if not months:
        hits['months が空（ケース）'] += 1
        return
    totals = [m['total'] for m in months]
    for t in (5999, 6000, 6001):
        flag(hits, f'月の T がちょうど{t}分（ケース）', totals.count(t))
    near = collections.Counter()
    for m in months:
        for n, total in m['sums'].items():
            if total is not None and abs(total - 4800 * int(n)) <= 1:
                near[total - 4800 * int(n)] += 1
    for delta, label in ((-1, '4800×n−1'), (0, '4800×nちょうど'), (1, '4800×n+1')):
        flag(hits, f'直近n月の合計が{label}（ケース）', near[delta])
    findings = collections.Counter(f['code'] for m in months for f in m['findings'])
    for code, count in findings.items():
        hits[f'findings {code}（件）'] += count
    violations = {c for c in findings if c != 'UnknownHistory'}
    flag(hits, '違反あり（UnknownHistory を除く、ケース）', bool(violations))
    flag(hits, '履歴が足りず Unknown あり（ケース）', findings.get('UnknownHistory', 0))
    overtime = [m['overtime'] for m in case['months']]
    flag(hits, 'O＋H がちょうど44640分（上限、ケース）',
         any(m['overtime'] + m['holiday'] == 44640 for m in case['months']))
    flag(hits, 'history の total がちょうど44640分（上限、ケース）',
         any(h['total'] == 44640 for h in case['history']))
    flag(hits, 'O がちょうど2700分（発動月でない、ケース）', overtime.count(2700))
    flag(hits, 'O がちょうど2701分（発動月、ケース）', overtime.count(2701))
    limit = agreement.get('monthly_limit_minutes')
    if limit is not None and kind != 'none':
        flag(hits, 'O が協定の月限度ちょうど（ケース）', overtime.count(limit))
        flag(hits, 'O が協定の月限度+1（ケース）', overtime.count(limit + 1))
        flag(hits, '協定の月限度が2700未満で、限度超え2700以下の月（ケース）',
             limit < 2700 and any(limit < o <= 2700 for o in overtime))
    years = [m['year_overtime'] for m in months]
    if kind == 'special':
        maximum = agreement['special_months_max']
        counts = [m['special_count'] for m in months]
        flag(hits, '発動月の数が上限ちょうど（ケース）', maximum in counts)
        flag(hits, '発動月の数が上限+1（ケース）', maximum + 1 in counts)
        flag(hits, '回数を超えたあとに発動しない月がある（ケース）', any(
            m['special_count'] > maximum and o <= 2700 for m, o in zip(months, overtime)))
        thresholds = [agreement['special_annual_limit_minutes']]
    elif kind == 'general':
        thresholds = [agreement['annual_limit_minutes'], 21600]
    else:
        thresholds = []
    for threshold in thresholds:
        label = '法定の21600分' if threshold == 21600 else '協定の年限度'
        flag(hits, f'協定年の ΣO が{label}ちょうど（ケース）', threshold in years)
        flag(hits, f'協定年の ΣO が{label}+1（ケース）', threshold + 1 in years)
    flag(hits, 'year_overtime_known が偽の月がある（協定年の途中から、ケース）',
         any(not m['year_overtime_known'] for m in months))
    ysm = agreement['year_start_month']
    flag(hits, '協定年をまたぐ（ケース）', any(
        dt.date.fromisoformat(m['start']).month == ysm for m in case['months'][1:]))


def analyze_leave(case, exit_code, output, hits):
    employee = case.get('employee') if isinstance(case, dict) else None
    if not isinstance(employee, dict):
        return
    try:
        hire = dt.date.fromisoformat(employee['hire_date'])
    except (KeyError, TypeError, ValueError):
        return
    six = next_start(hire, 6)
    if six.day != hire.day:
        hits['初回の付与日に同じ日付がなく翌月1日（ケース）'] += 1
    if exit_code != 0:
        return
    for status in ('Met', 'InProgress', 'NotMet', 'NotApplicable'):
        flag(hits, f'5日義務 {status}（ケース）', sum(1 for f in output['five_day'] if f['status'] == status))
    flag(hits, 'as_of より後の取得の記録（数えない、ケース）', any(r['date'] > case['as_of'] for r in case['taken']))
    grants = employee.get('grants')
    flag(hits, '明示の付与に as_of より後のもの（出さない、ケース）',
         grants is not None and any(g['date'] > case['as_of'] for g in grants))
    flag(hits, '5日の期間の終わりが2月29日（ケース）',
         any(f['window_end'][5:] == '02-29' for f in output['five_day']))
    flag(hits, '5日義務の取得がちょうど10半日（ケース）', sum(1 for f in output['five_day'] if f['taken_halves'] == 10))
    flag(hits, '5日義務の取得が9か11半日（ケース）', sum(1 for f in output['five_day'] if f['taken_halves'] in (9, 11)))
    as_of = dt.date.fromisoformat(case['as_of'])
    ends = {dt.date.fromisoformat(f['window_end']) for f in output['five_day']}
    flag(hits, 'as_of が期間の最後の日（ケース）', as_of in ends)
    flag(hits, 'as_of が期間の最後の日の翌日（ケース）', as_of - dt.timedelta(days=1) in ends)
    expires = {dt.date.fromisoformat(lot['expires']) for lot in output['lots']}
    taken_dates = {dt.date.fromisoformat(r['date']) for r in case['taken']}
    flag(hits, '失効日の前日に取得（ケース）', any(e - dt.timedelta(days=1) in taken_dates for e in expires))
    flag(hits, '失効日の当日に取得（別のロットから消化、ケース）', bool(expires & taken_dates))
    flag(hits, 'as_of が失効日の当日（ケース）', as_of in expires)
    flag(hits, '失効した未使用分 expired_halves>0（ケース）', any(lot['expired_halves'] > 0 for lot in output['lots']))
    flag(hits, '2月29日の付与（ケース）', any(lot['grant_date'][5:] == '02-29' for lot in output['lots']))
    flag(hits, 'as_of の当日の付与（ケース）', any(lot['grant_date'] == case['as_of'] for lot in output['lots']))
    flag(hits, '注意 AttendanceUnconfirmed（ケース）', 'AttendanceUnconfirmed' in output['notices'])
    flag(hits, '出勤率 below を含む（ケース）', 'below' in employee.get('attendance', []))
    flag(hits, '前倒しの付与を明示（ケース）', employee.get('grants') is not None)
    flag(hits, '比例付与（10日未満の付与あり、ケース）', any(lot['granted_halves'] < 20 for lot in output['lots']))
    flag(hits, '半日の取得（ケース）', any(r['unit'] == 'half' for r in case['taken']))
    flag(hits, '使用が奇数半日のロットがある（半日ずつの消化、ケース）',
         any((lot['used_halves'] % 2 == 1) for lot in output['lots']))


if __name__ == '__main__':
    sys.exit(main())
