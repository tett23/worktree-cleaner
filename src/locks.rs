use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Identity {
    device: u64,
    inode: u64,
}

fn identity(path: &Path) -> Result<Identity, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|err| format!("{}: {err}", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    Ok(Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[derive(Clone, Debug)]
enum Process {
    Absent,
    Started(String),
    Unknown,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

fn probe_process(pid: i32) -> Result<Process, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        return Ok(Process::Unknown);
    }
    #[cfg(target_os = "macos")]
    {
        if unsafe { kill(pid, 0) } != 0 {
            let error = std::io::Error::last_os_error();
            return if error.raw_os_error() == Some(3) {
                Ok(Process::Absent)
            } else {
                Ok(Process::Unknown)
            };
        }
        let output = Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "lstart="])
            .env("LC_ALL", "C")
            .output()
            .map_err(|err| format!("query process {pid}: {err}"))?;
        if !output.status.success() {
            return Ok(Process::Unknown);
        }
        let start = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
        let start = start.trim();
        if start.is_empty() || start.contains('\n') {
            Ok(Process::Unknown)
        } else {
            Ok(Process::Started(start.to_owned()))
        }
    }
}

fn timestamp(value: &str) -> Option<(u32, u32, u32, u32, u32, u32)> {
    let parts: Vec<_> = value.split_ascii_whitespace().collect();
    if parts.len() != 5 || !["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].contains(&parts[0]) {
        return None;
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|item| *item == parts[1])? as u32
        + 1;
    let day: u32 = parts[2].parse().ok()?;
    let clock: Vec<_> = parts[3].split(':').collect();
    if clock.len() != 3 {
        return None;
    }
    let hour: u32 = clock[0].parse().ok()?;
    let minute: u32 = clock[1].parse().ok()?;
    let second: u32 = clock[2].parse().ok()?;
    let year: u32 = parts[4].parse().ok()?;
    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some((year, month, day, hour, minute, second))
}

fn calendar_day(year: u32, month: u32, day: u32, weekday: &str) -> Option<i64> {
    if !(1970..=9999).contains(&year) {
        return None;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if month == 0 || month > 12 || day == 0 || day > lengths[(month - 1) as usize] {
        return None;
    }
    let offsets = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let adjusted_year = year - u32::from(month < 3);
    let weekday_index = (adjusted_year + adjusted_year / 4 - adjusted_year / 100
        + adjusted_year / 400
        + offsets[(month - 1) as usize]
        + day)
        % 7;
    if ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][weekday_index as usize] != weekday {
        return None;
    }
    let days_before_year = |value: u32| -> i64 {
        let value = i64::from(value);
        365 * value + (value + 3) / 4 - (value + 99) / 100 + (value + 399) / 400
    };
    let prior: u32 = lengths[..(month - 1) as usize].iter().sum();
    Some(days_before_year(year) + i64::from(prior + day - 1))
}

fn timestamp_seconds(value: &str) -> Option<i64> {
    let (year, month, day, hour, minute, second) = timestamp(value)?;
    let weekday = value.split_ascii_whitespace().next()?;
    let day = calendar_day(year, month, day, weekday)?;
    Some(day * 86_400 + i64::from(hour * 3_600 + minute * 60 + second))
}

fn sufficiently_later(recorded: &str, observed: &str) -> bool {
    let Some(recorded) = timestamp_seconds(recorded) else {
        return false;
    };
    let Some(observed) = timestamp_seconds(observed) else {
        return false;
    };
    observed - recorded > 48 * 3_600
}

fn lock_owner(reason: &[u8], registration_name: &OsStr) -> Result<(i32, String), String> {
    let text = std::str::from_utf8(reason.strip_suffix(b"\n").unwrap_or(reason))
        .map_err(|_| "lock reason is not UTF-8")?;
    let expected = registration_name
        .to_str()
        .ok_or("registration name is not UTF-8")?;
    let remainder = text
        .strip_prefix("claude agent ")
        .ok_or("unrecognized lock owner")?;
    let (name, details) = remainder
        .split_once(" (pid ")
        .ok_or("unrecognized lock reason")?;
    if name != expected {
        return Err("lock owner name differs from registration".into());
    }
    let details = details
        .strip_suffix(')')
        .ok_or("unrecognized lock reason")?;
    let (pid, start) = details
        .split_once(" start ")
        .ok_or("unrecognized lock reason")?;
    if pid.is_empty() || pid.starts_with('0') || !pid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("invalid lock PID".into());
    }
    let pid: i32 = pid.parse().map_err(|_| "invalid lock PID")?;
    if timestamp_seconds(start).is_none() {
        return Err("invalid lock start time".into());
    }
    Ok((pid, start.to_owned()))
}

fn inspect_with<F>(
    lock_path: &Path,
    worktree_path: &Path,
    registration_name: &OsStr,
    probe: F,
) -> Result<LockEvidence, String>
where
    F: Fn(i32) -> Result<Process, String>,
{
    let registration = identity(
        lock_path
            .parent()
            .ok_or("lock has no registration directory")?,
    )?;
    let worktree = identity(worktree_path)?;
    let metadata = fs::symlink_metadata(lock_path).map_err(|err| err.to_string())?;
    if !metadata.is_file() {
        return Err("lock is not a regular file".into());
    }
    let lock = Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    let reason = fs::read(lock_path).map_err(|err| err.to_string())?;
    let (pid, recorded_start) = lock_owner(&reason, registration_name)?;
    let explanation = match probe(pid)? {
        Process::Absent => format!("owner PID {pid} is absent (ESRCH)"),
        Process::Started(current_start) if sufficiently_later(&recorded_start, &current_start) => {
            format!(
                "owner PID {pid} was reused; current process started at least 48 hours later ({current_start})"
            )
        }
        Process::Started(_) => return Err("lock owner may still be running".into()),
        Process::Unknown => return Err("cannot establish that lock owner exited".into()),
    };
    Ok(LockEvidence {
        reason,
        explanation,
        registration,
        worktree,
        lock,
    })
}

pub fn inspect(
    lock_path: &Path,
    worktree_path: &Path,
    registration_name: &OsStr,
) -> Result<LockEvidence, String> {
    inspect_with(lock_path, worktree_path, registration_name, probe_process)
}

fn same_registration(lock_path: &Path, expected: &LockEvidence) -> Result<(), String> {
    if identity(
        lock_path
            .parent()
            .ok_or("lock has no registration directory")?,
    )? != expected.registration
    {
        return Err("registration directory changed".into());
    }
    Ok(())
}

fn same_identity(
    lock_path: &Path,
    worktree_path: &Path,
    expected: &LockEvidence,
) -> Result<(), String> {
    same_registration(lock_path, expected)?;
    if identity(worktree_path)? != expected.worktree {
        return Err("worktree directory changed".into());
    }
    Ok(())
}

fn restore(
    lock_path: &Path,
    _worktree_path: &Path,
    expected: &LockEvidence,
    original: String,
) -> Result<(), String> {
    same_registration(lock_path, expected)
        .map_err(|err| format!("{original}; lock restoration failed: {err}"))?;
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(format!("{original}; another lock was preserved"));
        }
        Err(err) => return Err(format!("{original}; lock restoration failed: {err}")),
    };
    file.write_all(&expected.reason)
        .map_err(|err| format!("{original}; lock restoration failed: {err}"))?;
    Err(format!("{original}; original lock restored"))
}

fn reclaim_and_remove_with<F, V, P>(
    lock_path: &Path,
    worktree_path: &Path,
    registration_name: &OsStr,
    expected: &LockEvidence,
    probe: F,
    mut verify: V,
    prune: P,
) -> Result<(), String>
where
    F: Fn(i32) -> Result<Process, String>,
    V: FnMut(bool) -> Result<(), String>,
    P: FnOnce() -> Result<(), String>,
{
    same_identity(lock_path, worktree_path, expected)?;
    let current = inspect_with(lock_path, worktree_path, registration_name, &probe)?;
    if current.reason != expected.reason {
        return Err("lock reason changed".into());
    }
    verify(false)?;
    same_identity(lock_path, worktree_path, expected)?;
    let before_unlock = inspect_with(lock_path, worktree_path, registration_name, &probe)?;
    if before_unlock.reason != expected.reason || before_unlock.lock != expected.lock {
        return Err("lock changed before unlock".into());
    }
    // The caller must ensure that no other process is using or reacquiring the lock.
    fs::remove_file(lock_path).map_err(|err| format!("unlock worktree: {err}"))?;
    if let Err(err) = same_identity(lock_path, worktree_path, expected).and_then(|()| verify(true))
    {
        return restore(lock_path, worktree_path, expected, err);
    }
    if let Err(err) = prune() {
        return restore(lock_path, worktree_path, expected, err);
    }
    Ok(())
}

pub fn reclaim_and_remove<V, P>(
    lock_path: &Path,
    worktree_path: &Path,
    registration_name: &OsStr,
    expected: &LockEvidence,
    verify: V,
    prune: P,
) -> Result<(), String>
where
    V: FnMut(bool) -> Result<(), String>,
    P: FnOnce() -> Result<(), String>,
{
    reclaim_and_remove_with(
        lock_path,
        worktree_path,
        registration_name,
        expected,
        probe_process,
        verify,
        prune,
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockEvidence {
    pub reason: Vec<u8>,
    pub explanation: String,
    registration: Identity,
    worktree: Identity,
    lock: Identity,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn fixture() -> std::path::PathBuf {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("wtc-lock-test-{}-{id}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("locked");
        fs::write(
            &path,
            b"claude agent agent-abc (pid 123 start Thu Nov  5 16:31:45 2026)",
        )
        .unwrap();
        path
    }

    #[test]
    fn invariant_03_missing_process_is_reclaimable() {
        let path = fixture();
        let result = inspect_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            |_| Ok(Process::Absent),
        );
        assert!(result.is_ok());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invariant_03_reused_pid_is_reclaimable() {
        let path = fixture();
        let result = inspect_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            |_| Ok(Process::Started("Wed Nov 11 09:44:36 2026".into())),
        );
        assert!(result.is_ok());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invariant_03_active_or_unknown_process_is_preserved() {
        let path = fixture();
        for state in [
            Process::Started("Thu Nov  5 16:31:45 2026".into()),
            Process::Started("Wed Nov  4 16:31:45 2026".into()),
            Process::Unknown,
        ] {
            assert!(
                inspect_with(
                    &path,
                    path.parent().unwrap(),
                    OsStr::new("agent-abc"),
                    |_| Ok(state.clone())
                )
                .is_err()
            );
        }
        assert!(
            inspect_with(&path, path.parent().unwrap(), OsStr::new("other"), |_| Ok(
                Process::Absent
            ))
            .is_err()
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invariant_03_invalid_date_and_nearby_reuse_are_preserved() {
        let path = fixture();
        assert!(
            inspect_with(
                &path,
                path.parent().unwrap(),
                OsStr::new("agent-abc"),
                |_| { Ok(Process::Started("Fri Nov  6 16:31:45 2026".into())) }
            )
            .is_err()
        );
        fs::write(
            &path,
            b"claude agent agent-abc (pid 123 start Mon Nov  5 16:31:45 2026)",
        )
        .unwrap();
        assert!(
            inspect_with(
                &path,
                path.parent().unwrap(),
                OsStr::new("agent-abc"),
                |_| { Ok(Process::Absent) }
            )
            .is_err()
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invariant_05_changed_lock_is_preserved() {
        let path = fixture();
        let evidence = inspect_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            |_| Ok(Process::Absent),
        )
        .unwrap();
        fs::write(&path, b"other lock").unwrap();
        assert!(
            reclaim_and_remove_with(
                &path,
                path.parent().unwrap(),
                OsStr::new("agent-abc"),
                &evidence,
                |_| Ok(Process::Absent),
                |_| Ok(()),
                || Ok(())
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"other lock");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invariant_06_prune_failure_restores_original_reason() {
        let path = fixture();
        let evidence = inspect_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            |_| Ok(Process::Absent),
        )
        .unwrap();
        assert!(
            reclaim_and_remove_with(
                &path,
                path.parent().unwrap(),
                OsStr::new("agent-abc"),
                &evidence,
                |_| Ok(Process::Absent),
                |_| Ok(()),
                || Err("prune failed".into())
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), evidence.reason);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invariant_06_other_lock_is_preserved_during_restoration() {
        let path = fixture();
        let evidence = inspect_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            |_| Ok(Process::Absent),
        )
        .unwrap();
        assert!(
            reclaim_and_remove_with(
                &path,
                path.parent().unwrap(),
                OsStr::new("agent-abc"),
                &evidence,
                |_| Ok(Process::Absent),
                |_| Ok(()),
                || {
                    fs::write(&path, b"new owner").unwrap();
                    Err("prune failed".into())
                }
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"new owner");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invariant_06_missing_registration_reports_restoration_failure() {
        let path = fixture();
        let evidence = inspect_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            |_| Ok(Process::Absent),
        )
        .unwrap();
        let result = reclaim_and_remove_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            &evidence,
            |_| Ok(Process::Absent),
            |_| Ok(()),
            || {
                fs::remove_dir_all(path.parent().unwrap()).unwrap();
                Err("partial prune".into())
            },
        );
        assert!(result.unwrap_err().contains("lock restoration failed"));
    }

    #[test]
    fn invariant_05_relock_after_unlock_is_preserved() {
        let path = fixture();
        let evidence = inspect_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            |_| Ok(Process::Absent),
        )
        .unwrap();
        let result = reclaim_and_remove_with(
            &path,
            path.parent().unwrap(),
            OsStr::new("agent-abc"),
            &evidence,
            |_| Ok(Process::Absent),
            |unlocked| {
                if unlocked {
                    fs::write(&path, b"new owner").unwrap();
                    return Err("relocked".into());
                }
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new owner");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
