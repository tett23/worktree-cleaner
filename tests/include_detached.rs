mod common;

use common::{Fixture, git};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cleaner(cwd: &Path, args: &[&str], bin: Option<&Path>) -> Output {
    let path = std::env::var_os("PATH").unwrap();
    let path = std::env::join_paths(
        bin.map(Path::to_path_buf)
            .into_iter()
            .chain(std::env::split_paths(&path)),
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
        // ADR 0010: fresh fixtures are candidates only without the activity protection.
        .args(["--min-idle", "0"])
        .args(args)
        .output()
        .unwrap()
}

fn row(output: &Output, path: &Path) -> (String, String) {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| line.split('\t').map(str::to_owned).collect::<Vec<_>>())
        .find(|fields| fields[1] == path.to_str().unwrap())
        .map(|fields| (fields[2].clone(), fields[3].clone()))
        .unwrap_or_else(|| {
            panic!(
                "missing {}: {}{}",
                path.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
}

fn rev(cwd: &Path, name: &str) -> String {
    git(cwd, &["rev-parse", name]).trim().to_owned()
}

fn commit(worktree: &Path, content: &str) -> String {
    fs::write(worktree.join("tracked"), content).unwrap();
    git(worktree, &["commit", "-am", content]);
    rev(worktree, "HEAD")
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

fn is_ancestor(repo: &Path, commit: &str, base: &str) -> bool {
    Command::new("git")
        .current_dir(repo)
        .args(["merge-base", "--is-ancestor", commit, base])
        .status()
        .unwrap()
        .success()
}

/// `main` has two commits: `parent` and its child `tip`.
struct Setup {
    fixture: Fixture,
    repo: PathBuf,
    parent: String,
    tip: String,
}

fn setup() -> Setup {
    let fixture = Fixture::new();
    let repo = fixture.repo("project", "main");
    let parent = rev(&repo, "HEAD");
    let tip = commit(&repo, "tip");
    git(&repo, &["update-ref", "refs/remotes/origin/main", &tip]);
    Setup {
        fixture,
        repo,
        parent,
        tip,
    }
}

fn integrated(commit: &str, base: &str) -> (String, String) {
    (
        "candidate".to_owned(),
        format!("clean detached HEAD {commit} is integrated into {base}"),
    )
}

const DETACHED: &[&str] = &["--include-detached"];

#[test]
fn adr0011_i1_without_option_detached_is_kept() {
    let s = setup();
    let tip = detached(&s.fixture, &s.repo, "tip", &s.tip);
    let list = cleaner(&s.repo, &[], None);
    assert!(list.status.success(), "{list:?}");
    assert_eq!(
        row(&list, &tip),
        ("keep".into(), "worktree HEAD is detached".into())
    );
    let single = cleaner(&s.repo, &["--remove", tip.to_str().unwrap()], None);
    assert!(!single.status.success());
    assert!(tip.exists());
    for args in [
        &["--remove-all"][..],
        &["--include-unmerged", "--remove-all"],
        &["--remote-base", "--remove-all"],
    ] {
        let all = cleaner(&s.repo, args, None);
        assert!(all.status.success(), "{all:?}");
        assert_eq!(
            row(&all, &tip),
            ("keep".into(), "worktree HEAD is detached".into())
        );
        assert!(tip.exists());
    }
}

#[test]
fn adr0011_i2_tip_and_ancestor_are_candidates() {
    let s = setup();
    let tip = detached(&s.fixture, &s.repo, "tip", &s.tip);
    let parent = detached(&s.fixture, &s.repo, "parent", &s.parent);
    let list = cleaner(&s.repo, DETACHED, None);
    assert!(list.status.success(), "{list:?}");
    assert_eq!(row(&list, &tip), integrated(&s.tip, "main"));
    assert_eq!(row(&list, &parent), integrated(&s.parent, "main"));
    assert!(tip.exists() && parent.exists());
}

#[test]
fn adr0011_i2_remote_base_measures_against_origin_tracking_branch() {
    let s = setup();
    git(&s.repo, &["reset", "--hard", &s.parent]);
    let fetched = detached(&s.fixture, &s.repo, "fetched", &s.tip);
    let local = cleaner(&s.repo, DETACHED, None);
    assert_eq!(row(&local, &fetched).0, "keep");
    let remote = cleaner(&s.repo, &["--include-detached", "--remote-base"], None);
    assert!(remote.status.success(), "{remote:?}");
    assert_eq!(row(&remote, &fetched), integrated(&s.tip, "origin/main"));
}

#[test]
fn adr0011_i2_i5_removed_commits_stay_reachable_from_base() {
    let s = setup();
    let tip = detached(&s.fixture, &s.repo, "tip", &s.tip);
    let parent = detached(&s.fixture, &s.repo, "parent", &s.parent);
    let single = cleaner(
        &s.repo,
        &["--include-detached", "--remove", tip.to_str().unwrap()],
        None,
    );
    assert!(single.status.success(), "{single:?}");
    assert_eq!(row(&single, &tip).0, "removed");
    assert!(!tip.exists());
    assert!(is_ancestor(&s.repo, &s.tip, "main"));

    let all = cleaner(&s.repo, &["--include-detached", "--remove-all"], None);
    assert!(all.status.success(), "{all:?}");
    assert_eq!(row(&all, &parent), {
        let (_, reason) = integrated(&s.parent, "main");
        ("removed".to_owned(), reason)
    });
    assert!(!parent.exists());
    assert!(is_ancestor(&s.repo, &s.parent, "main"));
    assert!(!git(&s.repo, &["worktree", "list", "--porcelain"]).contains("detached"));
}

#[test]
fn adr0011_i3_commit_ahead_of_base_is_kept() {
    let s = setup();
    let ahead = detached(&s.fixture, &s.repo, "ahead", &s.tip);
    let head = commit(&ahead, "ahead");
    for args in [
        &["--include-detached"][..],
        &["--include-detached", "--include-unmerged"],
        &["--include-detached", "--remote-base"],
        &["--include-detached", "--include-unmerged", "--remove-all"],
    ] {
        let output = cleaner(&s.repo, args, None);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(row(&output, &ahead).0, "keep", "{args:?}");
        assert!(ahead.exists());
    }
    let single = cleaner(
        &s.repo,
        &[
            "--include-detached",
            "--include-unmerged",
            "--remove",
            ahead.to_str().unwrap(),
        ],
        None,
    );
    assert!(!single.status.success());
    assert!(ahead.exists());
    assert_eq!(rev(&ahead, "HEAD"), head);
}

#[test]
fn adr0011_i4_locked_detached_worktree_is_kept() {
    let s = setup();
    let locked = detached(&s.fixture, &s.repo, "locked", &s.parent);
    // Control: a branch worktree with the same kind of lock.
    let branch = s.fixture.worktree(&s.repo, "topic");
    // This positive PID exceeds macOS's PID range; kill(pid, 0) yields ESRCH.
    for (path, name) in [(&locked, "locked-worktree"), (&branch, "topic-worktree")] {
        git(
            &s.repo,
            &[
                "worktree",
                "lock",
                "--reason",
                &format!("claude agent {name} (pid 2000000000 start Thu Nov  5 16:31:45 2026)"),
                path.to_str().unwrap(),
            ],
        );
    }
    let file = s.repo.join(".git/worktrees/locked-worktree/locked");
    let before = fs::read(&file).unwrap();
    for args in [
        &["--include-detached", "--reclaim-agent-locks"][..],
        &[
            "--include-detached",
            "--reclaim-agent-locks",
            "--include-unmerged",
        ],
        &[
            "--include-detached",
            "--reclaim-agent-locks",
            "--remove-all",
        ],
    ] {
        let output = cleaner(&s.repo, args, None);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(row(&output, &locked).0, "keep", "{args:?}");
        assert_eq!(fs::read(&file).unwrap(), before);
        assert!(locked.exists());
    }
    if cfg!(target_os = "macos") {
        assert!(!branch.exists(), "the control lock was not reclaimable");
    }
}

#[test]
fn adr0011_i2_i7_github_merged_does_not_query_detached_worktrees() {
    let s = setup();
    let integrated_tree = detached(&s.fixture, &s.repo, "integrated", &s.parent);
    let ahead = detached(&s.fixture, &s.repo, "ahead", &s.tip);
    let ahead_head = commit(&ahead, "ahead");
    // Control: a divergent branch, which `--github-merged` does query.
    let topic = s.fixture.worktree(&s.repo, "topic");
    let topic_head = commit(&topic, "topic");
    git(
        &s.repo,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:owner/project.git",
        ],
    );
    let bin = s.fixture.path.join("bin");
    fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    let calls = s.fixture.path.join("calls");
    common::write_executable(
        &gh,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n",
            calls.to_str().unwrap()
        ),
    );
    let list = cleaner(
        &s.repo,
        &["--include-detached", "--github-merged"],
        Some(&bin),
    );
    assert!(list.status.success(), "{list:?}");
    assert_eq!(row(&list, &integrated_tree), integrated(&s.parent, "main"));
    assert_eq!(row(&list, &ahead).0, "keep");
    assert_eq!(row(&list, &topic).0, "keep");
    let all = cleaner(
        &s.repo,
        &["--include-detached", "--github-merged", "--remove-all"],
        Some(&bin),
    );
    assert!(all.status.success(), "{all:?}");
    assert_eq!(row(&all, &integrated_tree).0, "removed");
    assert!(!integrated_tree.exists());
    assert!(ahead.exists() && topic.exists());
    let recorded = fs::read_to_string(&calls).unwrap();
    assert_eq!(recorded.lines().count(), 2, "{recorded}");
    assert!(
        recorded.lines().all(|line| line.contains(&topic_head)),
        "{recorded}"
    );
    assert!(!recorded.contains(&ahead_head) && !recorded.contains(&s.parent));
}

#[test]
fn adr0011_i8_worktree_specific_refs_keep_the_worktree() {
    let s = setup();
    let pinned = detached(&s.fixture, &s.repo, "pinned", &s.parent);
    git(&pinned, &["update-ref", "refs/worktree/x", "HEAD"]);
    for args in [
        &["--include-detached"][..],
        &["--include-detached", "--include-unmerged", "--remove-all"],
        &["--include-detached", "--remote-base", "--remove-all"],
    ] {
        let output = cleaner(&s.repo, args, None);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            row(&output, &pinned),
            ("keep".into(), "worktree has worktree-specific refs".into()),
            "{args:?}"
        );
        assert!(pinned.exists());
    }
    let single = cleaner(
        &s.repo,
        &["--include-detached", "--remove", pinned.to_str().unwrap()],
        None,
    );
    assert!(!single.status.success());
    assert!(pinned.exists());
}

#[test]
fn adr0011_i2_allow_ignored_counts_ignored_files_of_detached_worktree() {
    let s = setup();
    fs::write(s.repo.join(".gitignore"), "*.log\n").unwrap();
    git(&s.repo, &["add", ".gitignore"]);
    git(&s.repo, &["commit", "-m", "ignore logs"]);
    let tip = rev(&s.repo, "HEAD");
    let built = detached(&s.fixture, &s.repo, "built", &tip);
    fs::write(built.join("build.log"), "output\n").unwrap();

    let strict = cleaner(&s.repo, DETACHED, None);
    assert!(strict.status.success(), "{strict:?}");
    assert_eq!(row(&strict, &built).0, "keep");

    let allowed = cleaner(&s.repo, &["--include-detached", "--allow-ignored"], None);
    assert!(allowed.status.success(), "{allowed:?}");
    let (_, reason) = integrated(&tip, "main");
    assert_eq!(
        row(&allowed, &built),
        (
            "candidate".to_owned(),
            format!("{reason}; deletes 1 ignored files")
        )
    );

    let removal = cleaner(
        &s.repo,
        &["--include-detached", "--allow-ignored", "--remove-all"],
        None,
    );
    assert!(removal.status.success(), "{removal:?}");
    assert_eq!(row(&removal, &built).0, "removed");
    assert!(!built.exists());
    assert!(is_ancestor(&s.repo, &tip, "main"));
}
