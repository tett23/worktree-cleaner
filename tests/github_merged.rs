mod common;

use common::{Fixture, git};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn adr0003_invariants_1_4_5_squashed_pr_requires_network_opt_in_and_fresh_evidence() {
    let f = Fixture::new();
    let repo = f.repo("project", "main");
    let path = f.worktree(&repo, "topic");
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:owner/project.git",
        ],
    );
    fs::write(path.join("tracked"), "topic").unwrap();
    git(&path, &["commit", "-am", "topic"]);
    let head = git(&path, &["rev-parse", "HEAD"]);
    git(&repo, &["merge", "--squash", "topic"]);
    git(&repo, &["commit", "-m", "integrate topic"]);
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    let bin = f.path.join("bin");
    fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(&gh,"#!/bin/sh\necho call >> \"$CALLS\"\nif test -f \"$CHANGED\" && test \"$(wc -l < \"$CALLS\")\" -ge 2; then exit 0; fi\nprintf '%s\\n' \"$ROW\"\n").unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    let calls = f.path.join("calls");
    let changed = f.path.join("changed");
    let invoke = |args: &[&str], sha: &str| {
        Command::new(env!("CARGO_BIN_EXE_worktree-cleaner"))
            .args(args)
            .current_dir(&repo)
            .env(
                "PATH",
                std::env::join_paths(
                    std::iter::once(bin.clone())
                        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                )
                .unwrap(),
            )
            .env("CALLS", &calls)
            .env("CHANGED", &changed)
            .env(
                "ROW",
                format!(
                    "1\t2026-11-11T10:31:27Z\t{}\towner/project\tmain\t{}",
                    head.trim(),
                    sha
                ),
            )
            .output()
            .unwrap()
    };
    let out = invoke(&[], merge.trim());
    assert!(out.status.success());
    assert!(!calls.exists());
    fs::write(path.join("private"), "mine").unwrap();
    let out = invoke(&["--github-merged"], merge.trim());
    assert!(out.status.success());
    assert!(!calls.exists());
    fs::remove_file(path.join("private")).unwrap();
    let out = invoke(&["--github-merged"], head.trim());
    assert!(String::from_utf8_lossy(&out.stdout).contains("\tkeep\t"));
    let out = invoke(&["--github-merged"], merge.trim());
    let report = String::from_utf8(out.stdout).unwrap();
    assert!(
        report.contains("\tcandidate\t")
            && report.contains("https://github.com/owner/project/pull/1"),
        "{report}"
    );
    fs::write(&changed, "").unwrap();
    fs::remove_file(&calls).unwrap();
    let out = invoke(&["--github-merged", "--remove-all"], merge.trim());
    assert!(!out.status.success());
    assert!(path.exists());
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 2);
    fs::remove_file(&changed).unwrap();
    let out = invoke(&["--github-merged", "--remove-all"], merge.trim());
    assert!(out.status.success());
    assert!(!path.exists());
    assert_eq!(git(&repo, &["rev-parse", "topic"]), head);
}
