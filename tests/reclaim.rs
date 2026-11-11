mod common;

use common::{Fixture, git};
use std::fs;
use std::path::Path;
use std::process::Command;

fn invoke(repo: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_worktree-cleaner"))
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn adr0003_invariants_2_3_preserve_unknown_lock_even_with_all_options() {
    let f = Fixture::new();
    let repo = f.repo("project", "main");
    let path = f.worktree(&repo, "topic");
    git(
        &repo,
        &[
            "worktree",
            "lock",
            "--reason",
            "external drive",
            path.to_str().unwrap(),
        ],
    );
    let file = repo.join(".git/worktrees/topic-worktree/locked");
    let before = fs::read(&file).unwrap();
    let out = invoke(
        &repo,
        &[
            "--reclaim-agent-locks",
            "--include-unmerged",
            "--github-merged",
            "--remove-all",
        ],
    );
    assert!(out.contains("\tkeep\t"), "{out}");
    assert_eq!(fs::read(file).unwrap(), before);
    assert!(path.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn adr0003_invariants_2_3_5_reclaim_only_clean_integrated_worktree() {
    let f = Fixture::new();
    let repo = f.repo("project", "main");
    let path = f.worktree(&repo, "topic");
    // This positive PID exceeds macOS's PID range; kill(pid, 0) yields ESRCH.
    git(
        &repo,
        &[
            "worktree",
            "lock",
            "--reason",
            "claude agent topic-worktree (pid 2000000000 start Thu Nov  5 16:31:45 2026)",
            path.to_str().unwrap(),
        ],
    );
    let file = repo.join(".git/worktrees/topic-worktree/locked");
    let before = fs::read(&file).unwrap();
    let out = invoke(&repo, &["--reclaim-agent-locks"]);
    assert!(out.contains("\tcandidate\t"), "{out}");
    assert_eq!(fs::read(&file).unwrap(), before);
    fs::write(path.join("new"), "untracked").unwrap();
    let out = invoke(&repo, &["--reclaim-agent-locks", "--remove-all"]);
    assert!(out.contains("\tkeep\t"), "{out}");
    assert_eq!(fs::read(&file).unwrap(), before);
    fs::remove_file(path.join("new")).unwrap();
    let out = invoke(&repo, &["--reclaim-agent-locks", "--remove-all"]);
    assert!(out.contains("\tremoved\t"), "{out}");
    assert!(!path.exists());
    assert!(!file.exists());
    git(&repo, &["show-ref", "--verify", "refs/heads/topic"]);
}
