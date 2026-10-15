#!/usr/bin/env python3
"""調査用検算。祝日CSVから日付生成せず、法規則・特措法例外から独立生成。
近似式は法的根拠ではなく検算仮定。正式春分・秋分は取得した暦要項p1と照合。
入力はこのディレクトリの原CSVと隣のdownloads/naoYYYY.txt。外部通信なし。
出典：内閣府 https://www8.cao.go.jp/chosei/shukujitsu/gaiyou.html
暦要項 https://eco.mtk.nao.ac.jp/koyomi/yoko/pdf/yokoYYYY.pdf
"""
from pathlib import Path
from datetime import date, timedelta
from collections import Counter
import csv
import hashlib
import io
import json
import math
import re

ROOT = Path(__file__).resolve().parent
DOWNLOADS = ROOT.parent / 'downloads'
DAY = timedelta(days=1)


def equinox(year, month):
    if not 1980 <= year <= 2099:
        raise ValueError('近似式の検算範囲外')
    base = 20.8431 if month == 3 else 23.2488
    d = math.floor(base + 0.242194 * (year - 1980)) - math.floor((year - 1980) / 4)
    return date(year, month, d)


def monday(year, month, nth):
    first = date(year, month, 1)
    return first + timedelta(days=(0 - first.weekday()) % 7 + 7 * (nth - 1))


def generate(year):
    fixed = [(1, 1, '元日'), (2, 11, '建国記念の日'), (2, 23, '天皇誕生日'),
             (4, 29, '昭和の日'), (5, 3, '憲法記念日'), (5, 4, 'みどりの日'),
             (5, 5, 'こどもの日'), (8, 11, '山の日'), (11, 3, '文化の日'),
             (11, 23, '勤労感謝の日')]
    primary = {date(year, m, d): n for m, d, n in fixed}
    for m, nth, name in [(1, 2, '成人の日'), (7, 3, '海の日'),
                         (9, 3, '敬老の日'), (10, 2, 'スポーツの日')]:
        primary[monday(year, m, nth)] = name
    primary[equinox(year, 3)] = '春分の日'
    primary[equinox(year, 9)] = '秋分の日'
    # 東京五輪特措法第32条1・2項による、2020/2021年に限定した移動。
    moved = {2020: {'海の日': (7, 23), 'スポーツの日': (7, 24), '山の日': (8, 10)},
             2021: {'海の日': (7, 22), 'スポーツの日': (7, 23), '山の日': (8, 8)}}
    for name, (m, d) in moved.get(year, {}).items():
        old = next(dt for dt, n in primary.items() if n == name)
        del primary[old]
        primary[date(year, m, d)] = name
    national = set(primary)
    result = dict(primary)
    kinds = {d: '国民の祝日（2条／特措法）' for d in national}
    # 3条3項：前日と翌日が国民の祝日である「国民の祝日でない日」。
    for d in sorted(national):
        middle = d + DAY
        if middle not in national and middle + DAY in national:
            result[middle] = '休日'
            kinds[middle] = '3条3項'
    # 3条2項：日曜祝日の後の最初の「国民の祝日でない日」。
    # 派生休日を国民の祝日に昇格させない。土曜に振替する規則はない。
    for d in sorted(national):
        if d.weekday() == 6:
            substitute = d + DAY
            while substitute in national:
                substitute += DAY
            result[substitute] = '休日'
            kinds[substitute] = '3条2項' if substitute not in kinds else kinds[substitute] + '・3条2項'
    return result, kinds


def official_equinoxes(year):
    p = DOWNLOADS / f'nao{year}.txt'
    if not p.is_file():
        raise FileNotFoundError(f'公表済年の取得資料がない: {p.name}')
    # 国民の祝日表は各PDFの第1ページ。節気時刻表の行とは区別。
    first_page = p.read_text(encoding='utf-8').split('\f')[0]
    dates = {}
    for month, pattern in [(3, r'春\s*分\s*の\s*日\s+3\s+(\d{1,2})'),
                           (9, r'秋\s*分\s*の\s*日\s+9\s+(\d{1,2})')]:
        matches = re.findall(pattern, first_page)
        if len(matches) != 1:
            raise ValueError(f'暦要項{year}抽出不確実: month={month}, matches={matches}')
        dates[month] = date(year, month, int(matches[0]))
    return dates


def main():
    raw = (ROOT / 'syukujitsu_raw.csv').read_bytes()
    # 宣言charsetはHTTPにない。Shift_JIS厳密復号が成功することを検証。
    assert raw.decode('shift_jis') == raw.decode('cp932'), '復号結果が異なる'
    rows = list(csv.reader(io.StringIO(raw.decode('shift_jis'))))
    assert rows[0] == ['国民の祝日・休日月日', '国民の祝日・休日名称'], rows[0]
    actual = {}
    ordered = []
    for row in rows[1:]:
        assert len(row) == 2 and row[1], row
        y, m, d = map(int, row[0].split('/'))
        dt = date(y, m, d)
        assert dt not in actual, f'重複: {dt}'
        actual[dt] = row[1]
        ordered.append(dt)
    assert ordered == sorted(ordered), '原CSVの日付順が不正'
    with (ROOT / 'syukujitsu_utf8.csv').open('w', encoding='utf-8', newline='') as f:
        writer = csv.writer(f, lineterminator='\n')
        writer.writerow(['日付', '名称'])
        writer.writerows((d.isoformat(), actual[d]) for d in ordered)
    max_year = max(d.year for d in actual)
    # 未確認年まで、無断で検算範囲を拡張しない。更新時は公表資料も取得する。
    assert max_year == 2027, 'CSV収録年更新：暦要項・例外法・規約の再調査が必要'
    years = []
    diffs = []
    equinox_rows = []
    with (ROOT / 'holidays_generated_2020_2027.csv').open('w', encoding='utf-8', newline='') as f:
        writer = csv.writer(f, lineterminator='\n')
        writer.writerow(['日付', '名称', '法的区分'])
        for year in range(2020, max_year + 1):
            generated, kinds = generate(year)
            observed = {d: n for d, n in actual.items() if d.year == year}
            differences = [{'date': d.isoformat(), 'generated': generated.get(d), 'csv': observed.get(d)}
                           for d in sorted(set(generated) | set(observed))
                           if generated.get(d) != observed.get(d)]
            diffs.extend(differences)
            counter = Counter(kinds.values())
            years.append({'year': year, 'generated_count': len(generated), 'csv_count': len(observed),
                          'difference_count': len(differences), 'categories': dict(counter)})
            writer.writerows((d.isoformat(), generated[d], kinds[d]) for d in sorted(generated))
            for month, official in official_equinoxes(year).items():
                approximation = equinox(year, month)
                equinox_rows.append({'year': year, 'name': '春分の日' if month == 3 else '秋分の日',
                                     'official': official.isoformat(), 'approximation': approximation.isoformat(),
                                     'match': official == approximation,
                                     'source': f'https://eco.mtk.nao.ac.jp/koyomi/yoko/pdf/yoko{year}.pdf', 'page': 1})
    with (ROOT / 'equinox_verified.csv').open('w', encoding='utf-8', newline='') as f:
        writer = csv.DictWriter(f, fieldnames=list(equinox_rows[0]), lineterminator='\n')
        writer.writeheader()
        writer.writerows(equinox_rows)
    report = {'raw_encoding': 'Shift_JISで厳密復号成功（CP932も同じ文字列に復号）',
              'raw_sha256': hashlib.sha256(raw).hexdigest(), 'raw_bytes': len(raw),
              'raw_data_rows': len(actual), 'first_date': min(actual).isoformat(),
              'last_date': max(actual).isoformat(), 'years_checked': years,
              'checked_holiday_rows': sum(y['csv_count'] for y in years),
              'holiday_differences': diffs, 'official_equinoxes_checked': len(equinox_rows),
              'equinox_mismatches': [r for r in equinox_rows if not r['match']],
              'unverified_future_years': [2028, 2029, 2030]}
    (ROOT / 'verification_result.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(report, ensure_ascii=False, indent=2))
    assert not diffs, '祝日生成とCSVが不一致'
    assert all(r['match'] for r in equinox_rows), '近似式と公式暦要項が不一致'


if __name__ == '__main__':
    main()
