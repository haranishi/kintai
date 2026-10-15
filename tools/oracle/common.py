# SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""別実装（オラクル）の共通部品。標準ライブラリだけを使う。

本実装（Rust）のコードは一切見ずに、次の仕様だけから書いている。
- 日付・時点の表記と範囲: docs/spec/core.md
- エラーの形と終了コード: docs/spec/case-format.md「エラーの形」「終了コード」
- 検査の順序: docs/spec/validation.md
"""
import datetime as dt
import json
import re
import sys

EXIT_OK = 0
EXIT_INVALID = 2
EXIT_UNSUPPORTED = 3
EXIT_REFUSED = 4

# core.md「日付」: 2000-01-01〜2099-12-31だけを受け付ける。
MIN_DATE = dt.date(2000, 1, 1)
MAX_DATE = dt.date(2099, 12, 31)

# core.md「時点」: 1970-01-01 00:00（日本時間の壁時計）からの整数の分。
EPOCH_ORDINAL = dt.date(1970, 1, 1).toordinal()
MINUTES_PER_DAY = 1440

# Python の date.weekday() の順（月曜=0）。
WEEKDAY_NAMES = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun']


class CaseError(Exception):
    """ケースを、標準エラーのエラー（終了コード2か3）で終える。"""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message

    @property
    def exit_code(self):
        return EXIT_INVALID if self.code == 'InvalidInput' else EXIT_UNSUPPORTED


def invalid(message):
    raise CaseError('InvalidInput', message)


def unsupported(code, message):
    raise CaseError(code, message)


# ---------------------------------------------------------------- 型の検査

def is_int(value):
    # JSON の true/false は Python で bool（int の子）になるので、type で厳密に比べる。
    return type(value) is int


def is_str(value):
    return type(value) is str


def is_bool(value):
    return type(value) is bool


def check_object(value, where, allowed, required=()):
    """オブジェクトであること、知らない項目がないこと、必須の項目があること。"""
    if type(value) is not dict:
        invalid(f'{where}: object expected')
    for key in value:
        if key not in allowed:
            invalid(f'{where}: unknown field "{key}"')
    for key in required:
        if key not in value:
            invalid(f'{where}: missing field "{key}"')
    return value


def check_list(value, where):
    if type(value) is not list:
        invalid(f'{where}: array expected')
    return value


def check_type(value, predicate, where, nullable=False):
    """型だけを見る（validation.md 手順1）。nullable なら null を許す。"""
    if value is None and nullable:
        return
    if not predicate(value):
        invalid(f'{where}: wrong type')


# ---------------------------------------------------------------- 日付と時点

_DATE_RE = re.compile(r'([0-9]{4})-([0-9]{2})-([0-9]{2})')
# 区切りは `T` か空白（core.md・case-format.md。第1版の疑問Q2は第2版で決着）。
_INSTANT_RE = re.compile(r'([0-9]{4})-([0-9]{2})-([0-9]{2})[T ]([0-9]{2}):([0-9]{2})')


def parse_date(text, where):
    """`YYYY-MM-DD` を date にする。表記・実在・範囲のどれかが駄目なら InvalidInput。"""
    match = _DATE_RE.fullmatch(text) if is_str(text) else None
    if match is None:
        invalid(f'{where}: date must be YYYY-MM-DD')
    try:
        value = dt.date(int(match[1]), int(match[2]), int(match[3]))
    except ValueError:
        invalid(f'{where}: no such date {text}')
    if not MIN_DATE <= value <= MAX_DATE:
        invalid(f'{where}: date outside 2000-01-01..2099-12-31')
    return value


def parse_instant(text, where):
    """`YYYY-MM-DDTHH:MM` を、1970-01-01 00:00 からの整数の分にする。"""
    match = _INSTANT_RE.fullmatch(text) if is_str(text) else None
    if match is None:
        invalid(f'{where}: datetime must be YYYY-MM-DDTHH:MM')
    hour, minute = int(match[4]), int(match[5])
    if hour == 24:
        # core.md「24:00は受け付けない（翌日の0:00と書く）」、validation.md 手順4「24:00の表記」。
        invalid(f'{where}: 24:00 is not accepted; write 00:00 of the next day')
    if hour > 23 or minute > 59:
        invalid(f'{where}: no such time {text}')
    # 2099-12-31 の24:00に終わる勤務は 2100-01-01T00:00 と書くしかなく、範囲外で不正な入力
    # （core.md・validation.md 手順4。第1版の疑問Q3は第2版で決着）。
    day = parse_date(f'{match[1]}-{match[2]}-{match[3]}', where)
    return day_number(day) * MINUTES_PER_DAY + hour * 60 + minute


def day_number(date):
    """1970-01-01 を0とする日の番号。"""
    return date.toordinal() - EPOCH_ORDINAL


def date_of(day_no):
    return dt.date.fromordinal(day_no + EPOCH_ORDINAL)


def weekday_name(date):
    return WEEKDAY_NAMES[date.weekday()]


def weekday_name_of_day(day_no):
    # 1970-01-01 は木曜（月曜=0 の数え方で3）。
    return WEEKDAY_NAMES[(day_no + 3) % 7]


def week_start_of(date, start_name):
    """週の起算日の曜日から数えて、date を含む週の初日。"""
    offset = (date.weekday() - WEEKDAY_NAMES.index(start_name)) % 7
    return date - dt.timedelta(days=offset)


def period_start_of(date, start_day):
    """月の起算日（1〜28）から1か月の期間のうち、date を含む期間の初日。"""
    if date.day >= start_day:
        return date.replace(day=start_day)
    year, month = (date.year, date.month - 1) if date.month > 1 else (date.year - 1, 12)
    return dt.date(year, month, start_day)


def shift_month(year, month, n):
    index = year * 12 + (month - 1) + n
    return index // 12, index % 12 + 1


def add_months_same_day(date, n):
    """日が1〜28の日付に n か月を足す（どの月にもその日がある）。"""
    year, month = shift_month(date.year, date.month, n)
    return dt.date(year, month, date.day)


def period_end(start, n):
    """期間の終わり(S, n)（leave.md「日付の規則」、民法143条2項）。

    S の n か月後に同じ日付があれば、その前日。なければ、その月の末日。
    """
    year, month = shift_month(start.year, start.month, n)
    try:
        return dt.date(year, month, start.day) - dt.timedelta(days=1)
    except ValueError:
        next_year, next_month = shift_month(year, month, 1)
        return dt.date(next_year, next_month, 1) - dt.timedelta(days=1)


def next_start(start, n):
    """次の初日(S, n) = 期間の終わり(S, n) の翌日（leave.md「日付の規則」）。"""
    return period_end(start, n) + dt.timedelta(days=1)


def parse_date_or_none(text):
    """日付の書式と実在だけを見る（範囲は見ない）。だめなら None。

    対応外の判定（手順3）が日付を使うときのためのもの。書式の誤りそのものは手順4で拒否する。
    """
    match = _DATE_RE.fullmatch(text) if is_str(text) else None
    if match is None:
        return None
    try:
        return dt.date(int(match[1]), int(match[2]), int(match[3]))
    except ValueError:
        return None


# ---------------------------------------------------------------- 入出力

# validation.md 手順1: RFC 8259 の JSON だけを受け付ける。NaN・Infinity、孤立したサロゲート、
# 128段目に入る配列・オブジェクト（最も外側の値を1段目と数え、127段まで読める）、倍精度に収まらない数、
# 同じ名前の項目の2回は、読めない扱いで InvalidInput（決着（旧Q43・Q44））。
MAX_NESTING = 127


def _reject_constant(name):
    raise ValueError(f'non-standard JSON constant {name}')


def _parse_int(text):
    # -0 は読める。小数の -0.0 として読み、整数の欄の型の検査（is_int）で拒否する。だから
    # 中身を検査しない所（holiday_swaps の要素など）の -0 は読める（validation.md 手順1、決着（旧Q44））。
    if text == '-0':
        return -0.0
    return int(text)


def _parse_float(text):
    # 1e400 のように倍精度に収まらない数は、どこにあっても読めない扱い（決着（旧Q44））。
    value = float(text)
    if value != value or value in (float('inf'), float('-inf')):
        raise ValueError(f'number out of range: {text}')
    return value


def _object_without_duplicates(pairs):
    # 同じ名前の項目が2回あるJSONは不正な入力（validation.md 手順1。第1版の疑問Q30）。
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate field "{key}"')
        result[key] = value
    return result


def _check_strings_and_nesting(value):
    """読んだ後に、孤立したサロゲートと入れ子の深さを見る（validation.md 手順1）。

    最も外側の値を1段目と数え、128段目に配列かオブジェクトがあれば読めない（127段まで読める）。
    """
    def check_text(text):
        for char in text:
            if '\ud800' <= char <= '\udfff':
                raise ValueError('lone surrogate in a string')

    stack = [(value, 1)]
    while stack:
        item, level = stack.pop()
        if isinstance(item, (dict, list)) and level > MAX_NESTING:
            raise ValueError(f'nesting deeper than {MAX_NESTING}')
        if isinstance(item, dict):
            for key, child in item.items():
                check_text(key)
                stack.append((child, level + 1))
        elif isinstance(item, list):
            for child in item:
                stack.append((child, level + 1))
        elif isinstance(item, str):
            check_text(item)


def load_json(text):
    value = json.loads(text, parse_constant=_reject_constant, parse_int=_parse_int,
                       parse_float=_parse_float, object_pairs_hook=_object_without_duplicates)
    _check_strings_and_nesting(value)
    return value


def dumps(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'))


def run_bytes(handler, data):
    """CLI と同じ結果を返す: (終了コード, 標準出力の文字列, 標準エラーの文字列)。

    handler の中のプログラムの誤り（CaseError 以外の例外）は、握りつぶさずに外へ出す。
    """
    try:
        case = load_json(data.decode('utf-8'))
    except (UnicodeDecodeError, ValueError, RecursionError) as exc:
        return _error_result(CaseError('InvalidInput', f'not readable as JSON: {exc}'))
    try:
        output = handler(case)
    except CaseError as exc:
        return _error_result(exc)
    exit_code = EXIT_REFUSED if output.get('refused') else EXIT_OK
    return exit_code, dumps(output) + '\n', ''


def _error_result(exc):
    body = {'error': {'code': exc.code, 'message': exc.message}}
    return exc.exit_code, '', dumps(body) + '\n'


def cli(handler, argv=None):
    import argparse
    parser = argparse.ArgumentParser(description=handler.__doc__)
    parser.add_argument('file', help='ケースのJSONファイル')
    args = parser.parse_args(argv)
    try:
        with open(args.file, 'rb') as stream:
            data = stream.read()
    except OSError as exc:
        exit_code, out, err = _error_result(CaseError('InvalidInput', f'cannot read: {exc}'))
    else:
        exit_code, out, err = run_bytes(handler, data)
    sys.stdout.write(out)
    sys.stderr.write(err)
    return exit_code
