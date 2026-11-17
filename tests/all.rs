mod common;

use common::{Fixture, git};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

/// The options that `--all` stands for, apart from `--min-idle 0` (ADR 0013).
const SIX: &[&str] = &[
    "--include-unmerged",
    "--include-detached",
    "--github-merged",
    "--reclaim-agent-locks",
    "--allow-ignored",
    "--remote-base",
];

/// A repository whose `origin/main` is ahead of the local `main`, with a
/// fake gh on PATH that reports no pull requests.
struct Setup {
    fixture: Fixture,
    repo: PathBuf,
    bare: PathBuf,
    bin: PathBuf,
}

impl Setup {
    fn new() -> Self {
        let fixture = Fixture::new();
        let repo = fixture.repo("project", "main");
        let bare = fixture.path.join("origin.git");
        git(
            &fixture.path,
            &["init", "--bare", "-b", "main", bare.to_str().unwrap()],
        );
        // gh is asked only about github.com remotes, so push and fetch use the bare path.
        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/owner/project.git",
            ],
        );
        git(&repo, &["push", bare.to_str().unwrap(), "main"]);
        fs::write(repo.join(".git/info/exclude"), "build/\n").unwrap();
        let bin = fixture.path.join("bin");
        fs::create_dir(&bin).unwrap();
        common::write_executable(&bin.join("gh"), "#!/bin/sh\nexit 0\n");
        Self {
            fixture,
            repo,
            bare,
            bin,
        }
    }

    /// Publish `commit` as the remote main and fetch it, leaving local main behind.
    fn advance(&self, commit: &str) {
        let bare = self.bare.to_str().unwrap();
        git(
            &self.repo,
            &[
                "push",
                "--force",
                bare,
                &format!("{commit}:refs/heads/main"),
            ],
        );
        git(
            &self.repo,
            &["fetch", bare, "+refs/heads/main:refs/remotes/origin/main"],
        );
    }

    fn worktree(&self, name: &str) -> PathBuf {
        self.fixture.worktree(&self.repo, name)
    }

    fn detached(&self, name: &str, commit: &str) -> PathBuf {
        let path = self.fixture.path.join(format!("{name}-worktree"));
        git(
            &self.repo,
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

    /// Run the cleaner without any implicit option.
    fn run(&self, args: &[&str]) -> Output {
        let path = std::env::var_os("PATH").unwrap();
        let path = std::env::join_paths(
            std::iter::once(self.bin.clone()).chain(std::env::split_paths(&path)),
        )
        .unwrap();
        Command::new(env!("CARGO_BIN_EXE_worktree-cleaner"))
            .current_dir(&self.repo)
            .env("HOME", &self.repo)
            .env("XDG_CONFIG_HOME", &self.repo)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_COUNT", "0")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env("PATH", path)
            .args(args)
            .output()
            .unwrap()
    }

    /// Make the worktree's Git activity look `hours` old.
    fn age(&self, name: &str, hours: u64) {
        let time = SystemTime::now() - Duration::from_secs(hours * 3600);
        let admin = self.repo.join(".git/worktrees").join(name);
        for file in ["HEAD", "index", "logs/HEAD"] {
            let path = admin.join(file);
            if path.exists() {
                fs::File::options()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_modified(time)
                    .unwrap();
            }
        }
    }
}

fn commit(worktree: &Path, content: &str) -> String {
    fs::write(worktree.join("tracked"), content).unwrap();
    git(worktree, &["commit", "-am", content]);
    git(worktree, &["rev-parse", "HEAD"]).trim().to_owned()
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

fn with(base: &[&str], extra: &[&str]) -> Vec<String> {
    base.iter()
        .chain(extra)
        .map(|arg| (*arg).to_owned())
        .collect()
}

fn args(list: &[String]) -> Vec<&str> {
    list.iter().map(String::as_str).collect()
}

/// Worktrees for the first three rows of ADR 0013's boundary table.
struct Rows {
    detached: PathBuf,
    unmerged: PathBuf,
    recent: PathBuf,
}

fn rows(setup: &Setup) -> Rows {
    let fetched = setup.worktree("fetched");
    let head = commit(&fetched, "fetched");
    setup.advance(&head);
    git(
        &setup.repo,
        &["worktree", "remove", fetched.to_str().unwrap()],
    );
    let detached = setup.detached("detached", &head);
    fs::create_dir(detached.join("build")).unwrap();
    fs::write(detached.join("build/out"), "artifact").unwrap();
    setup.age("detached-worktree", 48);
    let unmerged = setup.worktree("unmerged");
    commit(&unmerged, "unmerged");
    setup.age("unmerged-worktree", 48);
    let recent = setup.worktree("recent");
    setup.age("recent-worktree", 1);
    Rows {
        detached,
        unmerged,
        recent,
    }
}

#[test]
fn adr0013_i1_all_equals_the_seven_options() {
    let setup = Setup::new();
    let rows = rows(&setup);
    let seven = with(SIX, &["--min-idle", "0"]);

    let all = setup.run(&["--all"]);
    let expanded = setup.run(&args(&seven));
    assert!(all.status.success(), "{all:?}");
    assert_eq!(all.stdout, expanded.stdout);
    assert_eq!(
        row(&all, &rows.detached).0,
        "candidate",
        "{}",
        row(&all, &rows.detached).1
    );
    assert!(
        row(&all, &rows.detached)
            .1
            .ends_with("; deletes 1 ignored files")
    );
    assert_eq!(
        row(&all, &rows.unmerged).0,
        "candidate",
        "{}",
        row(&all, &rows.unmerged).1
    );
    assert_eq!(row(&all, &rows.recent).0, "candidate");

    let single = setup.run(&["--all", "--remove", rows.unmerged.to_str().unwrap()]);
    assert!(single.status.success(), "{single:?}");
    assert_eq!(row(&single, &rows.unmerged).0, "removed");
    assert!(!rows.unmerged.exists());

    let removal = setup.run(&["--all", "--remove-all"]);
    assert!(removal.status.success(), "{removal:?}");
    assert_eq!(row(&removal, &rows.detached).0, "removed");
    assert_eq!(row(&removal, &rows.recent).0, "removed");
    assert!(!rows.detached.exists());
    assert!(!rows.recent.exists());
    for branch in ["fetched", "unmerged", "recent"] {
        git(
            &setup.repo,
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
        );
    }
}

#[test]
fn adr0013_i2_explicit_min_idle_overrides_all() {
    let setup = Setup::new();
    let rows = rows(&setup);
    for hours in ["24", "0"] {
        let all = setup.run(&["--all", "--min-idle", hours]);
        let expanded = setup.run(&args(&with(SIX, &["--min-idle", hours])));
        assert!(all.status.success(), "{all:?}");
        assert_eq!(all.stdout, expanded.stdout, "--min-idle {hours}");
        let flipped = setup.run(&["--min-idle", hours, "--all"]);
        assert_eq!(all.stdout, flipped.stdout, "--min-idle {hours} first");
    }
    let kept = setup.run(&["--all", "--min-idle", "24"]);
    assert_eq!(row(&kept, &rows.recent).0, "keep");
    assert!(
        row(&kept, &rows.recent)
            .1
            .contains("less than --min-idle 24")
    );
    assert_eq!(row(&kept, &rows.unmerged).0, "candidate");
    let open = setup.run(&["--all", "--min-idle", "0"]);
    assert_eq!(row(&open, &rows.recent).0, "candidate");
}

#[test]
fn adr0013_i3_all_with_an_included_option_is_all() {
    let setup = Setup::new();
    rows(&setup);
    let all = setup.run(&["--all"]);
    assert!(all.status.success(), "{all:?}");
    for option in SIX {
        let after = setup.run(&["--all", option]);
        let before = setup.run(&[option, "--all"]);
        assert!(after.status.success(), "{option}: {after:?}");
        assert_eq!(all.stdout, after.stdout, "--all {option}");
        assert_eq!(all.stdout, before.stdout, "{option} --all");
    }
}

#[test]
fn adr0013_i4_repeated_all_is_rejected() {
    let setup = Setup::new();
    let worktree = setup.worktree("merged");
    let output = setup.run(&["--all", "--all", "--remove-all"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected or repeated option: --all")
    );
    assert!(worktree.exists());
}
