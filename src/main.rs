mod git;
mod github;
mod locks;

use std::collections::BTreeSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str =
    "Usage: worktree-cleaner [--root DIR] [--base BRANCH] [--include-unmerged] [--github-merged] [--reclaim-agent-locks] [--remove PATH | --remove-all]

List linked worktrees and explain which ones can be removed.
Outside a repository, inspect repositories immediately below DIR (default: current directory).
The default base is the local branch corresponding to each repository's origin/HEAD.
--github-merged queries GitHub.com using gh for merged pull requests.
--reclaim-agent-locks checks stale Claude locks on this Mac (exclusive local worktrees only).
--remove PATH removes one eligible worktree. --remove-all removes all eligible worktrees.
--include-unmerged also includes clean worktrees whose local branch retains their commits.";

#[derive(Default)]
enum Action {
    #[default]
    List,
    One(PathBuf),
    All,
}

#[derive(Default)]
struct Options {
    root: Option<PathBuf>,
    base: Option<OsString>,
    include_unmerged: bool,
    github_merged: bool,
    reclaim_agent_locks: bool,
    action: Action,
}

fn options() -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| format!("missing value for {}", arg.display()))
        };
        match arg.to_str() {
            Some("--help" | "-h") => return Ok(None),
            Some("--root") if options.root.is_none() => options.root = Some(value()?.into()),
            Some("--base") if options.base.is_none() => options.base = Some(value()?),
            Some("--github-merged") if !options.github_merged => options.github_merged = true,
            Some("--reclaim-agent-locks") if !options.reclaim_agent_locks => {
                options.reclaim_agent_locks = true
            }
            Some("--include-unmerged") if !options.include_unmerged => {
                options.include_unmerged = true
            }
            Some("--remove-all") if matches!(options.action, Action::List) => {
                options.action = Action::All
            }
            Some("--remove") if matches!(options.action, Action::List) => {
                let path = PathBuf::from(value()?);
                options.action = Action::One(
                    fs::canonicalize(&path).map_err(|err| format!("{}: {err}", path.display()))?,
                );
            }
            _ => return Err(format!("unexpected or repeated option: {}", arg.display())),
        }
    }
    Ok(Some(options))
}

fn repositories(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    if let Some(repo) = git::discover(root, true)? {
        return Ok(BTreeSet::from([repo]));
    }
    let mut repos = BTreeSet::new();
    for entry in root
        .read_dir()
        .map_err(|err| format!("{}: {err}", root.display()))?
    {
        let entry = entry.map_err(|err| err.to_string())?;
        if entry.file_type().map_err(|err| err.to_string())?.is_dir() {
            match git::discover(&entry.path(), false) {
                Ok(Some(repo)) => {
                    repos.insert(repo);
                }
                Ok(None) => {}
                Err(reason) => eprintln!("{}: {reason}", entry.path().display()),
            }
        }
    }
    Ok(repos)
}

fn field(value: &OsStr) -> String {
    value
        .as_encoded_bytes()
        .utf8_chunks()
        .fold(String::new(), |mut text, chunk| {
            for ch in chunk.valid().chars() {
                if ch.is_control() || ch == '\\' {
                    text.extend(ch.escape_default());
                } else {
                    text.push(ch);
                }
            }
            for byte in chunk.invalid() {
                use std::fmt::Write;
                write!(text, "\\x{byte:02x}").unwrap();
            }
            text
        })
}

fn report(repository: &Path, path: &OsStr, decision: &str, reason: &str) -> Result<(), String> {
    let name = repository.file_name().unwrap_or(repository.as_os_str());
    writeln!(
        io::stdout().lock(),
        "{}\t{}\t{decision}\t{}",
        field(name),
        field(path),
        field(reason.as_ref())
    )
    .map_err(|err| err.to_string())
}

fn run() -> Result<(), String> {
    let Some(options) = options()? else {
        println!("{HELP}");
        return Ok(());
    };
    let cwd = env::current_dir()
        .and_then(fs::canonicalize)
        .map_err(|err| err.to_string())?;
    let root = options.root.as_deref().unwrap_or(&cwd);
    let root = root
        .canonicalize()
        .map_err(|err| format!("{}: {err}", root.display()))?;
    git::init()?;
    let selected = select(&root, &cwd, &options)?;
    remove(selected, &options, &cwd)
}

struct Candidate {
    repo: PathBuf,
    name: OsString,
    path: PathBuf,
    inspection: git::Inspection,
}

fn select(root: &Path, cwd: &Path, options: &Options) -> Result<Vec<Candidate>, String> {
    let mut selected = Vec::new();
    for repo in repositories(root)? {
        let trees = match git::worktrees(&repo) {
            Ok(trees) => trees,
            Err(reason) => {
                report(&repo, repo.as_os_str(), "keep", &reason)?;
                continue;
            }
        };
        for (name, path) in trees {
            let path = match path {
                Ok(path) => path,
                Err(reason) => {
                    report(&repo, &name, "keep", &reason)?;
                    continue;
                }
            };
            match git::inspect(&repo, &name, &path, options, cwd, None) {
                Err(reason) => report(&repo, path.as_os_str(), "keep", &reason)?,
                Ok(inspection) => {
                    let remove = match &options.action {
                        Action::All => true,
                        Action::One(target) => target == &path,
                        Action::List => false,
                    };
                    if remove {
                        selected.push(Candidate {
                            repo: repo.clone(),
                            name,
                            path,
                            inspection,
                        });
                    } else {
                        report(&repo, path.as_os_str(), "candidate", &inspection.reason())?;
                    }
                }
            }
        }
    }
    if matches!(options.action, Action::One(_)) && selected.len() != 1 {
        return Err("requested path is not a unique eligible worktree in this scope".into());
    }
    Ok(selected)
}

fn remove(selected: Vec<Candidate>, options: &Options, cwd: &Path) -> Result<(), String> {
    let mut failed = false;
    for Candidate {
        repo,
        name,
        path,
        inspection,
    } in selected
    {
        match git::inspect(&repo, &name, &path, options, cwd, Some(&inspection)) {
            Ok(removed) => report(&repo, path.as_os_str(), "removed", &removed.reason())?,
            Err(reason) => {
                failed = true;
                report(&repo, path.as_os_str(), "keep", &reason)?;
            }
        }
    }
    if failed {
        Err("could not complete all selected removals".into())
    } else {
        Ok(())
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("worktree-cleaner: {reason}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod common;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invariant_12_remove_all_rechecks_after_selection() {
        let fixture = common::Fixture::new();
        let repo = fixture.repo("repo", "main");
        let first = fixture.worktree(&repo, "clean");
        let second = fixture.worktree(&repo, "changed");
        let options = Options {
            action: Action::All,
            ..Options::default()
        };
        git::init().unwrap();
        let selected = select(&repo, &repo, &options).unwrap();
        assert_eq!(selected.len(), 2);
        fs::write(second.join("new"), "unfinished work").unwrap();
        assert!(remove(selected, &options, &repo).is_err());
        assert!(!first.exists());
        assert_eq!(fs::read(second.join("new")).unwrap(), b"unfinished work");
    }

    #[test]
    fn adr0002_invariant_04_branch_name_change_prevents_removal() {
        let fixture = common::Fixture::new();
        let repo = fixture.repo("repo", "main");
        let worktree = fixture.worktree(&repo, "feature");
        fs::write(worktree.join("committed"), "one\n").unwrap();
        common::git(&worktree, &["add", "committed"]);
        common::git(&worktree, &["commit", "-m", "feature"]);
        git::init().unwrap();
        let options = Options {
            include_unmerged: true,
            action: Action::All,
            ..Options::default()
        };
        let selected = select(&repo, &repo, &options).unwrap();
        assert_eq!(selected.len(), 1);
        common::git(&worktree, &["switch", "-c", "renamed"]);
        assert!(remove(selected, &options, &repo).is_err());
        assert!(worktree.exists());
    }

    #[test]
    fn adr0002_invariant_04_commit_change_prevents_removal() {
        let fixture = common::Fixture::new();
        let repo = fixture.repo("repo", "main");
        let worktree = fixture.worktree(&repo, "feature");
        fs::write(worktree.join("committed"), "one\n").unwrap();
        common::git(&worktree, &["add", "committed"]);
        common::git(&worktree, &["commit", "-m", "feature"]);
        git::init().unwrap();
        let options = Options {
            include_unmerged: true,
            action: Action::All,
            ..Options::default()
        };
        let selected = select(&repo, &repo, &options).unwrap();
        assert_eq!(selected.len(), 1);
        fs::write(worktree.join("later"), "two\n").unwrap();
        common::git(&worktree, &["add", "later"]);
        common::git(&worktree, &["commit", "-m", "later"]);
        assert!(remove(selected, &options, &repo).is_err());
        assert!(worktree.exists());
    }
}
