use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

pub struct Fixture {
    pub path: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("worktree-cleaner-test-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self {
            path: path.canonicalize().unwrap(),
        }
    }

    pub fn repo(&self, name: &str, branch: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::create_dir(&path).unwrap();
        git(&path, &["init", "-b", branch]);
        git(&path, &["config", "user.email", "test@example.com"]);
        git(&path, &["config", "user.name", "Test"]);
        fs::write(path.join("tracked"), "initial\n").unwrap();
        git(&path, &["add", "tracked"]);
        git(&path, &["commit", "-m", "initial"]);
        git(
            &path,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                "HEAD",
            ],
        );
        git(
            &path,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                &format!("refs/remotes/origin/{branch}"),
            ],
        );
        path
    }

    pub fn worktree(&self, repo: &Path, name: &str) -> PathBuf {
        let path = self.path.join(format!("{name}-worktree"));
        git(
            repo,
            &["worktree", "add", "-b", name, path.to_str().unwrap()],
        );
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
        .env("HOME", cwd)
        .env("XDG_CONFIG_HOME", cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
