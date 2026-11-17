mod common;

use common::{Fixture, git};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
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
        // ADR 0010 I1: fresh worktrees are candidates only without the activity protection.
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

fn commit(worktree: &Path, content: &str) -> String {
    fs::write(worktree.join("tracked"), content).unwrap();
    git(worktree, &["commit", "-am", content]);
    git(worktree, &["rev-parse", "HEAD"]).trim().to_owned()
}

/// A repository whose `origin` is a bare repository. `origin/main` has been
/// advanced and fetched, while the local `main` stays at the initial commit.
struct Remote {
    fixture: Fixture,
    repo: PathBuf,
}

impl Remote {
    fn new(name: &str) -> Self {
        let fixture = Fixture::new();
        let repo = fixture.repo(name, "main");
        let bare = fixture.path.join(format!("{name}-origin.git"));
        git(
            &fixture.path,
            &["init", "--bare", "-b", "main", bare.to_str().unwrap()],
        );
        git(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(&repo, &["push", "origin", "main"]);
        Self { fixture, repo }
    }

    /// Publish `commit` as the remote main and fetch it, leaving local main behind.
    fn advance(&self, commit: &str) {
        git(
            &self.repo,
            &[
                "push",
                "--force",
                "origin",
                &format!("{commit}:refs/heads/main"),
            ],
        );
        git(&self.repo, &["fetch", "origin"]);
    }

    fn worktree(&self, name: &str) -> PathBuf {
        self.fixture.worktree(&self.repo, name)
    }
}

const REMOTE: &[&str] = &["--remote-base"];

#[test]
fn adr0009_i1_without_option_decisions_are_unchanged() {
    let remote = Remote::new("project");
    let merged = remote.worktree("merged");
    let fetched = remote.worktree("fetched");
    let head = commit(&fetched, "fetched");
    remote.advance(&head);
    assert_ne!(
        git(&remote.repo, &["rev-parse", "main"]).trim(),
        git(&remote.repo, &["rev-parse", "origin/main"]).trim()
    );

    let list = cleaner(&remote.repo, &[], None);
    assert!(list.status.success());
    assert_eq!(
        row(&list, &merged),
        ("candidate".into(), "clean and integrated".into())
    );
    assert_eq!(row(&list, &fetched).0, "keep");

    let single = cleaner(&remote.repo, &["--remove", fetched.to_str().unwrap()], None);
    assert!(!single.status.success());
    assert!(fetched.exists());

    let all = cleaner(&remote.repo, &["--remove-all"], None);
    assert!(all.status.success());
    assert_eq!(row(&all, &merged).0, "removed");
    assert_eq!(row(&all, &fetched).0, "keep");
    assert!(!merged.exists());
    assert!(fetched.exists());
}

#[test]
fn adr0009_i2_remote_base_uses_origin_tracking_branch() {
    let remote = Remote::new("project");
    let behind = remote.worktree("behind");
    let tip = remote.worktree("tip");
    let ahead = remote.worktree("ahead");
    let behind_head = commit(&behind, "behind");
    git(&tip, &["reset", "--hard", &behind_head]);
    let tip_head = commit(&tip, "tip");
    git(&ahead, &["reset", "--hard", &tip_head]);
    commit(&ahead, "ahead");
    remote.advance(&tip_head);

    let list = cleaner(&remote.repo, REMOTE, None);
    assert!(list.status.success(), "{list:?}");
    let integrated = (
        "candidate".to_owned(),
        "clean and integrated into origin/main".to_owned(),
    );
    assert_eq!(row(&list, &behind), integrated);
    assert_eq!(row(&list, &tip), integrated);
    assert_eq!(row(&list, &ahead).0, "keep");

    let explicit = cleaner(&remote.repo, &["--remote-base", "--base", "main"], None);
    assert_eq!(row(&explicit, &behind), integrated);
    assert_eq!(row(&explicit, &ahead).0, "keep");

    let single = cleaner(
        &remote.repo,
        &["--remote-base", "--remove", behind.to_str().unwrap()],
        None,
    );
    assert!(single.status.success(), "{single:?}");
    assert_eq!(row(&single, &behind).0, "removed");
    assert!(!behind.exists());

    let refused = cleaner(
        &remote.repo,
        &["--remote-base", "--remove", ahead.to_str().unwrap()],
        None,
    );
    assert!(!refused.status.success());
    assert!(ahead.exists());

    let all = cleaner(&remote.repo, &["--remote-base", "--remove-all"], None);
    assert!(all.status.success(), "{all:?}");
    assert_eq!(row(&all, &tip).0, "removed");
    assert_eq!(row(&all, &ahead).0, "keep");
    assert!(!tip.exists());
    assert!(ahead.exists());
    assert_eq!(
        git(&remote.repo, &["rev-parse", "tip"]).trim(),
        tip_head.as_str()
    );
}

#[test]
fn adr0009_i2_local_base_branch_is_not_required() {
    let remote = Remote::new("project");
    let fetched = remote.worktree("fetched");
    let head = commit(&fetched, "fetched");
    remote.advance(&head);
    git(&remote.repo, &["switch", "-c", "other"]);
    git(&remote.repo, &["branch", "-D", "main"]);
    let list = cleaner(&remote.repo, REMOTE, None);
    assert_eq!(
        row(&list, &fetched),
        (
            "candidate".into(),
            "clean and integrated into origin/main".into()
        )
    );
}

#[test]
fn adr0009_i2_include_unmerged_measures_against_origin_tracking_branch() {
    let remote = Remote::new("project");
    let fetched = remote.worktree("fetched");
    let ahead = remote.worktree("ahead");
    let head = commit(&fetched, "fetched");
    remote.advance(&head);
    git(&ahead, &["reset", "--hard", &head]);
    commit(&ahead, "ahead");
    let list = cleaner(&remote.repo, &["--remote-base", "--include-unmerged"], None);
    assert_eq!(
        row(&list, &fetched),
        (
            "candidate".into(),
            "clean and integrated into origin/main".into()
        )
    );
    assert_eq!(
        row(&list, &ahead),
        ("candidate".into(), "clean unmerged branch ahead".into())
    );
}

#[test]
fn adr0009_i3_base_branch_worktree_is_kept() {
    let remote = Remote::new("project");
    let fetched = remote.worktree("fetched");
    let head = commit(&fetched, "fetched");
    remote.advance(&head);
    git(&remote.repo, &["switch", "-c", "other"]);
    let base = remote.fixture.path.join("main-worktree");
    git(
        &remote.repo,
        &["worktree", "add", base.to_str().unwrap(), "main"],
    );
    let list = cleaner(&remote.repo, REMOTE, None);
    assert_eq!(
        row(&list, &base),
        ("keep".into(), "worktree uses base branch".into())
    );
    let all = cleaner(&remote.repo, &["--remote-base", "--remove-all"], None);
    assert!(all.status.success(), "{all:?}");
    assert_eq!(row(&all, &base).0, "keep");
    assert!(base.exists());
}

#[test]
fn adr0009_i4_missing_tracking_branch_keeps_only_that_repository() {
    let fixture = Fixture::new();
    let missing = fixture.repo("missing", "main");
    let missing_tree = fixture.worktree(&missing, "missing-topic");
    git(&missing, &["update-ref", "-d", "refs/remotes/origin/main"]);
    let present = fixture.repo("present", "main");
    let present_tree = fixture.worktree(&present, "present-topic");
    let head = commit(&present_tree, "present");
    git(&present, &["update-ref", "refs/remotes/origin/main", &head]);

    let list = cleaner(&fixture.path, REMOTE, None);
    assert!(list.status.success(), "{list:?}");
    let (decision, reason) = row(&list, &missing_tree);
    assert_eq!(decision, "keep");
    assert!(reason.contains("refs/remotes/origin/main"), "{reason}");
    assert_eq!(
        row(&list, &present_tree),
        (
            "candidate".into(),
            "clean and integrated into origin/main".into()
        )
    );

    let all = cleaner(&fixture.path, &["--remote-base", "--remove-all"], None);
    assert!(all.status.success(), "{all:?}");
    assert!(missing_tree.exists());
    assert!(!present_tree.exists());
}

/// A fake gh that records each invocation and answers with `$ROW`.
fn fake_gh(fixture: &Fixture) -> (PathBuf, PathBuf) {
    let bin = fixture.path.join("bin");
    fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$CALLS\"\nprintf '%s\\n' \"$ROW\"\n",
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    (bin, fixture.path.join("calls"))
}

fn with_gh(cwd: &Path, args: &[&str], bin: &Path, calls: &Path, head: &str, merge: &str) -> Output {
    let path = std::env::var_os("PATH").unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&path)),
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_worktree-cleaner"))
        .current_dir(cwd)
        .env("PATH", path)
        .env("CALLS", calls)
        .env(
            "ROW",
            format!("1\t2026-11-11T10:31:27Z\t{head}\towner/project\tmain\t{merge}"),
        )
        // ADR 0010 I1: fresh worktrees are candidates only without the activity protection.
        .args(["--min-idle", "0"])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn adr0009_i5_remote_base_alone_does_not_run_gh() {
    let remote = Remote::new("project");
    let fetched = remote.worktree("fetched");
    let ahead = remote.worktree("ahead");
    let head = commit(&fetched, "fetched");
    remote.advance(&head);
    git(&ahead, &["reset", "--hard", &head]);
    commit(&ahead, "ahead");
    git(
        &remote.repo,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:owner/project.git",
        ],
    );
    let (bin, calls) = fake_gh(&remote.fixture);
    for args in [
        &["--remote-base"][..],
        &["--remote-base", "--include-unmerged"],
        &["--remote-base", "--remove-all"],
    ] {
        let output = cleaner(&remote.repo, args, Some(&bin));
        assert!(output.status.success(), "{output:?}");
    }
    assert!(!calls.exists());
    assert!(!fetched.exists());
    assert!(ahead.exists());
}

/// A squash-merged topic whose merge commit sits on the local main only.
#[test]
fn adr0009_i6_merge_commit_only_on_local_base_is_not_evidence() {
    let remote = Remote::new("project");
    let topic = remote.worktree("topic");
    let head = commit(&topic, "topic");
    git(&remote.repo, &["merge", "--squash", "topic"]);
    git(&remote.repo, &["commit", "-m", "integrate topic"]);
    let merge = git(&remote.repo, &["rev-parse", "HEAD"]).trim().to_owned();
    git(
        &remote.repo,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:owner/project.git",
        ],
    );
    let (bin, calls) = fake_gh(&remote.fixture);

    let local = with_gh(
        &remote.repo,
        &["--github-merged"],
        &bin,
        &calls,
        &head,
        &merge,
    );
    assert_eq!(row(&local, &topic).0, "candidate");

    let remote_list = with_gh(
        &remote.repo,
        &["--remote-base", "--github-merged"],
        &bin,
        &calls,
        &head,
        &merge,
    );
    assert!(remote_list.status.success(), "{remote_list:?}");
    assert_eq!(row(&remote_list, &topic).0, "keep");

    let removal = with_gh(
        &remote.repo,
        &["--remote-base", "--github-merged", "--remove-all"],
        &bin,
        &calls,
        &head,
        &merge,
    );
    assert!(removal.status.success(), "{removal:?}");
    assert!(topic.exists());
}

/// A squash-merged topic whose merge commit sits on origin/main only.
#[test]
fn adr0009_i8_merge_commit_on_tracking_branch_is_evidence() {
    let remote = Remote::new("project");
    let topic = remote.worktree("topic");
    let head = commit(&topic, "topic");
    git(&remote.repo, &["merge", "--squash", "topic"]);
    git(&remote.repo, &["commit", "-m", "integrate topic"]);
    let merge = git(&remote.repo, &["rev-parse", "HEAD"]).trim().to_owned();
    remote.advance(&merge);
    git(&remote.repo, &["reset", "--hard", "HEAD~1"]);
    git(
        &remote.repo,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:owner/project.git",
        ],
    );
    let (bin, calls) = fake_gh(&remote.fixture);

    let local = with_gh(
        &remote.repo,
        &["--github-merged"],
        &bin,
        &calls,
        &head,
        &merge,
    );
    assert_eq!(row(&local, &topic).0, "keep");

    fs::remove_file(&calls).unwrap();
    let list = with_gh(
        &remote.repo,
        &["--remote-base", "--github-merged"],
        &bin,
        &calls,
        &head,
        &merge,
    );
    assert!(list.status.success(), "{list:?}");
    assert_eq!(
        row(&list, &topic),
        (
            "candidate".into(),
            "clean and integrated via https://github.com/owner/project/pull/1".into()
        )
    );
    // The PR row names base `main`; gh's answer is accepted only for that name.
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 1);

    let removal = with_gh(
        &remote.repo,
        &["--remote-base", "--github-merged", "--remove-all"],
        &bin,
        &calls,
        &head,
        &merge,
    );
    assert!(removal.status.success(), "{removal:?}");
    assert!(!topic.exists());
    assert_eq!(git(&remote.repo, &["rev-parse", "topic"]).trim(), head);
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            if metadata.is_dir() {
                pending.push(entry.path());
                files.insert(entry.path(), Vec::new());
            } else {
                files.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

#[test]
fn adr0009_i7_listing_changes_nothing() {
    let remote = Remote::new("project");
    let fetched = remote.worktree("fetched");
    let ahead = remote.worktree("ahead");
    let head = commit(&fetched, "fetched");
    remote.advance(&head);
    git(&ahead, &["reset", "--hard", &head]);
    commit(&ahead, "ahead");
    let missing = remote.fixture.repo("missing", "main");
    let missing_tree = remote.fixture.worktree(&missing, "missing-topic");
    git(&missing, &["update-ref", "-d", "refs/remotes/origin/main"]);

    let refs = |repo: &Path| git(repo, &["for-each-ref"]);
    let registrations = |repo: &Path| git(repo, &["worktree", "list", "--porcelain"]);
    let before = (
        refs(&remote.repo),
        registrations(&remote.repo),
        refs(&missing),
        registrations(&missing),
        snapshot(&remote.fixture.path),
    );
    for args in [
        &["--remote-base"][..],
        &["--remote-base", "--include-unmerged"],
        &["--remote-base", "--base", "main"],
    ] {
        let output = cleaner(&remote.fixture.path, args, None);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(row(&output, &fetched).0, "candidate");
        assert_eq!(row(&output, &missing_tree).0, "keep");
    }
    let after = (
        refs(&remote.repo),
        registrations(&remote.repo),
        refs(&missing),
        registrations(&missing),
        snapshot(&remote.fixture.path),
    );
    assert_eq!(before, after);
}
