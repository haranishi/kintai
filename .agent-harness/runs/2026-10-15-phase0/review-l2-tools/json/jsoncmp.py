import subprocess, sys, json, os
WS = sys.argv[1]
sys.path.insert(0, os.path.join(WS, "tools", "oracle"))
sys.dont_write_bytecode = True
from common import run_bytes
from classify_oracle import classify
K = os.path.join(WS, ".cargo-target", "release", "kintai")
base = '{"schema":"kintai.case.v1","policy":{"work_system":"standard","scheduled_minutes_per_day":%s,"legal_holiday_weekday":"sun","month_start_day":1%s},"shifts":[%s]}'
shift = '{"start":"2026-10-05T09:00","end":"2026-10-05T18:00"}'
cases = {
 'bom': b'\xef\xbb\xbf' + (base % ('420','',shift)).encode(),
 'minus0': (base % ('-0','',shift)).encode(),
 'E2': (base % ('4E2','',shift)).encode(),
 'nbsp_ws': (base % ('420','',shift)).replace('{"schema"','{ "schema"').encode(),
 'dup_escaped': (base % ('420',',"month_start_day":1',shift)).replace(',"month_start_day":1}', ',"month_start\\u005fday":1}').encode(),
 'lone_surrogate_key': (base % ('420',',"\\ud800":1',shift)).encode(),
 'lone_surrogate_swaps': (base % ('420',',"holiday_swaps":["\\udc00"]',shift)).encode(),
 'pair_surrogate_swaps': (base % ('420',',"holiday_swaps":["\\ud83d\\ude00"]',shift)).encode(),
 'swaps_minus0': (base % ('420',',"holiday_swaps":[-0]',shift)).encode(),
 'swaps_1e400': (base % ('420',',"holiday_swaps":[1e400]',shift)).encode(),
 'swaps_1e308': (base % ('420',',"holiday_swaps":[1e308]',shift)).encode(),
 'swaps_tiny': (base % ('420',',"holiday_swaps":[1e-400]',shift)).encode(),
 'big_int': (base % ('99999999999999999999','',shift)).encode(),
 'neg_big_int': (base % ('-9223372036854775809','',shift)).encode(),
 'i64min': (base % ('-9223372036854775808','',shift)).encode(),
 'trailing_comma': (base % ('420','',shift)).replace(']}',',]}').encode(),
 'nul_in_string': (base % ('420','',shift.replace('09:00"','09:00\\u0000"'))).encode(),
 'invalid_utf8': (base % ('420','',shift)).encode().replace(b'standard', b'stand\xffrd'),
 'utf16': (base % ('420','',shift)).encode('utf-16'),
 'top_array': b'[]',
 'top_null': b'null',
 'empty': b'',
 'ws_only': b'  \n',
 'data_from_null': (base % ('420','',shift)).replace('"policy"','"data_from":null,"policy"').encode(),
 'data_from_bad': (base % ('420','',shift)).replace('"policy"','"data_from":"2026-02-30","policy"').encode(),
 'data_from_1999': (base % ('420','',shift)).replace('"policy"','"data_from":"1999-12-31","policy"').encode(),
 'legal_null': (base % ('420','',shift)).replace('"legal_holiday_weekday":"sun"','"legal_holiday_weekday":null').encode(),
 'shift_24': (base % ('420','',shift.replace('18:00"','24:00"'))).encode(),
 'shift_space_sep': (base % ('420','',shift.replace('T09:00',' 09:00'))).encode(),
 'shift_2100': (base % ('420','',shift.replace('2026-10-05T18:00','2100-01-01T00:00'))).encode(),
 'swaps_deep_127': (base % ('420',',"holiday_swaps":' + '['*125 + ']'*125, shift)).encode(),
 'swaps_deep_128': (base % ('420',',"holiday_swaps":' + '['*126 + ']'*126, shift)).encode(),
 'swaps_obj_deep_127': (base % ('420',',"holiday_swaps":[' + '{"a":'*124 + '1' + '}'*124 + ']', shift)).encode(),
 'swaps_obj_deep_128': (base % ('420',',"holiday_swaps":[' + '{"a":'*125 + '1' + '}'*125 + ']', shift)).encode(),
}
for name, data in cases.items():
    open('c.json','wb').write(data)
    r = subprocess.run([K,'case','c.json'],capture_output=True,text=True)
    o = run_bytes(classify, data)
    rc = json.loads(r.stderr)['error']['code'] if r.returncode in (2,3) else ''
    oc = json.loads(o[2])['error']['code'] if o[0] in (2,3) else ''
    same = (r.returncode == o[0] and rc == oc and (r.returncode not in (0,4) or json.loads(r.stdout) == json.loads(o[1])))
    print(f'{name:22s} bin={r.returncode}{"/"+rc if rc else ""} oracle={o[0]}{"/"+oc if oc else ""} {"OK" if same else "DIFF"}')
