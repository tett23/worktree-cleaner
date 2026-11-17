mod common;

use common::{Fixture, git};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cleaner(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_worktree-cleaner"))
        .current_dir(cwd)
        .env("HOME", cwd)
        .env("XDG_CONFIG_HOME", cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG_PARAMETERS")
        // ADR 0010 I1: fresh worktrees are candidates only without the activity protection.
        .args(["--min-idle", "0"])
        .args(args)
        .output()
        .unwrap()
}

fn row(output: &Output, path: &Path) -> (String, String) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| line.split('\t').map(str::to_owned).collect::<Vec<_>>())
        .find(|fields| fields[1] == path.to_str().unwrap())
        .map(|fields| (fields[2].clone(), fields[3].clone()))
        .unwrap_or_else(|| panic!("missing {}", path.display()))
}

/// A repository whose committed `.gitignore` ignores `target/` and `*.log`.
fn ignoring_repo(fixture: &Fixture) -> PathBuf {
    let repo = fixture.repo("project", "main");
    fs::write(repo.join(".gitignore"), "target/\n*.log\n").unwrap();
    git(&repo, &["add", ".gitignore"]);
    git(&repo, &["commit", "-m", "ignore build output"]);
    repo
}

fn with_ignored_file(fixture: &Fixture, repo: &Path, name: &str) -> PathBuf {
    let worktree = fixture.worktree(repo, name);
    fs::write(worktree.join("build.log"), "log\n").unwrap();
    assert!(git(&worktree, &["status", "--porcelain", "--ignored"]).contains("!! build.log"));
    worktree
}

const LIST: &[&str] = &["--base", "main"];
const ALLOW: &[&str] = &["--base", "main", "--allow-ignored"];

#[test]
fn adr0008_i1_default_decisions_are_unchanged() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let plain = fixture.worktree(&repo, "plain");
    let ignored = with_ignored_file(&fixture, &repo, "ignored");
    let output = cleaner(&repo, LIST);
    assert_eq!(
        row(&output, &plain),
        ("candidate".into(), "clean and integrated".into())
    );
    assert_eq!(row(&output, &ignored).0, "keep");
    let removal = cleaner(&repo, &["--base", "main", "--remove-all"]);
    assert_eq!(row(&removal, &ignored).0, "keep");
    assert!(ignored.join("build.log").exists());
}

#[test]
fn adr0008_i2_i7_ignored_only_worktree_is_candidate_with_count() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let one = with_ignored_file(&fixture, &repo, "one");
    let two = fixture.worktree(&repo, "two");
    fs::create_dir_all(two.join("target/debug")).unwrap();
    fs::write(two.join("target/a"), "a\n").unwrap();
    fs::write(two.join("target/debug/b"), "b\n").unwrap();
    let many = fixture.worktree(&repo, "many");
    fs::create_dir_all(many.join("target/debug")).unwrap();
    fs::write(many.join("target/debug/b"), "b\n").unwrap();
    symlink("debug/b", many.join("target/link")).unwrap();
    symlink("missing", many.join("target/dangling")).unwrap();
    fs::write(many.join("run.log"), "log\n").unwrap();
    let none = fixture.worktree(&repo, "none");
    let output = cleaner(&repo, ALLOW);
    assert_eq!(
        row(&output, &one),
        (
            "candidate".into(),
            "clean and integrated; deletes 1 ignored files".into()
        )
    );
    assert_eq!(
        row(&output, &two),
        (
            "candidate".into(),
            "clean and integrated; deletes 2 ignored files".into()
        )
    );
    assert_eq!(
        row(&output, &many),
        (
            "candidate".into(),
            "clean and integrated; deletes 4 ignored files".into()
        )
    );
    assert_eq!(
        row(&output, &none),
        (
            "candidate".into(),
            "clean and integrated; deletes 0 ignored files".into()
        )
    );
}

#[test]
fn adr0008_i3_unignored_untracked_file_is_kept() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let worktree = with_ignored_file(&fixture, &repo, "mixed");
    fs::write(worktree.join("notes"), "work\n").unwrap();
    let nested = with_ignored_file(&fixture, &repo, "nested");
    fs::create_dir(nested.join("new")).unwrap();
    fs::write(nested.join("new/notes"), "work\n").unwrap();
    let output = cleaner(&repo, ALLOW);
    assert_eq!(row(&output, &worktree).0, "keep");
    assert_eq!(row(&output, &nested).0, "keep");
    let removal = cleaner(
        &repo,
        &["--base", "main", "--allow-ignored", "--remove-all"],
    );
    assert_eq!(row(&removal, &worktree).0, "keep");
    assert!(worktree.join("notes").exists());
}

#[test]
fn adr0008_i4_staged_or_unstaged_change_is_kept() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let staged = with_ignored_file(&fixture, &repo, "staged");
    fs::write(staged.join("tracked"), "changed\n").unwrap();
    git(&staged, &["add", "tracked"]);
    let unstaged = with_ignored_file(&fixture, &repo, "unstaged");
    fs::write(unstaged.join("tracked"), "changed\n").unwrap();
    let output = cleaner(&repo, ALLOW);
    assert_eq!(row(&output, &staged).0, "keep");
    assert_eq!(row(&output, &unstaged).0, "keep");
}

#[test]
fn adr0008_i5_nested_repository_in_ignored_directory_is_kept() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let worktree = fixture.worktree(&repo, "nested");
    let inner = worktree.join("target/inner");
    fs::create_dir_all(&inner).unwrap();
    git(&inner, &["init", "-b", "main"]);
    let output = cleaner(&repo, ALLOW);
    assert_eq!(
        row(&output, &worktree),
        ("keep".into(), "nested repository".into())
    );
}

#[test]
fn adr0008_i6_special_file_in_ignored_directory_is_kept() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let worktree = fixture.worktree(&repo, "fifo");
    fs::create_dir(worktree.join("target")).unwrap();
    let pipe = worktree.join("target/pipe");
    assert!(
        Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap()
            .success()
    );
    let output = cleaner(&repo, ALLOW);
    assert_eq!(
        row(&output, &worktree),
        ("keep".into(), format!("special file: {}", pipe.display()))
    );
}

#[test]
fn adr0008_i2_i8_remove_all_deletes_ignored_files_and_keeps_branch() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let worktree = with_ignored_file(&fixture, &repo, "done");
    fs::create_dir(worktree.join("target")).unwrap();
    fs::write(worktree.join("target/artifact"), "bin\n").unwrap();
    let before = git(&repo, &["show-ref", "refs/heads/done"]);
    let output = cleaner(
        &repo,
        &["--base", "main", "--allow-ignored", "--remove-all"],
    );
    assert_eq!(
        row(&output, &worktree),
        (
            "removed".into(),
            "clean and integrated; deletes 2 ignored files".into()
        )
    );
    assert!(!worktree.exists());
    assert_eq!(git(&repo, &["show-ref", "refs/heads/done"]), before);
}

#[test]
fn adr0008_i2_combines_with_include_unmerged() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let worktree = with_ignored_file(&fixture, &repo, "feature");
    fs::write(worktree.join("committed"), "one\n").unwrap();
    git(&worktree, &["add", "committed"]);
    git(&worktree, &["commit", "-m", "feature"]);
    let output = cleaner(
        &repo,
        &["--base", "main", "--allow-ignored", "--include-unmerged"],
    );
    assert_eq!(
        row(&output, &worktree),
        (
            "candidate".into(),
            "clean unmerged branch feature; deletes 1 ignored files".into()
        )
    );
    assert_eq!(
        row(
            &cleaner(&repo, &["--base", "main", "--include-unmerged"]),
            &worktree
        )
        .0,
        "keep"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn adr0008_i2_combines_with_reclaim_agent_locks() {
    let fixture = Fixture::new();
    let repo = ignoring_repo(&fixture);
    let worktree = with_ignored_file(&fixture, &repo, "topic");
    // This positive PID exceeds macOS's PID range; kill(pid, 0) yields ESRCH.
    git(
        &repo,
        &[
            "worktree",
            "lock",
            "--reason",
            "claude agent topic-worktree (pid 2000000000 start Thu Nov  5 16:31:45 2026)",
            worktree.to_str().unwrap(),
        ],
    );
    let lock = repo.join(".git/worktrees/topic-worktree/locked");
    let without = cleaner(&repo, &["--base", "main", "--reclaim-agent-locks"]);
    assert_eq!(row(&without, &worktree).0, "keep");
    let listing = cleaner(
        &repo,
        &["--base", "main", "--reclaim-agent-locks", "--allow-ignored"],
    );
    let (decision, reason) = row(&listing, &worktree);
    assert_eq!(decision, "candidate");
    assert!(reason.ends_with("; deletes 1 ignored files"), "{reason}");
    assert!(lock.exists());
    let before = git(&repo, &["show-ref", "refs/heads/topic"]);
    let removal = cleaner(
        &repo,
        &[
            "--base",
            "main",
            "--reclaim-agent-locks",
            "--allow-ignored",
            "--remove-all",
        ],
    );
    assert_eq!(row(&removal, &worktree).0, "removed");
    assert!(!worktree.exists());
    assert!(!lock.exists());
    assert_eq!(git(&repo, &["show-ref", "refs/heads/topic"]), before);
}
