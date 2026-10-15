//! 法令の根拠（法令名、条項、e-Gov の URL と版ID、確認日）。
//!
//! 版ID と URL は `docs/legal/sources/revisions-2026-10-15.txt` と
//! `docs/legal/sources/statutes-2026-10-15.txt`（e-Gov 法令検索 API v2 から取得）に合わせる。
//! 改正があれば定数を足し、過去の日付の結果は変えない。

/// 法令（e-Gov 法令検索の1つの版）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Law {
    /// 法令名。
    pub name: &'static str,
    /// e-Gov の法令ID。
    pub law_id: &'static str,
    /// e-Gov の版ID（`法令ID_施行日_改正法令ID`）。
    pub revision_id: &'static str,
    /// 版の施行日（`YYYY-MM-DD`）。
    pub enforced_on: &'static str,
    /// e-Gov 法令検索の URL。
    pub url: &'static str,
}

/// 法令の根拠（条項まで）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Provenance {
    /// 法令。
    pub law: Law,
    /// 条項（例: `第32条第2項`）。
    pub article: &'static str,
    /// 何を定めているか（短い説明）。
    pub summary: &'static str,
    /// 条文を取得して照合した日（`YYYY-MM-DD`）。
    pub confirmed_on: &'static str,
}

impl Provenance {
    /// 法令名と条項をつないだ表記（例: `労働基準法第32条第2項`）。
    #[must_use]
    pub fn citation(&self) -> String {
        format!("{}{}", self.law.name, self.article)
    }
}

/// 確認日。e-Gov 法令検索 API v2 から条文を取得し、照合した日。
pub const CONFIRMED_ON: &str = "2026-10-15";

/// 労働基準法（2026-07-17 施行の版）。
pub const LABOR_STANDARDS_ACT: Law = Law {
    name: "労働基準法",
    law_id: "322AC0000000049",
    revision_id: "322AC0000000049_20260717_508AC0000000060",
    enforced_on: "2026-07-17",
    url: "https://laws.e-gov.go.jp/law/322AC0000000049",
};

/// 労働基準法施行規則（2026-04-01 施行の版）。
pub const LABOR_STANDARDS_REGULATION: Law = Law {
    name: "労働基準法施行規則",
    law_id: "322M40000100023",
    revision_id: "322M40000100023_20260401_508M60000100057",
    enforced_on: "2026-04-01",
    url: "https://laws.e-gov.go.jp/law/322M40000100023",
};

/// 割増賃金令（労働基準法第三十七条第一項の時間外及び休日の割増賃金に係る率の最低限度を定める政令。2001-01-06 施行の版）。
pub const PREMIUM_RATE_ORDER: Law = Law {
    name: "労働基準法第三十七条第一項の時間外及び休日の割増賃金に係る率の最低限度を定める政令",
    law_id: "406CO0000000005",
    revision_id: "406CO0000000005_20010106_412CO0000000309",
    enforced_on: "2001-01-06",
    url: "https://laws.e-gov.go.jp/law/406CO0000000005",
};

const fn act(article: &'static str, summary: &'static str) -> Provenance {
    Provenance {
        law: LABOR_STANDARDS_ACT,
        article,
        summary,
        confirmed_on: CONFIRMED_ON,
    }
}

const fn regulation(article: &'static str, summary: &'static str) -> Provenance {
    Provenance {
        law: LABOR_STANDARDS_REGULATION,
        article,
        summary,
        confirmed_on: CONFIRMED_ON,
    }
}

/// 労働基準法第32条第1項: 1週間40時間。
pub const LSA_32_1: Provenance = act(
    "第32条第1項",
    "1週間について40時間を超えて労働させてはならない",
);
/// 労働基準法第32条第2項: 1日8時間。
pub const LSA_32_2: Provenance = act(
    "第32条第2項",
    "1日について8時間を超えて労働させてはならない",
);
/// 労働基準法第32条の2: 1か月単位の変形労働時間制。
pub const LSA_32NO2: Provenance = act("第32条の2", "1か月単位の変形労働時間制");
/// 労働基準法第32条の3: フレックスタイム制。
pub const LSA_32NO3: Provenance = act("第32条の3", "フレックスタイム制");
/// 労働基準法第32条の4: 1年単位の変形労働時間制。
pub const LSA_32NO4: Provenance = act("第32条の4", "1年単位の変形労働時間制");
/// 労働基準法第32条の5: 1週間単位の非定型的変形労働時間制。
pub const LSA_32NO5: Provenance = act("第32条の5", "1週間単位の非定型的変形労働時間制");
/// 労働基準法第35条第1項: 毎週少なくとも1回の休日。
pub const LSA_35_1: Provenance = act("第35条第1項", "毎週少なくとも1回の休日");
/// 労働基準法第35条第2項: 4週間を通じ4日以上の休日（変形週休制）。
pub const LSA_35_2: Provenance = act("第35条第2項", "4週間を通じ4日以上の休日（変形週休制）");
/// 労働基準法第36条: 時間外・休日労働の協定と上限。
pub const LSA_36: Provenance = act("第36条", "時間外及び休日の労働（協定と上限）");
/// 労働基準法第36条第5項: 臨時的な特別の事情がある場合の上限（特別条項）。
pub const LSA_36_5: Provenance = act(
    "第36条第5項",
    "臨時的に限度時間を超える場合の上限（1か月100時間未満、1年720時間）",
);
/// 労働基準法第36条第6項第1号: 坑内労働その他の健康上特に有害な業務の、1日の時間外は2時間まで。
pub const LSA_36_6_ITEM1: Provenance = act(
    "第36条第6項第1号",
    "坑内労働その他の健康上特に有害な業務は、1日の時間外が2時間を超えないこと",
);
/// 労働基準法第36条第11項: 研究開発業務への上限規制の適用除外。
pub const LSA_36_11: Provenance = act(
    "第36条第11項",
    "新たな技術、商品又は役務の研究開発に係る業務の適用除外",
);
/// 労働基準法第37条第1項: 時間外・休日の割増賃金。1か月60時間を超えた時間外は5割以上。
pub const LSA_37_1: Provenance = act(
    "第37条第1項",
    "時間外・休日の割増賃金（1か月60時間を超えた時間外は5割以上）",
);
/// 労働基準法第37条第4項: 深夜（午後10時〜午前5時）の割増賃金。
pub const LSA_37_4: Provenance = act(
    "第37条第4項",
    "午後10時から午前5時までの深夜の割増賃金（2割5分以上）",
);
/// 労働基準法第38条の2: 事業場外労働のみなし労働時間制。
pub const LSA_38NO2: Provenance = act("第38条の2", "事業場外労働のみなし労働時間制");
/// 労働基準法第38条の3: 専門業務型裁量労働制。
pub const LSA_38NO3: Provenance = act("第38条の3", "専門業務型裁量労働制");
/// 労働基準法第38条の4: 企画業務型裁量労働制。
pub const LSA_38NO4: Provenance = act("第38条の4", "企画業務型裁量労働制");
/// 労働基準法第41条第1号: 農業・畜産・養蚕・水産の事業に従事する者。
pub const LSA_41_ITEM1: Provenance = act(
    "第41条第1号",
    "別表第一第六号（林業を除く）又は第七号の事業に従事する者（労働時間・休憩・休日の規定の適用除外）",
);
/// 労働基準法第41条第2号: 監督・管理の地位にある者、機密の事務を取り扱う者。
pub const LSA_41_ITEM2: Provenance = act(
    "第41条第2号",
    "監督若しくは管理の地位にある者又は機密の事務を取り扱う者（労働時間・休憩・休日の規定の適用除外）",
);
/// 労働基準法第41条第3号: 監視又は断続的労働に従事する者で、行政官庁の許可を受けたもの。
pub const LSA_41_ITEM3: Provenance = act(
    "第41条第3号",
    "監視又は断続的労働に従事する者で、使用者が行政官庁の許可を受けたもの（労働時間・休憩・休日の規定の適用除外）",
);
/// 労働基準法第41条の2: 高度プロフェッショナル制度。
pub const LSA_41NO2: Provenance = act("第41条の2", "高度プロフェッショナル制度");
/// 労働基準法第60条: 年少者の労働時間及び休日。
pub const LSA_60: Provenance = act("第60条", "満18歳に満たない者の労働時間及び休日");
/// 労働基準法第61条: 年少者の深夜業。
pub const LSA_61: Provenance = act("第61条", "満18歳に満たない者の深夜業");
/// 労働基準法第66条: 妊産婦の請求による時間外・休日・深夜の制限。
pub const LSA_66: Provenance = act("第66条", "妊産婦の請求による時間外・休日・深夜業の制限");
/// 労働基準法附則第139条第1項: 建設の事業のうち災害時の復旧・復興の事業。
pub const LSA_SUPPL_139_1: Provenance = act(
    "附則第139条第1項",
    "災害時における復旧及び復興の事業への、1か月100時間未満・2〜6か月平均80時間の規定の適用除外",
);
/// 労働基準法附則第140条: 自動車の運転の業務の上限の特例。
pub const LSA_SUPPL_140: Provenance = act("附則第140条", "自動車の運転の業務の上限の特例");
/// 労働基準法附則第141条: 医業に従事する医師の上限の特例。
pub const LSA_SUPPL_141: Provenance = act("附則第141条", "医業に従事する医師の上限の特例");
/// 労働基準法施行規則第18条: 健康上特に有害な業務の範囲。
pub const REG_18: Provenance = regulation("第18条", "健康上特に有害な業務の範囲");
/// 労働基準法施行規則第20条: 時間外・休日の労働が深夜に及ぶ場合の割増率。
pub const REG_20: Provenance = regulation(
    "第20条",
    "時間外が深夜に及ぶときは5割以上（1か月60時間超は7割5分以上）、休日が深夜に及ぶときは6割以上",
);
/// 労働基準法施行規則第23条: 宿直・日直の勤務の許可。
pub const REG_23: Provenance = regulation("第23条", "宿直又は日直の勤務で断続的な業務の許可");
/// 労働基準法施行規則第25条の2: 特例措置対象事業場の週44時間。
pub const REG_25NO2: Provenance = regulation(
    "第25条の2",
    "商業・映画演劇業・保健衛生業・接客娯楽業で常時10人未満の事業場は週44時間",
);
/// 割増賃金令（本則）: 時間外2割5分、休日3割5分。
pub const PREMIUM_ORDER: Provenance = Provenance {
    law: PREMIUM_RATE_ORDER,
    article: "（本則）",
    summary: "時間外の割増は2割5分以上、休日の割増は3割5分以上",
    confirmed_on: CONFIRMED_ON,
};

/// この版が持つ根拠の一覧。
pub const ALL: [Provenance; 32] = [
    LSA_32_1,
    LSA_32_2,
    LSA_32NO2,
    LSA_32NO3,
    LSA_32NO4,
    LSA_32NO5,
    LSA_35_1,
    LSA_35_2,
    LSA_36,
    LSA_36_5,
    LSA_36_6_ITEM1,
    LSA_36_11,
    LSA_37_1,
    LSA_37_4,
    LSA_38NO2,
    LSA_38NO3,
    LSA_38NO4,
    LSA_41_ITEM1,
    LSA_41_ITEM2,
    LSA_41_ITEM3,
    LSA_41NO2,
    LSA_60,
    LSA_61,
    LSA_66,
    LSA_SUPPL_139_1,
    LSA_SUPPL_140,
    LSA_SUPPL_141,
    REG_18,
    REG_20,
    REG_23,
    REG_25NO2,
    PREMIUM_ORDER,
];
