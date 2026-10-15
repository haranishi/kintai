# 査読 L3（独立・権利と規制の文書の事実）

査読日 2026-10-15。読み取り専用で行い、書いたのはこの報告書と作業用の一時フォルダだけ。`crates/` は各 `Cargo.toml` を読んだだけで、ほかにルートの `Cargo.lock` を読んだ。一次資料はすべて自分で取得し、取得日はすべて 2026-10-15（日本時間）。法律の判断はしていない。

## 要約: 81か所を確かめ、誤り🔴2件・不正確🟡5件・軽微⚪7件

法令の条文、改正の版、通達とQ&Aの番号は、一次資料とほぼ一致した。誤りは、外部の規約と数字の取り違えに集中している。

- **いちばん重いのは L3-01（🔴）。** `license-policy.md` と `REUSE.toml` は、国立天文台の春分・秋分の日付も公共データ利用規約（第1.0版）に従う CC BY 4.0 として扱う。実際の条件は国立天文台の独自の規程で、公共データ利用規約にも CC BY にも触れていない。公開するファイルのライセンス表記に直結する。
- もう一つの🔴は L3-02。表の Sentrifugo の数字（スター8、2017-10-30）はフォークのもので、本家はスター544。「どれもコピーレフト」という結論は変わらない。
- 公開の前に手当てが要るのは L3-06（🟡）。調査のログ4本は公的資料の抽出文を長く含むのに、`.gitignore` の対象外で、外す一覧にもない。
- 査読の途中（06:10）に、実装の担当が `kintai-import` から `csv` を外した。依存の一覧（L3-14）は公開の直前に作り直す必要がある。
- 次の一手: L3-01 と L3-06 を公開前に直す。L3-02〜L3-05 は記録の書き直しで足りる。⚪は次に触るときでよい。

## 1. 確認した範囲

| 文書 | 確かめた文・行 | 指摘 |
|---|---:|---|
| `docs/legal/rights-check.md` | 26 | L3-02, 04, 05, 06, 09, 14 |
| `docs/legal/regulation-notes.md` | 16 | L3-03, 04, 07, 08 |
| `docs/legal/professional-review.md`（根拠の欄） | 20 | なし（補足1件） |
| `docs/legal/license-policy.md` | 7 | L3-01, 06, 11, 12 |
| `DISCLAIMER.md` | 2 | L3-13 |
| `README.md` | 6 | L3-10 |
| `docs/faq.md`（と `limits36.md` の Q&A 2-32） | 4 | なし |

取得した一次資料（すべて 2026-10-15 取得）:

- GitHub（`gh api`。`haranishi/kintai` だけ認証なしの curl）: `repos/frappe/hrms` `repos/kimai/kimai` `repos/orangehrm/orangehrm` `repos/sapplica/sentrifugo`（と `/commits`） `repos/Sentrifugo/sentrifugo` `repos/kishida-devil/jp-payroll-mcp`、https://api.github.com/repos/haranishi/kintai 、`search/repositories`（`kintai in:name`、`勤怠 language:Rust` とその変形、`sentrifugo in:name`）
- crates.io: https://crates.io/api/v1/crates/kintai と、同じ形の `kintai-core` `kintai-jp-labor` `kintai-import` `kintai-cli` `kintai-app`
- e-Gov 法令API v2: `https://laws.e-gov.go.jp/api/2/law_data/<ID>` と `/law_revisions/<ID>`。ID は 322AC0000000049（版 20220617・20230401・20260624・20260717・20270401・20281223 も）、322M40000100023、406CO0000000005、347AC0000000057、347M50002000032、345AC0000000048、343AC1000000089、323AC1000000178、129AC0000000089、415AC0000000057
- 厚生労働省（https://www.mhlw.go.jp/ 配下）: `chosakuken/index.html`、`stf/newpage_48220.html` と `content/11402000/001370269.pdf`、`stf/newpage_76683.html` と `content/11201250/001758760.pdf`、`web/t_doc?dataId=00tb1899&dataType=1&pageNo=1`、`content/11200000/tp1216-1j.pdf`、`content/000487097.pdf`、`content/001140963.pdf`、`stf/topics/bukyoku/soumu/houritu/221.html`・`222.html`
- 労働局ほか: https://jsite.mhlw.go.jp/aichi-roudoukyoku/var/rev0/0119/6636/hasuutoriatukai.pdf 、https://jsite.mhlw.go.jp/nagasaki-roudoukyoku/content/contents/no28-25071801.pdf 、https://www.check-roudou.mhlw.go.jp/ と `/study/roudousya_jikangai.html`、https://www.pref.osaka.lg.jp/documents/6136/028.pdf
- 規約・データ: https://biz.moneyforward.com/agreement/ 、https://www.nao.ac.jp/terms/ ・`/terms/copyright.html`・`/faq/a0301.html`、https://eco.mtk.nao.ac.jp/koyomi/yoko/ ・`/koyomi/site/`、https://www.cao.go.jp/notice/rule.html 、https://www8.cao.go.jp/chosei/shukujitsu/gaiyou.html 、https://www.digital.go.jp/resources/open_data/public_data_license_v1.0 、https://opensource.org/api/licenses 、https://developercertificate.org/

## 2. 指摘

| ID | 重さ | 文書と行 | 書かれていること | 実際（引用・取得URL・取得日） | 直し方の提案 |
|---|---|---|---|---|---|
| L3-01 | 🔴 | `license-policy.md` 12行、`REUSE.toml` 23〜26行 | 内閣府の祝日CSVと国立天文台の春分・秋分の日付を一つの行にまとめ、「公共データ利用規約（第1.0版）に従い…CC BY 4.0と互換」。REUSE.toml は `equinox-*.csv` を祝日CSVと同じ注記に入れ、著作権表示を「Cabinet Office, Government of Japan（…National Astronomical Observatory of Japan for equinox dates）」、ライセンスを CC-BY-4.0 にしている | 予想表（www.nao.ac.jp）の条件は国立天文台ウェブサイト利用規程の「著作物利用について」。「ウェブサイト、ソーシャルネットワーク、アプリにおける情報発信」は事前の許諾なく使えるが、「クレジットの表示がない利用は認められません」。「プログラム、ソースコードを利用する場合」は事前の許諾が要る（https://www.nao.ac.jp/terms/copyright.html ）。公式の日付の出典の暦要項（eco.mtk.nao.ac.jp）は暦計算室のサイト利用規定で、「各ページの内容は、ご自由に利用ください。ご利用にあたり、届出の必要はございません。」（https://eco.mtk.nao.ac.jp/koyomi/site/ ）。どちらも公共データ利用規約と CC BY に触れていない。公共データ利用規約を採るのは内閣府（https://www.cao.go.jp/notice/rule.html ）。2026-10-15取得 | 行を分ける。内閣府のCSVは公共データ利用規約、国立天文台の日付は上の二つの規程で、クレジットは「国立天文台」。REUSE.toml の `equinox-*.csv` を別の注記にし、著作権表示から内閣府を外す。SPDXの識別子をどうするかは保守者が決める |
| L3-02 | 🔴 | `rights-check.md` 14行（§1の表） | Sentrifugo: GPL-3.0、スター8、最後の更新 2017-10-30 | 8と2017-10-30は、フォーク `Sentrifugo/sentrifugo` の値（`"fork":true`、`"parent":"sapplica/sentrifugo"`、pushed_at 2017-10-30T21:29:14Z。作成は2017-11-15で、その後のpushはない）。本家 `sapplica/sentrifugo` は GPL-3.0、スター544、フォーク429、pushed_at 2021-07-02T11:35:43Z（`gh api repos/sapplica/sentrifugo` と `repos/Sentrifugo/sentrifugo`、2026-10-15取得） | 本家の値に直し、表に `owner/repo` の列を足す。「最後の更新」は pushed_at（UTC）だと書く。OrangeHRM の2026-06-28はUTCの日付で、日本時間では06-29（updated_at は2026-10-14） |
| L3-03 | 🟡 | `regulation-notes.md` 17行、`sources/revisions-2026-10-15.txt` 9行 | 2027-04-01施行の改正は「第115条（労災の請求権の時効）の項の整理」（写しは「項の分割」） | 115条を2項に分け、2項ただし書で、災害補償の原因の疾病が「容易に判断することができない疾病として政令で定めるもの」なら「「二年間」とあるのは、「五年間」とする」を加える（版 `322AC0000000049_20270401_508AC0000000060` と現行版の条文の差分、https://laws.e-gov.go.jp/api/2/law_data/322AC0000000049_20270401_508AC0000000060 、2026-10-15取得）。時効を延ばす実質の改正で、対象は労基法上の「災害補償の請求権」 | 「115条に、政令で定める疾病の災害補償の請求権の時効を5年とする特則を加え、附則143条3項の参照を「第百十五条第一項」に直す」と書く。kintai に影響しない、という結論は正しい |
| L3-04 | 🟡 | `rights-check.md` 22行、`regulation-notes.md` 40行 | 「勤怠サービスの規約そのものは、kintaiでは確認していません」。確認できていないこと3「他社の勤怠サービスの利用規約」 | マネーフォワード クラウドの規約（2026年7月31日改定）は、第3条(14)で「マネーフォワード クラウド勤怠」を本サービスに含め、第13条(14)で「本サービスと競合又は類似するサービスを開発、改良又は提供する行為」を禁じる。第5条3項(2)は「本サービスと類似・競合する製品又はサービスの開発若しくは提供し…ていると当社が判断した場合」を利用登録の拒否事由とする（https://biz.moneyforward.com/agreement/ 、2026-10-15取得） | マネーフォワード クラウド勤怠は、確認済みの規約の対象だと書く。未確認として残すのは freee人事労務・KING OF TIME・ジョブカンなどに絞る |
| L3-05 | 🟡 | `rights-check.md` 27行 | 「厚生労働省のサイトの利用条件は、特に記載がなければ公共データ利用規約（第1.0版）に準拠します」 | 厚労省の宣言の対象は「当ホームページで公開している情報」（https://www.mhlw.go.jp/chosakuken/index.html ）。試験データの出典には別サイト「確かめよう労働条件」の例1・例2がある（`docs/spec/vectors/official-examples.json`、https://www.check-roudou.mhlw.go.jp/study/roudousya_jikangai.html ）。同サイトに利用条件のページは見当たらず、表示は「Copyright © Ministry of Health, Labour and Welfare, All Rights reserved.」だけ（2026-10-15取得） | 「確かめよう労働条件」を、労働局と同じく確認できていないことに加える |
| L3-06 | 🟡 | `rights-check.md` 32行、`license-policy.md` 31行（手元の事実） | research/ にあるのは「出典URLつきの短い引用」。公的な解釈は「3文以内の引用と出典」。公開前に外すのは `notes_*.md` `out-*.md` | `research/log-A.txt`（454,632バイト）・`log-B.txt`・`log-C.txt` と、1つ上の `log-r1.txt` は実装AIの作業ログで、取得資料の抽出文をそのまま含む。1回の出力の最大は log-A の79,065字（厚労省・労働局の資料への `rg -C 5` の結果）。ローカルのパス `/Users/（ユーザー名）` が各ログに14〜65回出る。`.gitignore` が外すのは `research/downloads/` だけで、`REUSE.toml` は `.agent-harness/**` を CC-BY-4.0 として公開する前提 | ログ4本を公開物から外す（`.gitignore` に足すか、外す一覧に入れる）。外すまでは「短い引用だけ」と書かない |
| L3-07 | ⚪ | `regulation-notes.md` 19行、`revisions-2026-10-15.txt` 10行 | 2028-12-23施行の改正は「年少者の証明書などの規定」 | 令和8年法律第46号（民法等の一部を改正する法律の施行に伴う関係法律の整備等に関する法律）による用語の改正。57条2項・58条・59条の「後見人」を「未成年後見人」に、121条の「成年被後見人」を「特定補助人を付する処分の審判を受けた者」に改める（版 `…_20281223_508AC0000000046` との差分、2026-10-15取得） | 「民法の改正に伴う、未成年者の労働契約と罰則の規定の用語の改正」と書く |
| L3-08 | ⚪ | `regulation-notes.md` 21行 | 第212回の議題は「勤務間インターバル制度、「つながらない権利」の在り方、労働時間の把握」 | 議事次第の議題は「（１）労働契約承継法における労働者等への通知等の電子化について」「（２）労働基準関係法制について」。挙げた3点は資料No.2-1・2-2の題（https://www.mhlw.go.jp/content/11201250/001758760.pdf 、2026-10-15取得） | 「議題（2）で、勤務間インターバル制度…を論点とする資料が出た」と書く（L1-19 の直しの後も残る細部） |
| L3-09 | ⚪ | `rights-check.md` 59行 | 確認できていないこと4「公共データ利用規約（第1.0版）とCC BY 4.0の互換の、一次資料での確認」 | 規約本文1.7「本利用ルールは、クリエイティブ・コモンズ・ライセンスの表示4.0 国際ライセンス…と互換性があります。」（https://www.digital.go.jp/resources/open_data/public_data_license_v1.0 、2026-10-15取得） | 確認済みに移す。同じ本文は「数値データ、簡単な表・グラフ等は著作権による保護の対象ではありません」とも書く |
| L3-10 | ⚪ | `README.md` 45行 | 「[公共データ利用規約（第1.0版）](https://www.mhlw.go.jp/chosakuken/index.html)に準拠」「祝日は内閣府の「国民の祝日」CSV」 | リンク先は厚労省の利用規約のページで、規約の本文はデジタル庁にある。規約1.1の記載例は「出典：D庁ウェブサイト（当該ページのURL）、PDL1.0（規約原文ページのURL）」で、加工したときは「編集・加工等を行ったこと及びその主体」も書く。祝日の行にはURL・加工したこと・取得日がない（`docs/spec/holidays.md` 8行は「README と出力に出典の1行を付ける」とする） | 規約名のリンクを規約本文に向け、厚労省のページは別に示す。祝日に「内閣府「国民の祝日について」のCSV（URL）を UTF-8 と日付の書式に変換、2026-10-15取得」を足す |
| L3-11 | ⚪ | `license-policy.md` 11行 | 「法令は著作権法13条により著作物でない」 | 13条の見出しは「権利の目的とならない著作物」、本文は「この章の規定による権利の目的となることができない」（e-Gov 345AC0000000048、2026-10-15取得） | 「著作権法13条により権利の目的とならない」と書く（`rights-check.md` 26行の書き方が正確） |
| L3-12 | ⚪ | `license-policy.md` 21行 | 「BSLやFSLのようなソース公開型は、OSIの定義を満たさず」 | OSIの承認一覧（126件）に Business Source License と Functional Source License はない。ただし一覧の「BSL-1.0」は Boost Software License で、承認済み（https://opensource.org/api/licenses 、2026-10-15取得） | 「BUSL（Business Source License）やFSL」と書き、Boost と取り違えないようにする |
| L3-13 | ⚪ | `DISCLAIMER.md` 15行 | 「労働基準法の改正案（連続勤務の上限、法定休日の特定の義務化など）が成立した場合」 | これらは研究会報告書（2025-01-08公表）の提言で、法案ではない。厚労省の提出法律案に該当はない（第221回は労災保険法等の改正案など、第222回は特定B型肝炎の法案1本。https://www.mhlw.go.jp/stf/topics/bukyoku/soumu/houritu/222.html 、2026-10-15取得） | 「研究会報告書が提言する見直し（連続勤務の上限など）が法律になった場合」と書く |
| L3-14 | 🟡 | `rights-check.md` 50行（§7） | 「直接の依存は、実行時に `serde` `serde_json` `toml` `pico-args` `csv` `encoding_rs` だけです」 | 03:38 に読んだ時点では一致した。06:10 に `crates/kintai-import/Cargo.toml` の `[dependencies]` が `kintai-core` と `encoding_rs = "0.8"` だけになり、`Cargo.lock` からも `csv` と `csv-core` の項目が消えた（手元のファイル、2026-10-15 06:10以降に確認） | 実装が固まった後、公開の直前に `Cargo.toml` から一覧を作り直す（`cargo tree -e normal --depth 1` の結果を写すなど）。`cargo deny check` の結果と同じ時点にそろえる |

### 疑い・補足（重さは付けない）

- P-02 の「平成6年5月31日基発331号の原文は入手できていない」は正しい。ただ、大阪府の資料（令和8年3月更新）が要旨を引いている。「暦日としての休日がある場合には、原則どおり当該日の午前0時から午後12時までの休息時間に労働した場合が3割5分の割増賃金の支払いを要する休日労働時間となり…【平6.5.31基発331号】」（https://www.pref.osaka.lg.jp/documents/6136/028.pdf 、2026-10-15取得）。休日への帰属は「直接の資料が無い」より「原文は未入手で、要旨は公的機関の資料にある」が近い。社会保険労務士に渡す材料になる。
- 労基法の現行版（2026-07-17）は、前の版（2026-06-24）と本則が同じで、令和8年法律第60号の附則（施行期日など）が加わっただけ。文書の記述は誤りではない。

## 3. 確認できなかったこと

1. 昭和63年基発150号と平成6年基発331号の原文。厚労省の法令等データベースからは取得していない。前者は愛知労働局の抜粋、後者は大阪府の資料の要旨で、実在と内容の大筋だけを確かめた。
2. `haranishi/kintai` は認証なしの公開APIで404。非公開のリポジトリの有無は見ていない。
3. kamoku の確認日（2026-10-14）は、kamoku 側の記録で確かめただけ。マネーフォワードの規約の中身は、現行のページで確かめた。
4. 都道府県労働局のサイトの利用条件（文書も未確認として扱う）と、「確かめよう労働条件」の利用条件（ページが見つからない。L3-05）。
5. 範囲外として見ていないもの: J-PlatPat（商標・特許）、AIの生成物の著作物性、MIT・Apache-2.0 の組み込みや再ライセンスの可否（法律の判断）、`cargo deny check` の結果（文書も未実施とする）。
6. GitHub の件数とスター、crates.io のダウンロード数は取得時点の値で、日ごとに変わる。

## 4. 合っていた主な事実

- GitHubと名前: Frappe HRMS（GPL-3.0・8,924・2026-10-14）、Kimai（AGPL-3.0・5,073・2026-10-14）、OrangeHRM（GPL-3.0・1,151）、jp-payroll-mcp（MIT・2・2026-08-26作成）。crates.io の `kintai`（「simple kintai time management system」・2024-06-18作成・1,287回）と、5つの名前の空き。`kintai in:name` は1,543件で上位63・14・11、`勤怠 language:Rust` は6件で最大8（READMEまで含めると17件、最大は同じ8）。
- 依存: 03:38 の時点では、実行時の直接の依存6つが `Cargo.toml`・`Cargo.lock` と一致した（06:10 以降は5つ。L3-14）。`deny.toml` の許可リストは文書どおり。
- 規約: 著作権法13条1号・2号と32条。厚労省の利用規約（公共データ利用規約に準拠、出典・加工の記載・国が作成したかのような公表の禁止）。内閣府の利用規約（令和7年3月25日現在、公共データ利用規約。祝日CSVは適用外の一覧にない）とCSVの収録範囲（1955〜2027年）。国立天文台の予想表（表1の2020〜2050年の62日がkintaiの表と全件一致、「あくまで参考」）。マネーフォワードの規約 第13条(14)（2026年7月31日改定版）。
- 法令: 労基法の版（現行2026-07-17、未施行2027-04-01・2028-12-23）。割増賃金令（2割5分・3割5分）、労基則20条（5割・7割5分・6割）、24条の3〜24条の7、25条の2、54条、56条、附則71条。安衛法66条の8の3、安衛則52条の7の3（3年）。労基法34〜39条・108・109・115条・附則143条、祝日法2・3条、社労士法2条・27条、民法143条2項、個人情報保護法17・21・23条。2023-04-01に附則138条（中小事業主の猶予）が削られたこと。
- 研究会・分科会・国会: 研究会報告書は2025-01-08公表で、13日を超える連続勤務（p.40）、法定休日の特定（p.41）、インターバル11時間（p.42）、副業の割増の通算（p.49）の提言がある。第212回分科会は2026-10-14。労働時間法制の改正案は、厚労省の提出法律案の一覧（第221回・第222回）にない。
- 通達・Q&A: 昭和63年基発1号（日は暦日、継続勤務は始業日、週は日曜から）、昭和63年基発150号（愛知労働局の抜粋）、平成21年基発0529001号 p.4（月の起算日）、Q&A 2-27（p.26）と2-32（p.27）、年5日の解説の印刷p.9（「管理が煩雑」）、長崎労働局 第28回（法定休日を特定しない場合）。
- ライセンスの文: MIT と Apache-2.0 の "AS IS"、Apache-2.0 の特許の許諾（3条）、DCO 1.1、CONTRIBUTING の AI の申告。
