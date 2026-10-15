//! 法令の根拠の定数が、取得した条文の記録（`docs/legal/sources/`）と一致する。
//! AC-11.2 の「条文」は、ここで確かめた定数から作る。

use kintai_jp_labor::Provenance;
use kintai_jp_labor::provenance::{
    ALL, CONFIRMED_ON, LABOR_STANDARDS_ACT, LABOR_STANDARDS_REGULATION, LSA_32_2, LSA_37_1,
    PREMIUM_RATE_ORDER, REG_20,
};

fn source(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../docs/legal/sources/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// 版ID・法令ID・URL・施行日が、取得の記録と一致する。
#[test]
fn laws_match_the_retrieved_revisions() {
    let revisions = source("revisions-2026-10-15.txt");
    let statutes = source("statutes-2026-10-15.txt");
    for law in [
        LABOR_STANDARDS_ACT,
        LABOR_STANDARDS_REGULATION,
        PREMIUM_RATE_ORDER,
    ] {
        assert!(revisions.contains(law.revision_id), "{}", law.name);
        assert!(
            statutes.contains(&format!("版ID={}", law.revision_id)),
            "{}",
            law.name
        );
        assert!(
            statutes.contains(&format!("法令ID={}", law.law_id)),
            "{}",
            law.name
        );
        assert!(
            statutes.contains(&format!("出典: {}", law.url)),
            "{}",
            law.name
        );
        assert!(law.revision_id.starts_with(law.law_id));
        let compact = law.enforced_on.replace('-', "");
        assert!(law.revision_id.contains(&compact), "{}", law.name);
        assert!(statutes.contains(&format!("施行日={}", law.enforced_on)));
    }
}

/// すべての根拠に、確認日と条項と説明がある。表記は法令名＋条項。
#[test]
fn every_provenance_is_complete() {
    assert_eq!(CONFIRMED_ON, "2026-10-15");
    assert!(CONFIRMED_ON.parse::<kintai_core::Date>().is_ok());
    for p in ALL {
        assert_eq!(p.confirmed_on, CONFIRMED_ON);
        assert_ne!(p.article, "");
        assert_ne!(p.summary, "");
        assert_eq!(p.citation(), format!("{}{}", p.law.name, p.article));
    }
    assert_eq!(LSA_32_2.citation(), "労働基準法第32条第2項");
    assert_eq!(LSA_37_1.law, LABOR_STANDARDS_ACT);
    assert_eq!(REG_20.citation(), "労働基準法施行規則第20条");
    let mut citations: Vec<String> = ALL.iter().map(Provenance::citation).collect();
    citations.sort();
    citations.dedup();
    assert_eq!(citations.len(), ALL.len(), "重複がない");
}
