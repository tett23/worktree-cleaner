mod common;

use common::{Fixture, git};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A repository with one clean worktree on an unmerged branch and a fake gh.
///
/// The fake gh follows `$MODE`: `fail` exits 1, `empty` reports no pull
/// request, and `flip` fails on the first call and then reports `$ROW`.
struct Setup {
    fixture: Fixture,
    repo: PathBuf,
    path: PathBuf,
    head: String,
    bin: PathBuf,
}

impl Setup {
    fn new(origin: &str) -> Self {
        let fixture = Fixture::new();
        let repo = fixture.repo("project", "main");
        git(&repo, &["remote", "add", "origin", origin]);
        let path = fixture.worktree(&repo, "topic");
        fs::write(path.join("tracked"), "topic").unwrap();
        git(&path, &["commit", "-am", "topic"]);
        let head = git(&path, &["rev-parse", "HEAD"]).trim().to_owned();
        let bin = fixture.path.join("bin");
        fs::create_dir(&bin).unwrap();
        common::write_executable(
            &bin.join("gh"),
            "#!/bin/sh\n\
             echo call >> \"$CALLS\"\n\
             case \"$MODE\" in\n\
             fail) echo 'gh: boom' >&2; exit 1 ;;\n\
             empty) exit 0 ;;\n\
             flip) if test \"$(wc -l < \"$CALLS\")\" -ge 2; then printf '%s\\n' \"$ROW\"; exit 0; fi; echo 'gh: boom' >&2; exit 1 ;;\n\
             esac\n",
        );
        Self {
            fixture,
            repo,
            path,
            head,
            bin,
        }
    }

    fn github() -> Self {
        Self::new("https://github.com/owner/project.git")
    }

    fn run(&self, mode: &str, args: &[&str]) -> Output {
        let merge = git(&self.repo, &["rev-parse", "main"]).trim().to_owned();
        Command::new(env!("CARGO_BIN_EXE_worktree-cleaner"))
            .current_dir(&self.repo)
            .env(
                "PATH",
                std::env::join_paths(
                    std::iter::once(self.bin.clone())
                        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                )
                .unwrap(),
            )
            .env("MODE", mode)
            .env("CALLS", self.fixture.path.join("calls"))
            .env(
                "ROW",
                format!(
                    "1\t2026-11-11T10:31:27Z\t{}\towner/project\tmain\t{merge}",
                    self.head
                ),
            )
            // ADR 0010 I1: fresh worktrees are candidates only without the activity protection.
            .args(["--min-idle", "0"])
            .args(args)
            .output()
            .unwrap()
    }
}

const BOTH: &[&str] = &["--github-merged", "--include-unmerged"];

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

#[test]
fn adr0014_i1_i2_unsupported_origin_yields_unmerged_candidate() {
    let setup = Setup::new("/srv/git/project.git");
    let out = setup.run("empty", BOTH);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        row(&out, &setup.path),
        (
            "candidate".into(),
            "clean unmerged branch topic; GitHub check failed: origin is not a supported github.com URL"
                .into()
        )
    );
}

#[test]
fn adr0014_i1_i2_failed_gh_yields_unmerged_candidate() {
    let setup = Setup::github();
    let out = setup.run("fail", BOTH);
    assert!(out.status.success(), "{out:?}");
    let (decision, reason) = row(&out, &setup.path);
    assert_eq!(decision, "candidate", "{reason}");
    assert!(
        reason
            .starts_with("clean unmerged branch topic; GitHub check failed: GitHub query failed: "),
        "{reason}"
    );
    assert!(reason.contains("gh: boom"), "{reason}");
}

#[test]
fn adr0014_i1_i2_failure_reason_precedes_ignored_count() {
    let setup = Setup::github();
    let out = setup.run("fail", &[BOTH, &["--allow-ignored"]].concat());
    let (decision, reason) = row(&out, &setup.path);
    assert_eq!(decision, "candidate", "{reason}");
    assert!(reason.contains("; GitHub check failed: "), "{reason}");
    assert!(reason.ends_with("; deletes 0 ignored files"), "{reason}");
}

#[test]
fn adr0014_i1_successful_query_without_pr_is_unchanged() {
    let setup = Setup::github();
    let out = setup.run("empty", BOTH);
    assert_eq!(
        row(&out, &setup.path),
        ("candidate".into(), "clean unmerged branch topic".into())
    );
}

// The lock owner is decidable only on macOS (ADR 0003).
#[cfg(target_os = "macos")]
#[test]
fn adr0014_i3_locked_worktree_with_failed_query_is_kept() {
    let setup = Setup::new("/srv/git/project.git");
    git(
        &setup.repo,
        &[
            "worktree",
            "lock",
            "--reason",
            "claude agent topic-worktree (pid 2000000000 start Thu Nov  5 16:31:45 2026)",
            setup.path.to_str().unwrap(),
        ],
    );
    let out = setup.run("empty", &[BOTH, &["--reclaim-agent-locks"]].concat());
    let (decision, reason) = row(&out, &setup.path);
    assert_eq!(decision, "keep", "{reason}");
    assert!(
        reason.contains("origin is not a supported github.com URL"),
        "{reason}"
    );
}

#[test]
fn adr0014_i4_without_include_unmerged_failure_keeps() {
    let setup = Setup::new("/srv/git/project.git");
    let out = setup.run("empty", &["--github-merged"]);
    assert_eq!(
        row(&out, &setup.path),
        (
            "keep".into(),
            "origin is not a supported github.com URL".into()
        )
    );
}

#[test]
fn adr0014_i5_evidence_found_at_recheck_keeps() {
    let setup = Setup::github();
    let out = setup.run("flip", &[BOTH, &["--remove-all"]].concat());
    let (decision, reason) = row(&out, &setup.path);
    assert_eq!(decision, "keep", "{reason}");
    assert!(setup.path.exists());
}

#[test]
fn adr0014_i6_removal_retains_branch() {
    let setup = Setup::github();
    let out = setup.run("fail", &[BOTH, &["--remove-all"]].concat());
    assert!(out.status.success(), "{out:?}");
    assert_eq!(row(&out, &setup.path).0, "removed");
    assert!(!setup.path.exists());
    assert_eq!(
        git(&setup.repo, &["rev-parse", "refs/heads/topic"]).trim(),
        setup.head
    );
}
