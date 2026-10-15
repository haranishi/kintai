//! `kintai --version` に出す git の短いハッシュを、ビルドのときに決める。
//!
//! git コマンドは呼ばず、`.git` のファイル（HEAD と参照）を読むだけにする。
//! 環境変数 `KINTAI_GIT_HASH` があればそれを使い、どちらも取れなければ `unknown`。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=KINTAI_GIT_HASH");
    let from_env = env::var("KINTAI_GIT_HASH")
        .ok()
        .and_then(|hash| short_hash(hash.trim()));
    let hash = from_env
        .or_else(read_git_hash)
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo::rustc-env=KINTAI_GIT_HASH={hash}");
}

/// 16進の文字だけからなる7文字以上の値なら、先頭7文字。
fn short_hash(text: &str) -> Option<String> {
    let valid = text.len() >= 7 && text.bytes().all(|b| b.is_ascii_hexdigit());
    valid.then(|| text.chars().take(7).collect())
}

/// 変わったら作り直す、と cargo に伝える（存在するファイルだけ）。
fn watch(path: &Path) {
    if path.exists() {
        println!("cargo::rerun-if-changed={}", path.display());
    }
}

fn read_git_hash() -> Option<String> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR")?);
    let root = manifest.parent()?.parent()?;
    let dot_git = root.join(".git");
    // An archive can later become a checkout. Also watch the worktree pointer,
    // including when absent, so Cargo does not cache `unknown` permanently.
    if !dot_git.is_dir() {
        println!("cargo::rerun-if-changed={}", dot_git.display());
    }
    let git_dir = if dot_git.is_file() {
        // ワークツリーでは `.git` が「gitdir: <場所>」を書いたファイルになる。
        let text = fs::read_to_string(&dot_git).ok()?;
        let target = text.trim().strip_prefix("gitdir:")?.trim();
        root.join(target)
    } else {
        dot_git
    };
    let head_path = git_dir.join("HEAD");
    watch(&head_path);
    let head = fs::read_to_string(&head_path).ok()?;
    let head = head.trim();
    let Some(reference) = head.strip_prefix("ref:").map(str::trim) else {
        return short_hash(head);
    };
    // 参照はワークツリーごとの場所ではなく、共通の場所にあることがある。
    let common_path = git_dir.join("commondir");
    watch(&common_path);
    let common = fs::read_to_string(common_path)
        .ok()
        .map_or_else(|| git_dir.clone(), |dir| git_dir.join(dir.trim()));
    // A packed branch can acquire a new loose ref without changing HEAD or
    // packed-refs. Watch the ref directory to observe that transition.
    println!("cargo::rerun-if-changed={}", common.join("refs").display());
    for dir in [&git_dir, &common] {
        let loose = dir.join(reference);
        watch(&loose);
        if let Ok(text) = fs::read_to_string(&loose) {
            return short_hash(text.trim());
        }
    }
    let packed = common.join("packed-refs");
    watch(&packed);
    let text = fs::read_to_string(&packed).ok()?;
    text.lines().find_map(|line| {
        let (hash, name) = line.split_once(' ')?;
        (name.trim() == reference)
            .then(|| short_hash(hash))
            .flatten()
    })
}
