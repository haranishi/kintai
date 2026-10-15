//! AC-17.1: exercise Cargo's real metadata invalidation with tiny dependency-free crates.
mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = common::temp_dir("version-build");
        fs::create_dir_all(root.join("crates/cli/src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/cli\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/cli/Cargo.toml"),
            "[package]\nname = \"revision-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/cli/build.rs"),
            include_str!("../build.rs"),
        )
        .unwrap();
        fs::write(
            root.join("crates/cli/src/main.rs"),
            "fn main() { println!(\"{}\", env!(\"KINTAI_GIT_HASH\")); }\n",
        )
        .unwrap();
        Self(root)
    }

    fn write(&self, name: &str, contents: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn revision(&self, override_hash: Option<&str>) -> String {
        let mut command = Command::new(env!("CARGO"));
        command
            .args(["run", "--quiet", "--offline"])
            .current_dir(&self.0)
            .env("CARGO_TARGET_DIR", self.0.join("target"))
            .env_remove("KINTAI_GIT_HASH");
        if let Some(hash) = override_hash {
            command.env("KINTAI_GIT_HASH", hash);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn ac17_1_archive_to_checkout_and_packed_to_loose_refresh_the_revision() {
    let fixture = Fixture::new();
    assert_eq!(fixture.revision(None), "unknown");
    fixture.write(".git/HEAD", "ref: refs/heads/main\n");
    fixture.write(
        ".git/refs/heads/main",
        "aaaaaaa1111111111111111111111111111111111\n",
    );
    assert_eq!(fixture.revision(None), "aaaaaaa");
    fixture.write(
        ".git/packed-refs",
        "bbbbbbb2222222222222222222222222222222222 refs/heads/main\n",
    );
    fs::remove_file(fixture.0.join(".git/refs/heads/main")).unwrap();
    assert_eq!(fixture.revision(None), "bbbbbbb");
    fixture.write(
        ".git/refs/heads/main",
        "ccccccc3333333333333333333333333333333333\n",
    );
    assert_eq!(fixture.revision(None), "ccccccc");
}

#[test]
fn ac17_1_worktree_common_refs_detached_head_and_override_are_supported() {
    let fixture = Fixture::new();
    fixture.write(".git", "gitdir: metadata/worktree\n");
    fixture.write("metadata/worktree/HEAD", "ref: refs/heads/main\n");
    fixture.write("metadata/worktree/commondir", "../common\n");
    fixture.write(
        "metadata/common/packed-refs",
        "ddddddd4444444444444444444444444444444444 refs/heads/main\n",
    );
    fs::create_dir_all(fixture.0.join("metadata/common/refs/heads")).unwrap();
    assert_eq!(fixture.revision(None), "ddddddd");
    fixture.write(
        "metadata/common/refs/heads/main",
        "eeeeeee5555555555555555555555555555555555\n",
    );
    assert_eq!(fixture.revision(None), "eeeeeee");
    fixture.write(
        "metadata/worktree/HEAD",
        "fffffff6666666666666666666666666666666666\n",
    );
    assert_eq!(fixture.revision(None), "fffffff");
    assert_eq!(fixture.revision(Some(" 123456789abcdef ")), "1234567");
    assert_eq!(fixture.revision(Some("not-a-hash")), "fffffff");
}
