mod common;

use common::{Fixture, git};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cleaner(cwd: &Path, args: &[&str], bin: Option<&Path>) -> Output {
    let path = std::env::join_paths(
        bin.map(Path::to_path_buf)
            .into_iter()
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_worktree-cleaner"))
        .current_dir(cwd)
        .env("HOME", cwd)
        .env("XDG_CONFIG_HOME", cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env("PATH", path)
        .args(["--min-idle", "0"])
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

/// Returns the decision and reason of the row for `path`.
fn row(output: &Output, path: &Path) -> (String, String) {
    let fields = rows(output)
        .into_iter()
        .find(|fields| fields[1] == path.to_str().unwrap())
        .unwrap_or_else(|| {
            panic!(
                "missing {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stdout)
            )
        });
    (fields[2].clone(), fields[3].clone())
}

fn kept(output: &Output, path: &Path) -> String {
    let (decision, reason) = row(output, path);
    assert_eq!(decision, "keep", "{}: {reason}", path.display());
    reason
}

#[test]
fn adr0007_i1_i2_i3_status_reason_names_the_first_path_of_the_highest_kind() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    fs::write(repo.join(".git/info/exclude"), "target/\naaa/\n").unwrap();
    let clean = fixture.worktree(&repo, "clean");

    let ignored = fixture.worktree(&repo, "ignored");
    fs::create_dir(ignored.join("target")).unwrap();
    fs::write(ignored.join("target/out"), "artifact\n").unwrap();

    let two_ignored = fixture.worktree(&repo, "two-ignored");
    for dir in ["target", "aaa"] {
        fs::create_dir(two_ignored.join(dir)).unwrap();
        fs::write(two_ignored.join(dir).join("out"), "artifact\n").unwrap();
    }

    let untracked = fixture.worktree(&repo, "untracked");
    fs::write(untracked.join("a"), "work\n").unwrap();
    fs::create_dir(untracked.join("target")).unwrap();
    fs::write(untracked.join("target/out"), "artifact\n").unwrap();

    let staged = fixture.worktree(&repo, "staged");
    fs::write(staged.join("b"), "work\n").unwrap();
    git(&staged, &["add", "b"]);
    fs::write(staged.join("a"), "work\n").unwrap();

    let modified = fixture.worktree(&repo, "modified");
    fs::write(modified.join("tracked"), "changed\n").unwrap();
    fs::write(modified.join("a"), "work\n").unwrap();

    let directory = fixture.worktree(&repo, "directory");
    fs::create_dir(directory.join("d")).unwrap();
    fs::write(directory.join("d/x"), "work\n").unwrap();
    fs::write(directory.join("d/y"), "work\n").unwrap();

    let output = cleaner(&repo, &[], None);
    assert_eq!(row(&output, &clean).0, "candidate");
    assert_eq!(kept(&output, &ignored), "ignored file: target/");
    assert_eq!(kept(&output, &two_ignored), "ignored file: aaa/");
    assert_eq!(kept(&output, &untracked), "untracked file: a");
    assert_eq!(kept(&output, &staged), "tracked changes: b");
    assert_eq!(kept(&output, &modified), "tracked changes: tracked");
    assert_eq!(kept(&output, &directory), "untracked file: d/x");

    // With --allow-ignored (docs/adr/0008), ignored entries are not changes
    // and the rest keeps ADR 0007's order.
    let output = cleaner(&repo, &["--allow-ignored"], None);
    assert_eq!(row(&output, &ignored).0, "candidate");
    assert_eq!(row(&output, &two_ignored).0, "candidate");
    assert_eq!(kept(&output, &untracked), "untracked file: a");
    assert_eq!(kept(&output, &staged), "tracked changes: b");
    assert_eq!(kept(&output, &directory), "untracked file: d/x");
}

fn rev(cwd: &Path, name: &str) -> String {
    git(cwd, &["rev-parse", name]).trim().to_owned()
}

fn detached(fixture: &Fixture, repo: &Path, name: &str, commit: &str) -> PathBuf {
    let path = fixture.path.join(format!("{name}-worktree"));
    git(
        repo,
        &[
            "worktree",
            "add",
            "--detach",
            path.to_str().unwrap(),
            commit,
        ],
    );
    path
}

fn remove_gitlink(path: &Path) {
    fs::remove_file(path.join(".git")).unwrap();
}

#[test]
fn adr0007_i4_i5_i6_i12_missing_gitlink_or_directory_explains_head() {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let first = rev(&repo, "HEAD");
    // A commit that only origin/main reaches.
    fs::write(repo.join("remote"), "remote\n").unwrap();
    git(&repo, &["add", "remote"]);
    git(&repo, &["commit", "-m", "remote"]);
    let remote = rev(&repo, "HEAD");
    git(&repo, &["update-ref", "refs/remotes/origin/main", &remote]);
    git(&repo, &["reset", "--hard", &first]);

    // Every other branch sorts after main, so main is the smallest reaching
    // ref of a detached HEAD at the first commit.
    let branch = fixture.worktree(&repo, "x");
    remove_gitlink(&branch);

    let gone = fixture.worktree(&repo, "y-gone");
    remove_gitlink(&gone);
    git(&repo, &["update-ref", "-d", "refs/heads/y-gone"]);

    let locked = fixture.worktree(&repo, "z-locked");
    git(&repo, &["worktree", "lock", locked.to_str().unwrap()]);
    remove_gitlink(&locked);

    let remote_only = detached(&fixture, &repo, "remote-only", &remote);
    remove_gitlink(&remote_only);

    let both = detached(&fixture, &repo, "both", &first);
    remove_gitlink(&both);

    let lost = detached(&fixture, &repo, "lost", &first);
    fs::write(lost.join("lost"), "lost\n").unwrap();
    git(&lost, &["add", "lost"]);
    git(&lost, &["commit", "-m", "lost"]);
    let lost_commit = rev(&lost, "HEAD");
    remove_gitlink(&lost);

    let missing = fixture.worktree(&repo, "w-missing");
    fs::remove_dir_all(&missing).unwrap();

    let missing_locked = fixture.worktree(&repo, "w-missing-locked");
    git(
        &repo,
        &["worktree", "lock", missing_locked.to_str().unwrap()],
    );
    fs::remove_dir_all(&missing_locked).unwrap();

    let unreadable = fixture.worktree(&repo, "u-unreadable");
    remove_gitlink(&unreadable);
    let head = repo.join(".git/worktrees/u-unreadable-worktree/HEAD");
    fs::set_permissions(&head, fs::Permissions::from_mode(0o000)).unwrap();

    let invalid = fixture.worktree(&repo, "v-invalid");
    remove_gitlink(&invalid);
    fs::write(
        repo.join(".git/worktrees/v-invalid-worktree/HEAD"),
        "garbage\n",
    )
    .unwrap();

    let output = cleaner(&repo, &["--remove-all"], None);
    fs::set_permissions(&head, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(
        kept(&output, &branch),
        "worktree .git file is missing; HEAD is branch x"
    );
    assert_eq!(
        kept(&output, &gone),
        "worktree .git file is missing; HEAD is missing branch y-gone"
    );
    assert_eq!(
        kept(&output, &locked),
        "worktree .git file is missing; HEAD is branch z-locked"
    );
    assert_eq!(
        kept(&output, &remote_only),
        format!(
            "worktree .git file is missing; detached HEAD {remote} is reachable from origin/main"
        )
    );
    assert_eq!(
        kept(&output, &both),
        format!("worktree .git file is missing; detached HEAD {first} is reachable from main")
    );
    assert_eq!(
        kept(&output, &lost),
        format!(
            "worktree .git file is missing; detached HEAD {lost_commit} is not reachable from any branch or remote-tracking branch"
        )
    );
    assert_eq!(
        kept(&output, &missing),
        "worktree directory is missing; HEAD is branch w-missing"
    );
    assert_eq!(
        kept(&output, &missing_locked),
        "worktree directory is missing; HEAD is branch w-missing-locked"
    );
    for path in [&unreadable, &invalid] {
        let reason = kept(&output, path);
        assert!(
            reason.starts_with("worktree .git file is missing; cannot inspect HEAD: "),
            "{reason}"
        );
    }
    for path in [&branch, &gone, &locked, &remote_only, &both, &lost] {
        assert!(path.exists(), "{}", path.display());
    }
}

/// A repository whose `topic` worktree diverges from main, and a fake gh
/// whose behavior `script` sets from the worktree HEAD.
fn github_fixture(script: impl Fn(&str) -> String) -> (Fixture, PathBuf, PathBuf, PathBuf) {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let path = fixture.worktree(&repo, "topic");
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:owner/project.git",
        ],
    );
    fs::write(path.join("tracked"), "topic\n").unwrap();
    git(&path, &["commit", "-am", "topic"]);
    let head = rev(&path, "HEAD");
    let bin = fixture.path.join("bin");
    fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    common::write_executable(&gh, &format!("#!/bin/sh\n{}\n", script(&head)));
    (fixture, repo, path, bin)
}

#[test]
fn adr0007_i9_missing_merge_commit_names_commit_and_pr() {
    let merge = "0123456789abcdef0123456789abcdef01234567";
    let (_fixture, repo, path, bin) = github_fixture(|head| {
        format!("printf '1\\t2026-11-11T10:31:27Z\\t{head}\\towner/project\\tmain\\t{merge}\\n'")
    });
    let output = cleaner(&repo, &["--github-merged"], Some(&bin));
    assert_eq!(
        kept(&output, &path),
        format!(
            "merge commit {merge} of https://github.com/owner/project/pull/1 is not in the local repository; fetch origin to check it"
        )
    );
}

#[test]
fn adr0007_i8_i10_gh_stderr_line_keeps_four_columns() {
    let (_fixture, repo, path, bin) = github_fixture(|_| {
        "printf '\\n  HTTP 422:\\tNo commit found for SHA  \\nsecond\\n' >&2; exit 1".into()
    });
    let output = cleaner(&repo, &["--github-merged"], Some(&bin));
    assert_eq!(
        kept(&output, &path),
        "GitHub query failed: exit status: 1: HTTP 422:\\tNo commit found for SHA"
    );
}
