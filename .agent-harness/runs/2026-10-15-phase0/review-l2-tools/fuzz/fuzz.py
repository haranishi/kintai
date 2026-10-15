"""L2 査読: 壊した入力でパニックしないか（終了コードが決まりの範囲か）を確かめる簡単なファジング。

使い方: python3 -I fuzz.py WS BIN SEED N
"""
import json
import os
import random
import subprocess
import sys

WS, BIN, SEED, N = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
FIX = os.path.join(WS, 'tests', 'fixtures', 'synthetic')
WORK = os.path.join(WS, '.tmp', 'fuzz', 'work')
os.makedirs(WORK, exist_ok=True)
rng = random.Random(SEED)

seeds = {
    'policy': open(os.path.join(FIX, 'policy.toml'), 'rb').read(),
    'punches': open(os.path.join(FIX, 'punches.csv'), 'rb').read(),
    'leaves': open(os.path.join(FIX, 'leaves.csv'), 'rb').read(),
    'history': open(os.path.join(FIX, 'history.csv'), 'rb').read(),
}
cases = []
for name in ('official-examples.json', 'limits36-official.json', 'leave-official.json'):
    d = json.load(open(os.path.join(WS, 'docs', 'spec', 'vectors', name), encoding='utf-8'))
    for v in d['vectors']:
        if 'case' in v:
            cases.append(json.dumps(v['case'], ensure_ascii=False).encode())
INTERESTING = [b'\x00', b'\xff', b'\r', b'\n', b'"', b',', b'\xef\xbb\xbf', b'=', b'[', b']', b'{', b'}',
               b'99999999999999999999', b'-1', b'0', b'2099-12-31 23:59:59', b'2100-01-01', b'24:00', b'\\ud800',
               b'1e400', b'-0', b'null', b'true', b'nan', b'\x82\xa0', b'2026-02-29', b'\t', b' ']


def mutate(data):
    data = bytearray(data)
    for _ in range(rng.randint(1, 6)):
        op = rng.randrange(7)
        if not data:
            data += rng.choice(INTERESTING)
            continue
        i = rng.randrange(len(data))
        if op == 0:
            data[i] = rng.randrange(256)
        elif op == 1:
            data[i:i] = rng.choice(INTERESTING)
        elif op == 2:
            j = min(len(data), i + rng.randint(1, 40))
            del data[i:j]
        elif op == 3:
            j = min(len(data), i + rng.randint(1, 200))
            data[i:i] = data[i:j]
        elif op == 4:
            lines = bytes(data).split(b'\n')
            k = rng.randrange(len(lines))
            lines.insert(k, lines[rng.randrange(len(lines))])
            data = bytearray(b'\n'.join(lines))
        elif op == 5:
            # 数字を別の数字に
            digits = [p for p, b in enumerate(data) if 48 <= b <= 57]
            if digits:
                data[rng.choice(digits)] = rng.randrange(48, 58)
        else:
            data = bytearray(bytes(data).replace(b'\n', b'\r\n', 1) if rng.random() < 0.5 else bytes(data).replace(b'\n', b'\r', 1))
    return bytes(data)


def write(name, data):
    path = os.path.join(WORK, name)
    open(path, 'wb').write(data)
    return path


bad = 0
counts = {}
for it in range(N):
    target = rng.choice(['policy', 'punches', 'leaves', 'history', 'case', 'case', 'policy', 'punches'])
    files = {k: v for k, v in seeds.items()}
    if target in files:
        files[target] = mutate(files[target])
    paths = {k: write(k, v) for k, v in files.items()}
    cmd_kind = rng.choice(['report', 'check', 'leave', 'explain']) if target != 'case' else 'case'
    if target == 'leaves':
        cmd_kind = 'leave'
    if target == 'history':
        cmd_kind = 'check'
    if cmd_kind == 'report':
        cmd = [BIN, 'report', '--policy', paths['policy'], '--punches', paths['punches'], '--month', '2026-10',
               '--format', rng.choice(['json', 'csv', 'text'])]
    elif cmd_kind == 'check':
        cmd = [BIN, 'check', '--policy', paths['policy'], '--punches', paths['punches'], '--month', '2026-10',
               '--history', paths['history']] + (['--fail-on-violation'] if rng.random() < 0.5 else [])
    elif cmd_kind == 'leave':
        cmd = [BIN, 'leave', '--policy', paths['policy'], '--leaves', paths['leaves'], '--as-of',
               rng.choice(['2026-10-31', '2030-01-01', '2099-12-31', '2000-01-01']), '--format', rng.choice(['json', 'text'])]
    elif cmd_kind == 'explain':
        cmd = [BIN, 'explain', '--policy', paths['policy'], '--punches', paths['punches'], '--employee',
               rng.choice(['E001', 'E002', 'E003', 'E004', 'E005']), '--date', rng.choice(['2026-10-08', '2026-10-10', '2026-10-12']),
               '--format', rng.choice(['json', 'text'])]
    else:
        cpath = write('case.json', mutate(rng.choice(cases)))
        cmd = [BIN, 'case', cpath]
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=30)
    except subprocess.TimeoutExpired:
        bad += 1
        print('TIMEOUT', it, cmd_kind, target)
        continue
    counts[(cmd_kind, r.returncode)] = counts.get((cmd_kind, r.returncode), 0) + 1
    allowed = {0, 2, 3, 4} | ({1} if cmd_kind == 'check' else set())
    stderr = r.stderr.decode('utf-8', 'replace')
    ok = r.returncode in allowed and 'panicked' not in stderr
    if ok and r.returncode in (2, 3):
        try:
            err = json.loads(stderr)
            ok = 'code' in err['error'] and 'message' in err['error']
        except Exception:
            ok = False
    if ok and r.returncode in (0, 1, 4) and stderr:
        ok = False
    if not ok:
        bad += 1
        if bad <= 10:
            print('BAD', it, cmd_kind, target, r.returncode, stderr[:300])
            write(f'bad-{it}-{target}', files.get(target, b'') if target != 'case' else open(os.path.join(WORK, 'case.json'), 'rb').read())
print('iterations', N, 'bad', bad)
print(sorted(counts.items()))
