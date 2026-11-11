mod common;

use common::{Fixture, git};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cleaner(cwd: &Path, args: &[&str]) -> Output {
    let binary = env!("CARGO_BIN_EXE_worktree-cleaner");
    Command::new(binary)
        .current_dir(cwd)
        .env("HOME", cwd)
        .env("XDG_CONFIG_HOME", cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .args(args)
        .output()
        .unwrap()
}

fn rows(output: &Output) -> Vec<Vec<String>> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| {
            let fields: Vec<String> = line.split('\t').map(str::to_owned).collect();
            assert_eq!(fields.len(), 4, "{line}");
            fields
        })
        .collect()
}

fn decision(output: &Output, path: &Path) -> String {
    rows(output)
        .into_iter()
        .find(|fields| fields[1] == path.to_str().unwrap())
        .unwrap_or_else(|| panic!("missing {}", path.display()))[2]
        .clone()
}

#[test]
fn invariant_01_04_main_and_base_are_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let worktree = fixture.worktree(&repo, "done");
    let output = cleaner(&repo, &["--base", "main"]);
    assert_eq!(decision(&output, &worktree), "candidate");
    assert!(
        rows(&output)
            .iter()
            .all(|fields| fields[1] != repo.to_str().unwrap())
    );
    git(&repo, &["worktree", "lock", worktree.to_str().unwrap()]);
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &worktree),
        "keep"
    );
    git(&repo, &["worktree", "unlock", worktree.to_str().unwrap()]);
    assert_eq!(
        decision(&cleaner(&worktree, &["--base", "main"]), &worktree),
        "keep"
    );
}

#[test]
fn invariant_05_ancestors_are_candidates_and_divergence_is_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let same = fixture.worktree(&repo, "same");
    let divergent = fixture.worktree(&repo, "divergent");
    fs::write(divergent.join("new"), "new\n").unwrap();
    git(&divergent, &["add", "new"]);
    git(&divergent, &["commit", "-m", "diverge"]);
    let output = cleaner(&repo, &["--base", "main"]);
    assert_eq!(decision(&output, &same), "candidate");
    assert_eq!(decision(&output, &divergent), "keep");
    git(&divergent, &["checkout", "--detach"]);
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &divergent),
        "keep"
    );
}

#[test]
fn invariant_06_07_index_and_untracked_files_are_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let staged = fixture.worktree(&repo, "staged");
    fs::write(staged.join("tracked"), "changed\n").unwrap();
    git(&staged, &["add", "tracked"]);
    let untracked = fixture.worktree(&repo, "untracked");
    fs::write(untracked.join("new"), "new\n").unwrap();
    let unstaged = fixture.worktree(&repo, "unstaged");
    fs::write(unstaged.join("tracked"), "changed\n").unwrap();
    let ignored = fixture.worktree(&repo, "ignored");
    fs::write(repo.join(".git/info/exclude"), "build\n").unwrap();
    fs::write(ignored.join("build"), "artifact\n").unwrap();
    assert!(git(&ignored, &["status", "--porcelain", "--ignored"]).contains("!! build"));
    let output = cleaner(&repo, &["--base", "main"]);
    for path in [&staged, &untracked, &unstaged, &ignored] {
        assert_eq!(decision(&output, path), "keep", "{}", path.display());
    }
}

#[test]
fn invariant_09_repository_operation_is_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let worktree = fixture.worktree(&repo, "done");
    let gitdir = git(&worktree, &["rev-parse", "--git-dir"]);
    fs::write(
        PathBuf::from(gitdir.trim()).join("MERGE_HEAD"),
        "0000000000000000000000000000000000000000\n",
    )
    .unwrap();
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &worktree),
        "keep"
    );
}

#[test]
fn invariant_11_13_listing_and_removal_preserve_branch() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let worktree = fixture.worktree(&repo, "done");
    let before = git(&repo, &["show-ref", "refs/heads/done"]);
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &worktree),
        "candidate"
    );
    assert!(worktree.exists());
    assert_eq!(git(&repo, &["show-ref", "refs/heads/done"]), before);
    let output = cleaner(
        &repo,
        &["--base", "main", "--remove", worktree.to_str().unwrap()],
    );
    assert_eq!(decision(&output, &worktree), "removed");
    assert!(!worktree.exists());
    assert_eq!(git(&repo, &["show-ref", "refs/heads/done"]), before);
    assert!(!git(&repo, &["worktree", "list", "--porcelain"]).contains(worktree.to_str().unwrap()));
}

#[test]
fn invariant_14_16_parent_directory_uses_each_repository_base() {
    let fixture = Fixture::new();
    let first = fixture.repo("first", "main");
    let second = fixture.repo("second", "master");
    let first_tree = fixture.worktree(&first, "first-done");
    let second_tree = fixture.worktree(&second, "second-done");
    let output = cleaner(&fixture.path, &[]);
    assert_eq!(decision(&output, &first_tree), "candidate");
    assert_eq!(decision(&output, &second_tree), "candidate");
    let outside = cleaner(Path::new("/"), &["--root", fixture.path.to_str().unwrap()]);
    assert_eq!(decision(&outside, &first_tree), "candidate");
    assert_eq!(decision(&outside, &second_tree), "candidate");
}

#[test]
fn invariant_12_remove_all_rechecks_and_skips_dirty_worktrees() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let clean = fixture.worktree(&repo, "clean");
    let dirty = fixture.worktree(&repo, "dirty");
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &dirty),
        "candidate"
    );
    fs::write(dirty.join("new"), "work\n").unwrap();
    let output = cleaner(&repo, &["--base", "main", "--remove-all"]);
    assert_eq!(decision(&output, &dirty), "keep");
    assert_eq!(decision(&output, &clean), "removed");
    assert!(dirty.exists());
    assert!(!clean.exists());
}

#[test]
fn invariant_08_nested_repository_is_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let worktree = fixture.worktree(&repo, "nested");
    let nested = worktree.join("subproject");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "-b", "main"]);
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &worktree),
        "keep"
    );
}

#[test]
fn invariant_10_broken_registration_is_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let worktree = fixture.worktree(&repo, "broken");
    fs::remove_file(worktree.join(".git")).unwrap();
    let output = cleaner(&repo, &["--base", "main", "--remove-all"]);
    assert_eq!(decision(&output, &worktree), "keep");
    assert!(worktree.exists());
}

#[test]
fn invariant_14_only_direct_real_directories_are_entry_points() {
    let fixture = Fixture::new();
    let outer = fixture.path.join("outer");
    fs::create_dir(&outer).unwrap();
    let nested = outer.join("nested");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "-b", "main"]);
    git(&nested, &["config", "user.email", "test@example.com"]);
    git(&nested, &["config", "user.name", "Test"]);
    fs::write(nested.join("tracked"), "initial\n").unwrap();
    git(&nested, &["add", "tracked"]);
    git(&nested, &["commit", "-m", "initial"]);
    let tree = fixture.worktree(&nested, "nested-done");
    assert!(
        rows(&cleaner(&fixture.path, &["--base", "main"]))
            .iter()
            .all(|fields| fields[1] != tree.to_str().unwrap())
    );
    #[cfg(unix)]
    {
        let direct = fixture.repo("direct", "main");
        let direct_tree = fixture.worktree(&direct, "direct-done");
        std::os::unix::fs::symlink(&direct, fixture.path.join("alias")).unwrap();
        let observed = rows(&cleaner(&fixture.path, &["--base", "main"]));
        assert_eq!(
            observed
                .iter()
                .filter(|fields| fields[1] == direct_tree.to_str().unwrap())
                .count(),
            1
        );
    }
}

#[test]
fn invariant_15_16_missing_base_does_not_block_other_repositories() {
    let fixture = Fixture::new();
    let missing = fixture.repo("missing", "main");
    let valid = fixture.repo("valid", "main");
    let missing_tree = fixture.worktree(&missing, "missing-done");
    let valid_tree = fixture.worktree(&valid, "valid-done");
    git(
        &missing,
        &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"],
    );
    let output = cleaner(&fixture.path, &[]);
    assert_eq!(decision(&output, &missing_tree), "keep");
    assert_eq!(decision(&output, &valid_tree), "candidate");
}

#[test]
fn invariant_12_explicit_removal_is_limited_to_discovered_worktrees() {
    let fixture = Fixture::new();
    let first = fixture.repo("first", "main");
    let second = fixture.repo("second", "main");
    let outside = fixture.worktree(&second, "outside");
    let output = cleaner(
        &first,
        &["--base", "main", "--remove", outside.to_str().unwrap()],
    );
    assert!(!output.status.success());
    assert!(outside.exists());
}

#[test]
fn invariant_04_base_branch_worktree_is_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    git(&repo, &["switch", "-c", "host"]);
    let base_tree = fixture.path.join("base-tree");
    git(
        &repo,
        &["worktree", "add", base_tree.to_str().unwrap(), "main"],
    );
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &base_tree),
        "keep"
    );
}

#[test]
fn invariant_05_ancestor_branch_is_candidate() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let ancestor = fixture.worktree(&repo, "ancestor");
    fs::write(repo.join("later"), "later\n").unwrap();
    git(&repo, &["add", "later"]);
    git(&repo, &["commit", "-m", "later"]);
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &ancestor),
        "candidate"
    );
}

#[test]
fn invariant_06_hidden_tracked_changes_are_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let skip = fixture.worktree(&repo, "skip");
    let assume = fixture.worktree(&repo, "assume");
    git(&skip, &["update-index", "--skip-worktree", "tracked"]);
    git(&assume, &["update-index", "--assume-unchanged", "tracked"]);
    fs::write(skip.join("tracked"), "changed\n").unwrap();
    fs::write(assume.join("tracked"), "changed\n").unwrap();
    assert!(git(&skip, &["status", "--porcelain"]).trim().is_empty());
    assert!(git(&assume, &["status", "--porcelain"]).trim().is_empty());
    let output = cleaner(&repo, &["--base", "main"]);
    assert_eq!(decision(&output, &skip), "keep");
    assert_eq!(decision(&output, &assume), "keep");
}

#[test]
fn invariant_08_uninitialized_tracked_gitlink_is_preserved() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let oid = git(&repo, &["rev-parse", "HEAD"]);
    git(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},submodule", oid.trim()),
        ],
    );
    git(&repo, &["commit", "-m", "add gitlink"]);
    let worktree = fixture.worktree(&repo, "with-gitlink");
    assert!(git(&worktree, &["ls-files", "--stage"]).contains("160000"));
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &worktree),
        "keep"
    );
}

#[test]
fn invariant_12_single_remove_rechecks_a_lock() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let worktree = fixture.worktree(&repo, "done");
    assert_eq!(
        decision(&cleaner(&repo, &["--base", "main"]), &worktree),
        "candidate"
    );
    git(&repo, &["worktree", "lock", worktree.to_str().unwrap()]);
    let output = cleaner(
        &repo,
        &["--base", "main", "--remove", worktree.to_str().unwrap()],
    );
    if output.status.success() {
        assert_eq!(decision(&output, &worktree), "keep");
    } else {
        assert!(!output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("\tremoved\t"));
    }
    assert!(worktree.exists());
    assert!(git(&repo, &["worktree", "list", "--porcelain"]).contains(worktree.to_str().unwrap()));
}

#[test]
fn invariant_14_empty_parent_has_no_candidates() {
    let fixture = Fixture::new();
    assert!(rows(&cleaner(&fixture.path, &[])).is_empty());
}

#[test]
fn invariant_14_registered_worktree_outside_root_is_discovered() {
    let fixture = Fixture::new();
    let root = fixture.path.join("root");
    fs::create_dir(&root).unwrap();
    let repo = root.join("project");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("tracked"), "initial\n").unwrap();
    git(&repo, &["add", "tracked"]);
    git(&repo, &["commit", "-m", "initial"]);
    let worktree = fixture.worktree(&repo, "outside");
    let output = cleaner(&root, &["--base", "main"]);
    assert_eq!(decision(&output, &worktree), "candidate");
}
