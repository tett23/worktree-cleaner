use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub number: u64,
    pub merge_commit: String,
    pub url: String,
}

pub fn merged_prs(origin_url: &str, base: &str, head: &str) -> Result<Vec<Evidence>, String> {
    query_with_program(Path::new("gh"), origin_url, base, head)
}

fn repository_name(url: &str) -> Result<String, String> {
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("git@github.com:"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .ok_or("origin is not a supported github.com URL")?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err("origin does not identify one GitHub repository".into());
    };
    for part in [owner, repo] {
        if part.is_empty()
            || part == "."
            || part == ".."
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err("origin has an invalid repository identifier".into());
        }
    }
    Ok(format!("{owner}/{repo}"))
}

fn valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_rows(
    output: &str,
    repository: &str,
    base: &str,
    head: &str,
) -> Result<Vec<Evidence>, String> {
    let mut result = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 6 {
            return Err("incomplete GitHub PR response".into());
        }
        let [number, merged_at, pr_head, pr_repo, pr_base, merge_commit] =
            <[&str; 6]>::try_from(fields).map_err(|_| "incomplete GitHub PR response")?;
        let number = number
            .parse::<u64>()
            .map_err(|_| "invalid GitHub PR number")?;
        if number == 0
            || !valid_sha(pr_head)
            || pr_repo.is_empty()
            || pr_base.is_empty()
            || (!merged_at.is_empty() && !merged_at.contains('T'))
            || (!merge_commit.is_empty() && !valid_sha(merge_commit))
        {
            return Err("invalid GitHub PR response".into());
        }
        if !merged_at.is_empty() && pr_head == head && pr_repo == repository && pr_base == base {
            if !valid_sha(merge_commit) {
                return Err("merged GitHub PR has no merge commit".into());
            }
            result.push(Evidence {
                number,
                merge_commit: merge_commit.into(),
                url: format!("https://github.com/{repository}/pull/{number}"),
            });
        }
    }
    Ok(result)
}

const STDERR_LIMIT: u64 = 64 * 1024;
const LINE_LIMIT: usize = 200;

/// The first non-empty line of gh's standard error, trimmed and cut to at
/// most 200 bytes at a UTF-8 character boundary.
fn first_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let end = (0..=line.len().min(LINE_LIMIT))
        .rev()
        .find(|&index| line.is_char_boundary(index))
        .unwrap_or(0);
    Some(line[..end].to_owned())
}

fn query_with_program(
    program: &Path,
    origin_url: &str,
    base: &str,
    head: &str,
) -> Result<Vec<Evidence>, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let repository = repository_name(origin_url)?;
    if !valid_sha(head) || base.is_empty() {
        return Err("invalid base branch or HEAD commit".into());
    }
    let endpoint = format!("repos/{repository}/commits/{head}/pulls");
    let mut child = Command::new(program)
        .args(["api", "--hostname", "github.com", "--paginate", "--jq",
            ".[] | [.number, .merged_at, .head.sha, .base.repo.full_name, .base.ref, .merge_commit_sha] | @tsv", &endpoint])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("start gh: {err}"))?;
    let stdout = child.stdout.take().ok_or("gh did not provide output")?;
    let stderr = child.stderr.take().ok_or("gh did not provide errors")?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    // Drain standard error to its end so gh never blocks on a full pipe,
    // keeping only the head (docs/adr/0007).
    let error_reader = thread::spawn(move || {
        let mut stderr = stderr;
        let mut head = Vec::new();
        (&mut stderr)
            .take(STDERR_LIMIT)
            .read_to_end(&mut head)
            .and_then(|_| std::io::copy(&mut stderr, &mut std::io::sink()))
            .map(|_| head)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                thread::sleep(Duration::from_millis(50))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("GitHub query timed out".into());
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("wait for gh: {err}"));
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "read GitHub response failed")?
        .map_err(|err| format!("read GitHub response: {err}"))?;
    if !status.success() {
        let errors = error_reader.join().ok().and_then(Result::ok);
        return Err(match errors.as_deref().and_then(first_line) {
            Some(line) => format!("GitHub query failed: {status}: {line}"),
            None => format!("GitHub query failed: {status}"),
        });
    }
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("GitHub response is too large".into());
    }
    let output = String::from_utf8(bytes).map_err(|_| "GitHub response is not UTF-8")?;
    parse_rows(&output, &repository, base, head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    const HEAD: &str = "2f2463993cc65d77ac9574c365fb2a69118999e5";
    const MERGE: &str = "1a7387c6b5cabd356b3f2499ce833e9d82cac2e9";

    fn fixture(script: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "wtc-fake-gh-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[test]
    fn invariant_04_merged_prs_only_and_fixed_endpoint() {
        let program = fixture(&format!(
            "test \"$1\" = api && test \"$2\" = --hostname && test \"$3\" = github.com && test \"$4\" = --paginate && printf '1\t2026-11-11T10:31:27Z\t{HEAD}\ttett23/worktree-cleaner\tmain\t{MERGE}\n2\t\t{HEAD}\ttett23/worktree-cleaner\tmain\t{MERGE}\n3\t2026-11-11T10:31:27Z\t{HEAD}\ttett23/worktree-cleaner\tother\t{MERGE}\n'"
        ));
        let result = query_with_program(
            &program,
            "git@github.com:tett23/worktree-cleaner.git",
            "main",
            HEAD,
        )
        .unwrap();
        assert_eq!(
            result,
            vec![Evidence {
                number: 1,
                merge_commit: MERGE.into(),
                url: "https://github.com/tett23/worktree-cleaner/pull/1".into()
            }]
        );
    }

    #[test]
    fn invariant_04_bad_origin_and_head_rejected_before_spawn() {
        let program = fixture("exit 88");
        for origin in [
            "https://evil.example/x/y",
            "https://github.com/x/../y",
            "https://user:pass@github.com/x/y",
        ] {
            assert!(query_with_program(&program, origin, "main", HEAD).is_err());
        }
        assert!(query_with_program(&program, "https://github.com/x/y", "main", "bad").is_err());
    }

    #[test]
    fn invariant_04_failure_or_incomplete_response_rejects_all() {
        let failed = fixture(&format!(
            "printf '1\tmerged\t{HEAD}\ttett23/worktree-cleaner\tmain\t{MERGE}\n'; exit 1"
        ));
        assert!(
            query_with_program(
                &failed,
                "https://github.com/tett23/worktree-cleaner",
                "main",
                HEAD
            )
            .is_err()
        );
        let malformed = fixture("printf '1\tmerged\tbad\n'");
        assert!(
            query_with_program(
                &malformed,
                "https://github.com/tett23/worktree-cleaner",
                "main",
                HEAD
            )
            .is_err()
        );
    }

    fn failure(script: &str) -> String {
        query_with_program(
            &fixture(script),
            "https://github.com/tett23/worktree-cleaner",
            "main",
            HEAD,
        )
        .unwrap_err()
    }

    #[test]
    fn adr0007_i8_first_nonempty_stderr_line_is_trimmed_into_reason() {
        assert_eq!(
            failure(
                "printf '\\n \\t\\n  gh: No commit found for SHA (HTTP 422)  \\nnext\\n' >&2; exit 1"
            ),
            "GitHub query failed: exit status: 1: gh: No commit found for SHA (HTTP 422)"
        );
        assert_eq!(
            failure("printf 'tab\\there\\n' >&2; exit 1"),
            "GitHub query failed: exit status: 1: tab\there"
        );
    }

    #[test]
    fn adr0007_i8_empty_stderr_keeps_status_only_reason() {
        assert_eq!(failure("exit 1"), "GitHub query failed: exit status: 1");
        assert_eq!(
            failure("printf '\\n  \\n' >&2; exit 1"),
            "GitHub query failed: exit status: 1"
        );
    }

    #[test]
    fn adr0007_i8_long_line_is_cut_at_200_bytes_on_a_char_boundary() {
        let ascii = "a".repeat(201);
        assert_eq!(
            failure(&format!("printf '{ascii}\\n' >&2; exit 1")),
            format!("GitHub query failed: exit status: 1: {}", "a".repeat(200))
        );
        let multibyte = format!("{}é", "a".repeat(199));
        assert_eq!(multibyte.len(), 201);
        assert_eq!(
            failure(&format!("printf '{multibyte}\\n' >&2; exit 1")),
            format!("GitHub query failed: exit status: 1: {}", "a".repeat(199))
        );
        assert_eq!(
            failure(&format!("printf '{}\\n' >&2; exit 1", "a".repeat(200))),
            format!("GitHub query failed: exit status: 1: {}", "a".repeat(200))
        );
    }

    #[test]
    fn adr0007_i8_i11_large_stderr_does_not_block_gh() {
        let started = std::time::Instant::now();
        let reason = failure(
            "printf 'first line\\n' >&2; head -c 1048576 /dev/zero | tr '\\0' x >&2; printf '\\nlast\\n' >&2; exit 1",
        );
        assert_eq!(reason, "GitHub query failed: exit status: 1: first line");
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
    }

    #[test]
    fn adr0007_i8_invalid_utf8_is_replaced() {
        assert_eq!(
            failure("printf 'bad \\377 byte\\n' >&2; exit 1"),
            "GitHub query failed: exit status: 1: bad \u{fffd} byte"
        );
    }
}
