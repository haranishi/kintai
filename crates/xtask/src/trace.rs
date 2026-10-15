use crate::{Result, lines, walk};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
fn reference(text: &str) -> Vec<&str> {
    let mut rest = text;
    let mut found = Vec::new();
    while let Some((_, tail)) = rest.split_once("AC-") {
        let digits = tail.chars().take(2).all(|c| c.is_ascii_digit());
        if digits
            && tail.get(2..3) == Some(".")
            && let Some(number) = tail.get(3..)
        {
            let length = number.bytes().take_while(u8::is_ascii_digit).count();
            if length > 0
                && let Some(start) = rest.find("AC-")
                && let Some(ac) = rest.get(start..start.saturating_add(6).saturating_add(length))
            {
                found.push(ac);
            }
        }
        rest = tail;
    }
    found
}
fn condition(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_start().strip_prefix('-')?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let ac = reference(rest).into_iter().next()?;
    if !rest.starts_with(ac) {
        return None;
    }
    let tail = rest.strip_prefix(ac)?;
    if !tail.starts_with(char::is_whitespace) {
        return None;
    }
    let kind = tail.trim_start().strip_prefix('[')?.split_once(']')?.0;
    if kind.is_empty() || !kind.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some((ac, kind))
}
fn test_references(root: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    let mut places: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (folder, extension) in [("crates", "rs"), ("tools", "py"), ("web-tests", "cjs")] {
        for path in walk(&root.join(folder), &["node_modules", "target", ".git"])? {
            if path.extension().is_none_or(|s| s != extension) {
                continue;
            }
            let text = fs::read_to_string(&path)?;
            // A production-code comment is not evidence of a regression test.
            let unit_start = lines(&text)
                .iter()
                .position(|line| line.trim() == "#[cfg(test)]");
            let executable = path
                .strip_prefix(root)?
                .components()
                .any(|part| matches!(part.as_os_str().to_str(), Some("tests" | "examples")));
            for (number, line) in lines(&text).into_iter().enumerate() {
                if folder == "crates"
                    && !executable
                    && unit_start.is_none_or(|start| number < start)
                {
                    continue;
                }
                for ac in reference(line).into_iter().collect::<BTreeSet<_>>() {
                    places.entry(ac.to_owned()).or_default().push(format!(
                        "{}:{}",
                        path.strip_prefix(root)?.display(),
                        number.saturating_add(1)
                    ));
                }
            }
        }
    }
    Ok(places)
}

pub(super) fn run(root: &Path, strict: bool) -> Result<i32> {
    let mut found = Vec::new();
    let spec = root.join("docs/spec");
    if spec.is_dir() {
        let mut stories = fs::read_dir(spec)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        stories.sort();
        for path in stories
            .into_iter()
            .filter(|p| p.extension().is_some_and(|s| s == "md"))
        {
            for line in lines(&fs::read_to_string(path)?) {
                if let Some((ac, kind)) = condition(line) {
                    found.push((ac.to_owned(), kind.to_owned()));
                }
            }
        }
    }
    let places = test_references(root)?;
    let width = found
        .iter()
        .filter(|(_, k)| k != "manual")
        .map(|(ac, _)| ac.len())
        .max()
        .unwrap_or(8);
    println!(
        "{:<width$}  種類  参照  内訳（crates/tools/web）  例",
        "条件"
    );
    let mut missing = Vec::new();
    for (ac, kind) in &found {
        if kind == "manual" {
            continue;
        }
        let refs = places.get(ac).map_or(&[][..], Vec::as_slice);
        let example = refs.first().map_or("（なし）", String::as_str);
        let split = format!(
            "{}/{}/{}",
            refs.iter().filter(|s| s.starts_with("crates/")).count(),
            refs.iter().filter(|s| s.starts_with("tools/")).count(),
            refs.iter().filter(|s| s.starts_with("web-tests/")).count()
        );
        println!(
            "{ac:<width$}  {kind:<4}  {:>4}  {split:<20}  {example}",
            refs.len()
        );
        let prefix = match kind.as_str() {
            "rust" => Some("crates/"),
            "python" => Some("tools/"),
            "e2e" => Some("web-tests/"),
            _ => None,
        };
        if prefix.is_some_and(|prefix| !refs.iter().any(|place| place.starts_with(prefix))) {
            missing.push(ac.as_str());
        }
    }
    let known = found
        .iter()
        .map(|(ac, _)| ac.as_str())
        .collect::<BTreeSet<_>>();
    let unknown = places
        .keys()
        .filter(|s| !known.contains(s.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    println!(
        "\n条件 {}件（manual {}件は対象外）。参照の無い条件: {}件",
        found.len(),
        found.iter().filter(|(_, k)| k == "manual").count(),
        missing.len()
    );
    if !missing.is_empty() {
        println!("  {}", missing.join(" "));
    }
    if !unknown.is_empty() {
        println!("仕様に無い番号への参照: {}", unknown.join(" "));
    }
    Ok(i32::from(strict && !missing.is_empty()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse() {
        let a = ["AC-", "12.10"].concat();
        assert_eq!(reference(&a), vec![a.as_str()]);
        assert_eq!(
            condition(&format!(" - {a} [rust] rest")),
            Some((a.as_str(), "rust"))
        );
        assert!(condition(&format!("-{a} [rust]")).is_none());
    }
}
