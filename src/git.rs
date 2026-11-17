//! Small, direct bindings to the installed libgit2 API used for worktree cleanup.

use crate::{Options, github, locks};
use std::ffi::{CStr, CString, OsStr, OsString, c_char, c_int, c_void};
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;
use std::time::SystemTime;

#[repr(C)]
struct GitStrArray {
    strings: *mut *mut c_char,
    count: usize,
}

#[repr(C)]
struct GitError {
    message: *const c_char,
    _class: c_int,
}

#[repr(C)]
struct GitIndexTime {
    seconds: i32,
    nanoseconds: u32,
}

#[repr(C)]
struct GitIndexEntry {
    _ctime: GitIndexTime,
    _mtime: GitIndexTime,
    _dev: u32,
    _ino: u32,
    mode: u32,
    _uid: u32,
    _gid: u32,
    _file_size: u32,
    _id: [u8; 20],
    flags: u16,
    flags_extended: u16,
    _path: *const c_char,
}

#[repr(C)]
struct GitPruneOptions {
    version: u32,
    flags: u32,
}

#[link(name = "git2")]
unsafe extern "C" {
    fn git_libgit2_init() -> c_int;
    fn git_error_last() -> *const GitError;
    fn git_repository_open(out: *mut *mut c_void, path: *const c_char) -> c_int;
    fn git_repository_open_ext(
        out: *mut *mut c_void,
        path: *const c_char,
        flags: u32,
        ceiling_dirs: *const c_char,
    ) -> c_int;
    fn git_remote_lookup(out: *mut *mut c_void, repo: *mut c_void, name: *const c_char) -> c_int;
    fn git_remote_url(remote: *mut c_void) -> *const c_char;
    fn git_remote_free(remote: *mut c_void);
    fn git_repository_free(repo: *mut c_void);
    fn git_repository_path(repo: *mut c_void) -> *const c_char;
    fn git_repository_workdir(repo: *mut c_void) -> *const c_char;
    fn git_repository_commondir(repo: *mut c_void) -> *const c_char;
    fn git_repository_is_worktree(repo: *mut c_void) -> c_int;
    fn git_repository_state(repo: *mut c_void) -> c_int;
    fn git_repository_head(out: *mut *mut c_void, repo: *mut c_void) -> c_int;
    fn git_repository_index(out: *mut *mut c_void, repo: *mut c_void) -> c_int;
    fn git_reference_lookup(out: *mut *mut c_void, repo: *mut c_void, name: *const c_char)
    -> c_int;
    fn git_reference_symbolic_target(reference: *mut c_void) -> *const c_char;
    fn git_reference_target(reference: *mut c_void) -> *const c_void;
    fn git_reference_name(reference: *mut c_void) -> *const c_char;
    fn git_reference_free(reference: *mut c_void);
    fn git_oid_equal(first: *const c_void, second: *const c_void) -> c_int;
    fn git_graph_descendant_of(
        repo: *mut c_void,
        commit: *const c_void,
        ancestor: *const c_void,
    ) -> c_int;
    fn git_worktree_list(out: *mut GitStrArray, repo: *mut c_void) -> c_int;
    fn git_worktree_lookup(out: *mut *mut c_void, repo: *mut c_void, name: *const c_char) -> c_int;
    fn git_worktree_validate(worktree: *mut c_void) -> c_int;
    fn git_worktree_is_locked(reason: *mut c_void, worktree: *mut c_void) -> c_int;
    fn git_worktree_path(worktree: *mut c_void) -> *const c_char;
    fn git_worktree_prune(worktree: *mut c_void, options: *mut GitPruneOptions) -> c_int;
    fn git_worktree_free(worktree: *mut c_void);
    fn git_strarray_dispose(array: *mut GitStrArray);
    fn git_status_foreach(
        repo: *mut c_void,
        callback: unsafe extern "C" fn(*const c_char, u32, *mut c_void) -> c_int,
        payload: *mut c_void,
    ) -> c_int;
    fn git_index_entrycount(index: *mut c_void) -> usize;
    fn git_index_get_byindex(index: *mut c_void, position: usize) -> *const GitIndexEntry;
    fn git_index_get_bypath(
        index: *mut c_void,
        path: *const c_char,
        stage: c_int,
    ) -> *const GitIndexEntry;
    fn git_index_free(index: *mut c_void);
    fn git_ignore_path_is_ignored(
        ignored: *mut c_int,
        repo: *mut c_void,
        path: *const c_char,
    ) -> c_int;
    fn git_reference_foreach_name(
        repo: *mut c_void,
        callback: unsafe extern "C" fn(*const c_char, *mut c_void) -> c_int,
        payload: *mut c_void,
    ) -> c_int;
    fn git_object_lookup(
        out: *mut *mut c_void,
        repo: *mut c_void,
        id: *const c_void,
        kind: c_int,
    ) -> c_int;
    fn git_object_free(object: *mut c_void);
}

struct Repo(*mut c_void);
struct Worktree(*mut c_void);
struct Reference(*mut c_void);
struct Index(*mut c_void);

pub type Registration = (OsString, Result<PathBuf, String>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inspection {
    pub branch: OsString,
    pub commit: [u8; 20],
    pub unmerged: bool,
    github: Option<github::Evidence>,
    lock: Option<(PathBuf, locks::LockEvidence)>,
    /// `origin/<name>` when `--remote-base` selected the remote-tracking branch.
    remote_base: Option<String>,
    /// Ignored files the removal deletes; `None` unless `--allow-ignored` is set.
    ignored: Option<usize>,
    /// The short name of the base that contains a detached HEAD
    /// (`--include-detached`); `None` for a worktree on a branch.
    detached: Option<String>,
}

fn hex(oid: &[u8; 20]) -> String {
    oid.iter().map(|byte| format!("{byte:02x}")).collect()
}

type Evidence<'a> = (
    &'a OsString,
    &'a [u8; 20],
    bool,
    &'a Option<github::Evidence>,
    &'a Option<(PathBuf, locks::LockEvidence)>,
    &'a Option<String>,
    &'a Option<String>,
);

impl Inspection {
    pub fn reason(&self) -> String {
        let mut reason = if let Some(base) = &self.detached {
            format!(
                "clean detached HEAD {} is integrated into {base}",
                hex(&self.commit)
            )
        } else if let Some(pr) = &self.github {
            format!("clean and integrated via {}", pr.url)
        } else if self.unmerged {
            format!("clean unmerged branch {}", self.branch.to_string_lossy())
        } else if let Some(base) = &self.remote_base {
            format!("clean and integrated into {base}")
        } else {
            "clean and integrated".to_owned()
        };
        if let Some((_, lock)) = &self.lock {
            reason.push_str(&format!(
                "; lock {:?}: {}",
                String::from_utf8_lossy(&lock.reason),
                lock.explanation
            ));
        }
        if let Some(count) = self.ignored {
            reason.push_str(&format!("; deletes {count} ignored files"));
        }
        reason
    }

    /// The evidence compared between selection and removal. The ignored file
    /// count is reported, not compared (ADR 0008).
    fn evidence(&self) -> Evidence<'_> {
        (
            &self.branch,
            &self.commit,
            self.unmerged,
            &self.github,
            &self.lock,
            &self.remote_base,
            &self.detached,
        )
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        unsafe { git_repository_free(self.0) }
    }
}
impl Drop for Worktree {
    fn drop(&mut self) {
        unsafe { git_worktree_free(self.0) }
    }
}
impl Drop for Reference {
    fn drop(&mut self) {
        unsafe { git_reference_free(self.0) }
    }
}
impl Drop for Index {
    fn drop(&mut self) {
        unsafe { git_index_free(self.0) }
    }
}

fn error(context: &str) -> String {
    let detail = unsafe {
        let last = git_error_last();
        if last.is_null() || (*last).message.is_null() {
            String::from("libgit2 error")
        } else {
            CStr::from_ptr((*last).message)
                .to_string_lossy()
                .into_owned()
        }
    };
    format!("{context}: {detail}")
}

fn check(value: c_int, context: &str) -> Result<(), String> {
    if value == 0 {
        Ok(())
    } else {
        Err(error(context))
    }
}

fn cpath(path: &Path) -> Result<CString, String> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("path contains NUL: {}", path.display()))
}

fn cstr(value: &OsStr) -> Result<CString, String> {
    CString::new(value.as_bytes()).map_err(|_| "name contains NUL".to_owned())
}

unsafe fn path_from_c(pointer: *const c_char) -> Result<PathBuf, String> {
    if pointer.is_null() {
        return Err("libgit2 returned no path".to_owned());
    }
    Ok(PathBuf::from(OsString::from_vec(
        unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec(),
    )))
}

fn open(path: &Path, flags: u32) -> Result<Option<Repo>, String> {
    let mut pointer = ptr::null_mut();
    let code =
        unsafe { git_repository_open_ext(&mut pointer, cpath(path)?.as_ptr(), flags, ptr::null()) };
    if code == -3 || code == -8 {
        return Ok(None);
    }
    check(code, "open repository")?;
    Ok(Some(Repo(pointer)))
}

fn main_repo(path: &Path) -> Result<Repo, String> {
    let mut pointer = ptr::null_mut();
    check(
        unsafe { git_repository_open(&mut pointer, cpath(path)?.as_ptr()) },
        "open main repository",
    )?;
    let repo = Repo(pointer);
    if unsafe { git_repository_is_worktree(repo.0) } != 0
        || unsafe { git_repository_workdir(repo.0) }.is_null()
    {
        return Err("not a main worktree".to_owned());
    }
    Ok(repo)
}

fn lookup_worktree(repo: &Repo, name: &OsStr) -> Result<Worktree, String> {
    let mut pointer = ptr::null_mut();
    check(
        unsafe { git_worktree_lookup(&mut pointer, repo.0, cstr(name)?.as_ptr()) },
        "lookup worktree",
    )?;
    Ok(Worktree(pointer))
}

fn lookup_reference(repo: &Repo, name: &CStr) -> Result<Reference, String> {
    let mut pointer = ptr::null_mut();
    check(
        unsafe { git_reference_lookup(&mut pointer, repo.0, name.as_ptr()) },
        "resolve reference",
    )?;
    Ok(Reference(pointer))
}

pub fn init() -> Result<(), String> {
    let result = unsafe { git_libgit2_init() };
    if result > 0 {
        Ok(())
    } else {
        Err(error("initialize libgit2"))
    }
}

/// Resolve the owning main worktree. For a direct child, reject linked worktrees.
pub fn discover(path: &Path, search_parents: bool) -> Result<Option<PathBuf>, String> {
    let canonical = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(repo) = open(&canonical, if search_parents { 0 } else { 1 })? else {
        return Ok(None);
    };
    let workdir = unsafe { git_repository_workdir(repo.0) };
    if workdir.is_null() {
        return Ok(None);
    }
    let own_root = fs::canonicalize(unsafe { path_from_c(workdir)? }).map_err(|e| e.to_string())?;
    if !search_parents
        && (unsafe { git_repository_is_worktree(repo.0) } != 0 || canonical != own_root)
    {
        return Ok(None);
    }
    let common_dir = unsafe { path_from_c(git_repository_commondir(repo.0))? };
    let main = main_repo(&common_dir)?;
    let main_root = unsafe { path_from_c(git_repository_workdir(main.0))? };
    Ok(Some(
        fs::canonicalize(main_root).map_err(|e| e.to_string())?,
    ))
}

pub fn worktrees(repo_path: &Path) -> Result<Vec<Registration>, String> {
    let repo = main_repo(repo_path)?;
    let mut names = GitStrArray {
        strings: ptr::null_mut(),
        count: 0,
    };
    check(
        unsafe { git_worktree_list(&mut names, repo.0) },
        "list worktrees",
    )?;
    let result = (0..names.count)
        .map(|position| {
            let name = unsafe { CStr::from_ptr(*names.strings.add(position)) };
            let os_name = OsString::from_vec(name.to_bytes().to_vec());
            let path = lookup_worktree(&repo, &os_name)
                .and_then(|worktree| unsafe { path_from_c(git_worktree_path(worktree.0)) });
            (os_name, path)
        })
        .collect();
    unsafe { git_strarray_dispose(&mut names) };
    Ok(result)
}

/// The base that decides integration: the branch name every check compares
/// against, and the reference whose tip is the reachability target.
struct Base {
    /// Branch name without a prefix, such as `main`.
    name: Vec<u8>,
    /// `refs/heads/<name>` by default, `refs/remotes/origin/<name>` with `--remote-base`.
    reference: Reference,
    remote: bool,
}

impl Base {
    fn branch_ref(&self) -> Vec<u8> {
        [b"refs/heads/".as_slice(), &self.name].concat()
    }

    fn label(&self) -> Option<String> {
        self.remote.then(|| self.short_name())
    }

    /// `<name>`, or `origin/<name>` with `--remote-base`.
    fn short_name(&self) -> String {
        let name = String::from_utf8_lossy(&self.name);
        if self.remote {
            format!("origin/{name}")
        } else {
            name.into_owned()
        }
    }
}

fn base_name(repo: &Repo, explicit: Option<&OsStr>) -> Result<Vec<u8>, String> {
    if let Some(name) = explicit {
        return Ok(name.as_bytes().to_vec());
    }
    let origin_head = lookup_reference(repo, c"refs/remotes/origin/HEAD")?;
    let target = unsafe { git_reference_symbolic_target(origin_head.0) };
    if target.is_null() {
        return Err("origin/HEAD is not symbolic".to_owned());
    }
    Ok(unsafe { CStr::from_ptr(target) }
        .to_bytes()
        .strip_prefix(b"refs/remotes/origin/")
        .filter(|name| !name.is_empty())
        .ok_or("origin/HEAD does not name an origin branch")?
        .to_vec())
}

/// Select the base for one repository. Every check that needs the base,
/// including the recheck before removal, goes through this function.
fn base(repo: &Repo, explicit: Option<&OsStr>, remote: bool) -> Result<Base, String> {
    let name = base_name(repo, explicit)?;
    let prefix: &[u8] = if remote {
        b"refs/remotes/origin/"
    } else {
        b"refs/heads/"
    };
    let full =
        CString::new([prefix, &name].concat()).map_err(|_| "invalid base branch".to_owned())?;
    let reference = lookup_reference(repo, &full).map_err(|reason| {
        if remote {
            format!(
                "remote base {} is unavailable: {reason}",
                full.to_string_lossy()
            )
        } else {
            reason
        }
    })?;
    Ok(Base {
        name,
        reference,
        remote,
    })
}

const GIT_STATUS_WT_NEW: u32 = 1 << 7;
const GIT_STATUS_IGNORED: u32 = 1 << 14;

/// The first path the status callback reported for each kind of change, in
/// the order the reason prefers them: tracked, untracked, ignored
/// (docs/adr/0007). With `--allow-ignored`, ignored entries are not changes
/// (docs/adr/0008).
struct Changes {
    allow_ignored: bool,
    first: [Option<Vec<u8>>; 3],
}

impl Changes {
    fn kind(flags: u32) -> usize {
        match flags {
            GIT_STATUS_WT_NEW => 1,
            GIT_STATUS_IGNORED => 2,
            _ => 0,
        }
    }

    fn reason(&self) -> Option<String> {
        ["tracked changes", "untracked file", "ignored file"]
            .iter()
            .zip(&self.first)
            .find_map(|(label, path)| {
                path.as_ref()
                    .map(|path| format!("{label}: {}", String::from_utf8_lossy(path)))
            })
    }
}

unsafe extern "C" fn record_change(path: *const c_char, flags: u32, payload: *mut c_void) -> c_int {
    let changes = unsafe { &mut *(payload as *mut Changes) };
    if changes.allow_ignored && flags == GIT_STATUS_IGNORED {
        return 0;
    }
    let first = &mut changes.first[Changes::kind(flags)];
    if first.is_none() && !path.is_null() {
        *first = Some(unsafe { CStr::from_ptr(path) }.to_bytes().to_vec());
    }
    0
}

fn is_ignored(repo: &Repo, relative: &Path) -> Result<bool, String> {
    let mut ignored: c_int = 0;
    check(
        unsafe { git_ignore_path_is_ignored(&mut ignored, repo.0, cpath(relative)?.as_ptr()) },
        "check ignore rules",
    )?;
    Ok(ignored == 1)
}

/// Return the number of ignored files when `allow_ignored` permits them.
fn ensure_clean(repo: &Repo, root: &Path, allow_ignored: bool) -> Result<usize, String> {
    let mut changes = Changes {
        allow_ignored,
        first: Default::default(),
    };
    let code =
        unsafe { git_status_foreach(repo.0, record_change, (&mut changes as *mut Changes).cast()) };
    check(code, "read worktree status")?;
    if let Some(reason) = changes.reason() {
        return Err(reason);
    }

    let mut pointer = ptr::null_mut();
    check(
        unsafe { git_repository_index(&mut pointer, repo.0) },
        "read index",
    )?;
    let index = Index(pointer);
    for position in 0..unsafe { git_index_entrycount(index.0) } {
        let entry = unsafe { git_index_get_byindex(index.0, position) };
        if entry.is_null() {
            return Err("cannot read index entry".to_owned());
        }
        let entry = unsafe { &*entry };
        if entry.mode == 0o160000 {
            return Err("submodule in worktree".to_owned());
        }
        if entry.flags & 0xb000 != 0 || entry.flags_extended & 0x6000 != 0 {
            return Err("index has special or unresolved entries".to_owned());
        }
    }
    // Independently account for every filesystem entry before pruning the directory.
    let mut ignored = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(&directory).map_err(|e| format!("{}: {e}", directory.display()))? {
            let item = item.map_err(|e| e.to_string())?;
            if item.file_name() == ".git" {
                if directory != root {
                    return Err("nested repository".to_owned());
                }
                continue;
            }
            let metadata = fs::symlink_metadata(item.path()).map_err(|e| e.to_string())?;
            if metadata.is_dir() {
                pending.push(item.path());
            } else {
                if !metadata.is_file() && !metadata.file_type().is_symlink() {
                    return Err(format!("special file: {}", item.path().display()));
                }
                let relative = item
                    .path()
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                let entry = unsafe { git_index_get_bypath(index.0, cpath(&relative)?.as_ptr(), 0) };
                if entry.is_null() {
                    if allow_ignored && is_ignored(repo, &relative)? {
                        ignored += 1;
                        continue;
                    }
                    return Err(format!("untracked file: {}", item.path().display()));
                }
                if metadata.is_file() {
                    fs::File::open(item.path())
                        .map_err(|e| format!("{}: {e}", item.path().display()))?;
                }
            }
        }
    }
    Ok(ignored)
}

/// Whether looking up the object reports that the repository lacks it.
fn object_missing(repo: &Repo, oid: *const c_void) -> bool {
    let mut object = ptr::null_mut();
    // GIT_OBJECT_ANY
    match unsafe { git_object_lookup(&mut object, repo.0, oid, -2) } {
        0 => {
            unsafe { git_object_free(object) };
            false
        }
        code => code == -3,
    }
}

// A removal pass queries GitHub once, then repeats the local checks after the
// network round trip. The listing pass and removal pass never share a reply.
struct PrReply {
    origin: String,
    base: String,
    head: [u8; 20],
    prs: Vec<github::Evidence>,
}

fn merged_pr(
    main: &Repo,
    target: &Base,
    head: &[u8; 20],
    reply: &mut Option<PrReply>,
) -> Result<Option<github::Evidence>, String> {
    let mut remote = ptr::null_mut();
    check(
        unsafe { git_remote_lookup(&mut remote, main.0, c"origin".as_ptr()) },
        "read origin",
    )?;
    let url = unsafe { git_remote_url(remote) };
    let url = if url.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(url) }
            .to_str()
            .ok()
            .map(str::to_owned)
    };
    unsafe { git_remote_free(remote) };
    let url = url.ok_or("origin has no usable URL")?;
    let base = std::str::from_utf8(&target.name).map_err(|_| "invalid base branch")?;
    let head_hex = hex(head);
    let base_oid = unsafe { git_reference_target(target.reference.0) };
    if base_oid.is_null() {
        return Err("base has no commit".to_owned());
    }
    let reply = match reply {
        Some(previous)
            if previous.origin == url && previous.base == base && previous.head == *head =>
        {
            previous
        }
        Some(_) => return Err("GitHub query target changed during recheck".into()),
        empty => {
            let prs = github::merged_prs(&url, base, &head_hex)?;
            empty.insert(PrReply {
                origin: url,
                base: base.into(),
                head: *head,
                prs,
            })
        }
    };
    for pr in &reply.prs {
        let mut oid = [0u8; 20];
        for (i, byte) in oid.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&pr.merge_commit[2 * i..2 * i + 2], 16)
                .map_err(|_| "invalid merge commit")?;
        }
        let merge = oid.as_ptr().cast();
        if unsafe { git_oid_equal(base_oid, merge) } == 1 {
            return Ok(Some(pr.clone()));
        }
        match unsafe { git_graph_descendant_of(main.0, base_oid, merge) } {
            1 => return Ok(Some(pr.clone())),
            0 | -3 => continue,
            _ => {
                let failure = error("check PR merge commit ancestry");
                return Err(if object_missing(main, merge) {
                    format!(
                        "merge commit {} of {} is not in the local repository; fetch origin to check it",
                        pr.merge_commit, pr.url
                    )
                } else {
                    failure
                });
            }
        }
    }
    Ok(None)
}

fn absent(path: &Path) -> bool {
    matches!(fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

fn parse_oid(text: &[u8]) -> Option<[u8; 20]> {
    if text.len() != 40 {
        return None;
    }
    let digits = text
        .iter()
        .map(|byte| (*byte as char).to_digit(16).map(|digit| digit as u8))
        .collect::<Option<Vec<u8>>>()?;
    let mut oid = [0u8; 20];
    oid.iter_mut()
        .zip(digits.chunks(2))
        .for_each(|(byte, pair)| *byte = pair[0] << 4 | pair[1]);
    Some(oid)
}

/// Explains a registration whose working directory or `.git` file is gone,
/// before libgit2's validation reports only the first missing piece
/// (docs/adr/0007).
fn missing_checkout(main: &Repo, worktree: &Worktree, name: &OsStr) -> Option<String> {
    let path = unsafe { path_from_c(git_worktree_path(worktree.0)) }.ok()?;
    let state = if absent(&path) {
        "worktree directory is missing"
    } else if absent(&path.join(".git")) {
        "worktree .git file is missing"
    } else {
        return None;
    };
    let head = unsafe { path_from_c(git_repository_commondir(main.0)) }
        .map(|common| common.join("worktrees").join(name).join("HEAD"))
        .and_then(|head| describe_head(main, &head))
        .unwrap_or_else(|reason| format!("cannot inspect HEAD: {reason}"));
    Some(format!("{state}; {head}"))
}

/// Describes what a registration's `HEAD` file names, read directly because
/// libgit2 cannot open a repository for the missing checkout.
fn describe_head(main: &Repo, head: &Path) -> Result<String, String> {
    let content = fs::read(head).map_err(|e| format!("{}: {e}", head.display()))?;
    let content = content.trim_ascii();
    if let Some(target) = content.strip_prefix(b"ref: ") {
        let branch = target
            .trim_ascii()
            .strip_prefix(b"refs/heads/")
            .ok_or("HEAD names a reference outside refs/heads/")?;
        let mut full = b"refs/heads/".to_vec();
        full.extend(branch);
        let full = CString::new(full).map_err(|_| "branch name contains NUL")?;
        let mut pointer = ptr::null_mut();
        let branch = String::from_utf8_lossy(branch);
        return match unsafe { git_reference_lookup(&mut pointer, main.0, full.as_ptr()) } {
            0 => {
                drop(Reference(pointer));
                Ok(format!("HEAD is branch {branch}"))
            }
            -3 => Ok(format!("HEAD is missing branch {branch}")),
            _ => Err(error("resolve HEAD branch")),
        };
    }
    let oid = parse_oid(content).ok_or("HEAD names neither a branch nor a commit")?;
    Ok(match reachable_from(main, &oid)? {
        Some(name) => format!("detached HEAD {} is reachable from {name}", hex(&oid)),
        None => format!(
            "detached HEAD {} is not reachable from any branch or remote-tracking branch",
            hex(&oid)
        ),
    })
}

unsafe extern "C" fn collect_name(name: *const c_char, payload: *mut c_void) -> c_int {
    let names = unsafe { &mut *(payload as *mut Vec<Vec<u8>>) };
    if !name.is_null() {
        names.push(unsafe { CStr::from_ptr(name) }.to_bytes().to_vec());
    }
    0
}

/// The short name of the bytewise smallest branch or remote-tracking branch
/// whose tip is `oid` or a descendant of it. Symbolic references have no tip
/// of their own and are skipped.
fn reachable_from(main: &Repo, oid: &[u8; 20]) -> Result<Option<String>, String> {
    let mut names: Vec<Vec<u8>> = Vec::new();
    check(
        unsafe {
            git_reference_foreach_name(
                main.0,
                collect_name,
                (&mut names as *mut Vec<Vec<u8>>).cast(),
            )
        },
        "list references",
    )?;
    let mut names: Vec<(Vec<u8>, &[u8])> = names
        .iter()
        .filter_map(|name| {
            [&b"refs/heads/"[..], b"refs/remotes/"]
                .iter()
                .find_map(|prefix| name.strip_prefix(*prefix))
                .map(|short| (name.clone(), short))
        })
        .collect();
    names.sort();
    let commit = oid.as_ptr().cast();
    for (full, short) in names {
        let reference = lookup_reference(
            main,
            &CString::new(full).map_err(|_| "reference name contains NUL")?,
        )?;
        let tip = unsafe { git_reference_target(reference.0) };
        if tip.is_null() {
            continue;
        }
        if unsafe { git_oid_equal(tip, commit) } == 1 {
            return Ok(Some(String::from_utf8_lossy(short).into_owned()));
        }
        match unsafe { git_graph_descendant_of(main.0, tip, commit) } {
            1 => return Ok(Some(String::from_utf8_lossy(short).into_owned())),
            0 => {}
            _ => return Err(error("check HEAD reachability")),
        }
    }
    Ok(None)
}

struct Checked {
    inspection: Inspection,
    worktree: Worktree,
}

fn evaluate(
    repo_path: &Path,
    name: &OsStr,
    path: &Path,
    options: &Options,
    cwd: &Path,
    reply: &mut Option<PrReply>,
) -> Result<Checked, String> {
    let main = main_repo(repo_path)?;
    let worktree = lookup_worktree(&main, name)?;
    if let Some(reason) = missing_checkout(&main, &worktree, name) {
        return Err(reason);
    }
    check(
        unsafe { git_worktree_validate(worktree.0) },
        "invalid worktree registration",
    )?;
    let locked = unsafe { git_worktree_is_locked(ptr::null_mut(), worktree.0) };
    if locked < 0 {
        return Err(error("read worktree lock"));
    }
    if locked > 0 && !options.reclaim_agent_locks {
        return Err("worktree is locked".to_owned());
    }
    let registered = fs::canonicalize(unsafe { path_from_c(git_worktree_path(worktree.0))? })
        .map_err(|e| e.to_string())?;
    let requested = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if registered != requested {
        return Err("worktree path differs from registration".to_owned());
    }
    let gitlink = registered.join(".git");
    if !fs::symlink_metadata(&gitlink)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_file()
    {
        return Err("worktree gitlink is not a regular .git file".to_owned());
    }
    let current = fs::canonicalize(cwd).map_err(|e| e.to_string())?;
    if current.starts_with(&registered) {
        return Err("worktree contains current directory".to_owned());
    }
    let linked = open(&registered, 1)?.ok_or("cannot open linked worktree")?;
    if unsafe { git_repository_is_worktree(linked.0) } != 1 {
        return Err("not a linked worktree".to_owned());
    }
    let linked_workdir =
        fs::canonicalize(unsafe { path_from_c(git_repository_workdir(linked.0))? })
            .map_err(|e| e.to_string())?;
    if linked_workdir != registered {
        return Err("linked workdir differs from registered path".to_owned());
    }
    let main_common = fs::canonicalize(unsafe { path_from_c(git_repository_commondir(main.0))? })
        .map_err(|e| e.to_string())?;
    let linked_common =
        fs::canonicalize(unsafe { path_from_c(git_repository_commondir(linked.0))? })
            .map_err(|e| e.to_string())?;
    if main_common != linked_common {
        return Err("linked worktree belongs to another repository".to_owned());
    }
    let registration_dir = main_common.join("worktrees").join(name);
    if !fs::symlink_metadata(&registration_dir)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_dir()
    {
        return Err("worktree registration is not a directory".to_owned());
    }
    let registered_gitlink =
        fs::read(registration_dir.join("gitdir")).map_err(|e| e.to_string())?;
    let registered_gitlink = registered_gitlink
        .strip_suffix(b"\n")
        .unwrap_or(&registered_gitlink);
    let registered_gitlink = registered_gitlink
        .strip_suffix(b"\r")
        .unwrap_or(registered_gitlink);
    if fs::canonicalize(Path::new(OsStr::from_bytes(registered_gitlink)))
        .map_err(|e| e.to_string())?
        != fs::canonicalize(&gitlink).map_err(|e| e.to_string())?
    {
        return Err("registered gitlink is not the worktree .git file".to_owned());
    }
    let linked_gitdir = fs::canonicalize(unsafe { path_from_c(git_repository_path(linked.0))? })
        .map_err(|e| e.to_string())?;
    let expected_gitdir = fs::canonicalize(&registration_dir).map_err(|e| e.to_string())?;
    if linked_gitdir != expected_gitdir {
        return Err("linked worktree registration differs".to_owned());
    }
    if unsafe { git_repository_state(linked.0) } != 0 {
        return Err("Git operation is in progress".to_owned());
    }
    let lock = if locked > 0 {
        let lock_path = registration_dir.join("locked");
        let evidence = locks::inspect(&lock_path, &registered, name)?;
        Some((lock_path, evidence))
    } else {
        None
    };
    ensure_idle(&registration_dir, options.min_idle, SystemTime::now())?;
    let target = base(&main, options.base.as_deref(), options.remote_base)?;
    let head = {
        let mut pointer = ptr::null_mut();
        check(
            unsafe { git_repository_head(&mut pointer, linked.0) },
            "resolve worktree HEAD",
        )?;
        Reference(pointer)
    };
    let head_name = unsafe { git_reference_name(head.0) };
    let on_branch = !head_name.is_null()
        && unsafe { CStr::from_ptr(head_name) }
            .to_bytes()
            .starts_with(b"refs/heads/");
    if !on_branch {
        if !options.include_detached {
            return Err("worktree HEAD is detached".to_owned());
        }
        let inspection = detached(
            &main,
            &linked,
            &head,
            &target,
            options.allow_ignored,
            &registration_dir,
            &registered,
        )?;
        // A lock is never reclaimed for a detached HEAD (ADR 0011 I4).
        if lock.is_some() {
            return Err("worktree is locked".to_owned());
        }
        return Ok(Checked {
            inspection,
            worktree,
        });
    }
    if unsafe { CStr::from_ptr(head_name) }.to_bytes() == target.branch_ref() {
        return Err("worktree uses base branch".to_owned());
    }
    let target_oid = unsafe { git_reference_target(target.reference.0) };
    let head_oid = unsafe { git_reference_target(head.0) };
    if target_oid.is_null() || head_oid.is_null() {
        return Err("branch has no commit".to_owned());
    }
    let descendant = unsafe { git_graph_descendant_of(main.0, target_oid, head_oid) };
    if descendant < 0 {
        return Err(error("check commit ancestry"));
    }
    let included = unsafe { git_oid_equal(target_oid, head_oid) } == 1 || descendant == 1;
    if !included && !options.github_merged && (!options.include_unmerged || lock.is_some()) {
        return Err("branch is not merged into base".to_owned());
    }
    let branch = unsafe { CStr::from_ptr(head_name) }
        .to_bytes()
        .strip_prefix(b"refs/heads/")
        .ok_or("worktree HEAD is detached")?;
    if !included {
        let direct = lookup_reference(&linked, c"HEAD")?;
        let symbolic = unsafe { git_reference_symbolic_target(direct.0) };
        if symbolic.is_null()
            || unsafe { CStr::from_ptr(symbolic) } != unsafe { CStr::from_ptr(head_name) }
        {
            return Err("HEAD does not directly name the retained branch".to_owned());
        }
        let direct_branch = lookup_reference(&linked, unsafe { CStr::from_ptr(symbolic) })?;
        if unsafe { git_reference_target(direct_branch.0) }.is_null()
            || unsafe { git_oid_equal(git_reference_target(direct_branch.0), head_oid) } != 1
        {
            return Err("retained branch no longer points to HEAD".to_owned());
        }
    }
    let ignored = ensure_clean(&linked, &registered, options.allow_ignored)?;
    let commit = unsafe { *(head_oid as *const [u8; 20]) };
    let github = if !included && options.github_merged {
        merged_pr(&main, &target, &commit, reply)?
    } else {
        None
    };
    let unmerged = !included && github.is_none();
    if unmerged && (!options.include_unmerged || lock.is_some()) {
        return Err("no matching integrated pull request for this branch tip".into());
    }
    let inspection = Inspection {
        branch: OsString::from_vec(branch.to_vec()),
        commit,
        unmerged,
        github,
        lock,
        remote_base: target.label(),
        ignored: options.allow_ignored.then_some(ignored),
        detached: None,
    };
    Ok(Checked {
        inspection,
        worktree,
    })
}

// Activity protection (ADR 0010).
const ACTIVITY_FILES: [&str; 3] = ["HEAD", "index", "logs/HEAD"];

/// The latest modification time among the activity files that exist.
fn last_activity(paths: &[PathBuf]) -> Result<SystemTime, String> {
    paths
        .iter()
        .filter_map(|path| match fs::metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            metadata => Some(
                metadata
                    .and_then(|metadata| metadata.modified())
                    .map_err(|error| {
                        format!("cannot read worktree activity: {}: {error}", path.display())
                    }),
            ),
        })
        .collect::<Result<Vec<SystemTime>, String>>()?
        .into_iter()
        .max()
        .ok_or_else(|| "cannot read worktree activity: no HEAD, index or logs/HEAD".to_owned())
}

fn idle_since(last: SystemTime, now: SystemTime, min_idle: u64) -> Result<(), String> {
    let hours = now
        .duration_since(last)
        .map_err(|_| "worktree Git activity time is in the future".to_owned())?
        .as_secs()
        / 3600;
    if hours < min_idle {
        Err(format!(
            "worktree had Git activity {hours} hours ago (less than --min-idle {min_idle})"
        ))
    } else {
        Ok(())
    }
}

/// With `min_idle` 0 the protection is off and no file time is read.
fn ensure_idle(registration_dir: &Path, min_idle: u64, now: SystemTime) -> Result<(), String> {
    if min_idle == 0 {
        return Ok(());
    }
    let paths: Vec<PathBuf> = ACTIVITY_FILES
        .iter()
        .map(|name| registration_dir.join(name))
        .collect();
    idle_since(last_activity(&paths)?, now, min_idle)
}

/// Whether `directory` holds any non-directory entry at any depth.
fn holds_files(directory: &Path) -> Result<bool, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("{}: {error}", directory.display())),
    };
    entries
        .map(|entry| {
            let path = entry.map_err(|e| e.to_string())?.path();
            if fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .is_dir()
            {
                holds_files(&path)
            } else {
                Ok(true)
            }
        })
        .try_fold(false, |found, holds| Ok(found || holds?))
}

/// Decide a worktree whose HEAD is not on a local branch (ADR 0011).
fn detached(
    main: &Repo,
    linked: &Repo,
    head: &Reference,
    target: &Base,
    allow_ignored: bool,
    registration_dir: &Path,
    root: &Path,
) -> Result<Inspection, String> {
    let name = unsafe { git_reference_name(head.0) };
    let head_oid = unsafe { git_reference_target(head.0) };
    if name.is_null() || unsafe { CStr::from_ptr(name) } != c"HEAD" || head_oid.is_null() {
        return Err("worktree HEAD is neither a branch nor a commit".to_owned());
    }
    // Removing the worktree would drop these refs and the commits only they reach.
    if holds_files(&registration_dir.join("refs"))? {
        return Err("worktree has worktree-specific refs".to_owned());
    }
    let target_oid = unsafe { git_reference_target(target.reference.0) };
    if target_oid.is_null() {
        return Err("base has no commit".to_owned());
    }
    let included = unsafe { git_oid_equal(target_oid, head_oid) } == 1
        || match unsafe { git_graph_descendant_of(main.0, target_oid, head_oid) } {
            1 => true,
            0 => false,
            _ => return Err(error("check commit ancestry")),
        };
    if !included {
        return Err(format!(
            "detached HEAD is not integrated into {}",
            target.short_name()
        ));
    }
    let ignored = ensure_clean(linked, root, allow_ignored)?;
    Ok(Inspection {
        branch: OsString::new(),
        commit: unsafe { *(head_oid as *const [u8; 20]) },
        unmerged: false,
        github: None,
        lock: None,
        remote_base: target.label(),
        ignored: allow_ignored.then_some(ignored),
        detached: Some(target.short_name()),
    })
}

fn prune(worktree: &Worktree, repo_path: &Path, name: &OsStr, path: &Path) -> Result<(), String> {
    // VALID permits an existing worktree and WORKING_TREE removes the directory.
    // Deliberately omit LOCKED so a reacquired lock still protects it.
    let mut options = GitPruneOptions {
        version: 1,
        flags: 1 | 4,
    };
    let main = main_repo(repo_path)?;
    let registration = unsafe { path_from_c(git_repository_commondir(main.0))? }
        .join("worktrees")
        .join(name);
    check(
        unsafe { git_worktree_prune(worktree.0, &mut options) },
        "remove worktree",
    )
    .map_err(|reason| removal_failure(reason, path, &registration))
}

pub fn inspect(
    repo_path: &Path,
    name: &OsStr,
    path: &Path,
    options: &Options,
    cwd: &Path,
    expected: Option<&Inspection>,
) -> Result<Inspection, String> {
    let mut reply = None;
    let mut checked = evaluate(repo_path, name, path, options, cwd, &mut reply)?;
    if expected.is_some() && reply.is_some() {
        checked = evaluate(repo_path, name, path, options, cwd, &mut reply)?;
    }
    let current = checked.inspection;
    let Some(expected) = expected else {
        return Ok(current);
    };
    let protected = |inspection: &Inspection| {
        inspection.unmerged
            || inspection.github.is_some()
            || inspection.lock.is_some()
            || inspection.detached.is_some()
    };
    if (protected(&current) || protected(expected)) && current.evidence() != expected.evidence() {
        return Err(
            "worktree branch, commit, lock or integration evidence changed after selection".into(),
        );
    }
    if let Some((lock_path, lock)) = &current.lock {
        locks::reclaim_and_remove(
            lock_path,
            path,
            name,
            lock,
            |unlocked| {
                let verified =
                    evaluate(repo_path, name, path, options, cwd, &mut reply)?.inspection;
                let mut wanted = expected.clone();
                if unlocked {
                    wanted.lock = None;
                }
                if verified.evidence() != wanted.evidence() {
                    return Err(
                        "worktree or integration evidence changed during lock reclamation".into(),
                    );
                }
                Ok(())
            },
            || prune(&checked.worktree, repo_path, name, path),
        )?;
    } else {
        prune(&checked.worktree, repo_path, name, path)?;
    }
    Ok(current)
}

fn removal_failure(reason: String, path: &Path, registration: &Path) -> String {
    let state = |path: &Path| match fs::symlink_metadata(path) {
        Ok(_) => "present (contents may be partially removed)".to_owned(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent".to_owned(),
        Err(error) => format!("unknown ({error})"),
    };
    format!(
        "{reason}; removal may be partial; worktree {}: {}; registration {}: {}",
        path.display(),
        state(path),
        registration.display(),
        state(registration)
    )
}

#[cfg(test)]
mod removal_tests {
    use super::*;
    #[test]
    fn adr0004_invariant_2_partial_removal_reports_remaining_state() {
        let fixture = crate::common::Fixture::new();
        let worktree = fixture.path.join("worktree");
        fs::create_dir(&worktree).unwrap();
        let registration = fixture.path.join("missing-registration");
        let error = removal_failure("prune failed".into(), &worktree, &registration);
        assert!(error.contains("removal may be partial"));
        assert!(error.contains("present (contents may be partially removed)"));
        assert!(error.ends_with(": absent"));
    }

    #[test]
    fn adr0008_i9_recheck_keeps_worktree_with_new_untracked_file() {
        let fixture = crate::common::Fixture::new();
        let repo = fixture.repo("project", "main");
        fs::write(repo.join(".gitignore"), "*.log\n").unwrap();
        crate::common::git(&repo, &["add", ".gitignore"]);
        crate::common::git(&repo, &["commit", "-m", "ignore logs"]);
        let worktree = fixture.worktree(&repo, "done");
        fs::write(worktree.join("build.log"), "log\n").unwrap();
        init().unwrap();
        let options = Options {
            base: Some("main".into()),
            allow_ignored: true,
            min_idle: 0,
            ..Options::default()
        };
        let name = OsStr::new("done-worktree");
        let selected = inspect(&repo, name, &worktree, &options, &repo, None).unwrap();
        assert_eq!(
            selected.reason(),
            "clean and integrated; deletes 1 ignored files"
        );
        fs::write(worktree.join("notes"), "work\n").unwrap();
        let error = inspect(&repo, name, &worktree, &options, &repo, Some(&selected)).unwrap_err();
        assert_eq!(error, "untracked file: notes");
        assert_eq!(fs::read(worktree.join("notes")).unwrap(), b"work\n");
        assert!(worktree.join("build.log").exists());
        fs::remove_file(worktree.join("notes")).unwrap();
        inspect(&repo, name, &worktree, &options, &repo, Some(&selected)).unwrap();
        assert!(!worktree.exists());
    }
}

#[cfg(test)]
mod adr0010_tests {
    use super::*;
    use crate::{Action, Options};
    use std::time::Duration;

    const HOUR: Duration = Duration::from_secs(3600);

    fn files(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|name| dir.join(name)).collect()
    }

    fn write_activity(dir: &Path, names: &[&str], time: SystemTime) {
        for path in files(dir, names) {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "x").unwrap();
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(time)
                .unwrap();
        }
    }

    fn admin_dir(worktree: &Path) -> PathBuf {
        let output = crate::common::git(worktree, &["rev-parse", "--absolute-git-dir"]);
        PathBuf::from(output.trim())
    }

    fn set_activity(worktree: &Path, names: &[&str], time: SystemTime) {
        for path in files(&admin_dir(worktree), names) {
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(time)
                .unwrap();
        }
    }

    #[test]
    fn adr0010_i1_min_idle_zero_reads_nothing() {
        let fixture = crate::common::Fixture::new();
        let now = SystemTime::now();
        let empty = fixture.path.join("empty");
        fs::create_dir(&empty).unwrap();
        assert_eq!(ensure_idle(&empty, 0, now), Ok(()));
        let future = fixture.path.join("future");
        write_activity(&future, &ACTIVITY_FILES, now + HOUR);
        assert_eq!(ensure_idle(&future, 0, now), Ok(()));
        let fresh = fixture.path.join("fresh");
        write_activity(&fresh, &ACTIVITY_FILES, now);
        assert_eq!(ensure_idle(&fresh, 0, now), Ok(()));
        // An unreadable time is not read at all with --min-idle 0.
        let blocked = fixture.path.join("blocked");
        fs::write(&blocked, "a file, not a directory").unwrap();
        assert_eq!(ensure_idle(&blocked, 0, now), Ok(()));
    }

    #[test]
    fn adr0010_i2_i9_less_than_min_idle_is_kept_with_truncated_hours() {
        let now = SystemTime::now();
        assert_eq!(
            idle_since(now - (24 * HOUR - Duration::from_secs(60)), now, 24),
            Err("worktree had Git activity 23 hours ago (less than --min-idle 24)".into())
        );
        assert_eq!(
            idle_since(now - (24 * HOUR - Duration::from_nanos(1)), now, 24),
            Err("worktree had Git activity 23 hours ago (less than --min-idle 24)".into())
        );
        assert_eq!(
            idle_since(now - 25 * HOUR, now, 48),
            Err("worktree had Git activity 25 hours ago (less than --min-idle 48)".into())
        );
        assert_eq!(
            idle_since(now, now, 1),
            Err("worktree had Git activity 0 hours ago (less than --min-idle 1)".into())
        );
        assert_eq!(
            idle_since(now - 30 * HOUR, now, u64::MAX),
            Err(format!(
                "worktree had Git activity 30 hours ago (less than --min-idle {})",
                u64::MAX
            ))
        );
    }

    #[test]
    fn adr0010_i3_min_idle_or_more_is_eligible() {
        let now = SystemTime::now();
        assert_eq!(idle_since(now - 24 * HOUR, now, 24), Ok(()));
        assert_eq!(idle_since(now - 25 * HOUR, now, 24), Ok(()));
        assert_eq!(idle_since(now - HOUR, now, 1), Ok(()));
    }

    #[test]
    fn adr0010_i3_latest_existing_file_decides() {
        let fixture = crate::common::Fixture::new();
        let now = SystemTime::now();
        let only_head = fixture.path.join("only-head");
        write_activity(&only_head, &["HEAD"], now - 30 * HOUR);
        assert_eq!(ensure_idle(&only_head, 24, now), Ok(()));
        let mixed = fixture.path.join("mixed");
        write_activity(&mixed, &["HEAD", "logs/HEAD"], now - 30 * HOUR);
        write_activity(&mixed, &["index"], now - 2 * HOUR);
        assert_eq!(
            ensure_idle(&mixed, 24, now),
            Err("worktree had Git activity 2 hours ago (less than --min-idle 24)".into())
        );
    }

    #[test]
    fn adr0010_i4_future_activity_is_kept() {
        let now = SystemTime::now();
        assert_eq!(
            idle_since(now + HOUR, now, 24),
            Err("worktree Git activity time is in the future".into())
        );
        assert_eq!(
            idle_since(now + Duration::from_nanos(1), now, 1),
            Err("worktree Git activity time is in the future".into())
        );
    }

    #[test]
    fn adr0010_i5_no_activity_file_is_kept() {
        let fixture = crate::common::Fixture::new();
        let empty = fixture.path.join("empty");
        fs::create_dir(&empty).unwrap();
        assert_eq!(
            ensure_idle(&empty, 24, SystemTime::now()),
            Err("cannot read worktree activity: no HEAD, index or logs/HEAD".into())
        );
    }

    #[test]
    fn adr0010_i5_unreadable_time_is_kept() {
        let fixture = crate::common::Fixture::new();
        let now = SystemTime::now();
        write_activity(&fixture.path, &["HEAD"], now - 30 * HOUR);
        // A path below a regular file cannot be inspected (ENOTDIR), unlike a missing file.
        let blocker = fixture.path.join("blocker");
        fs::write(&blocker, "x").unwrap();
        let unreadable = blocker.join("index");
        let error = last_activity(&[fixture.path.join("HEAD"), unreadable.clone()]).unwrap_err();
        let prefix = format!("cannot read worktree activity: {}: ", unreadable.display());
        assert!(error.starts_with(&prefix), "{error}");
        assert!(error.len() > prefix.len(), "{error}");
    }

    #[test]
    fn adr0010_i6_activity_after_selection_prevents_removal() {
        let fixture = crate::common::Fixture::new();
        let repo = fixture.repo("repo", "main");
        let worktree = fixture.worktree(&repo, "done");
        let name = OsString::from("done-worktree");
        set_activity(&worktree, &ACTIVITY_FILES, SystemTime::now() - 30 * HOUR);
        init().unwrap();
        let options = Options {
            action: Action::All,
            ..Options::default()
        };
        assert_eq!(options.min_idle, 24);
        let selected = inspect(&repo, &name, &worktree, &options, &repo, None).unwrap();
        set_activity(&worktree, &["index"], SystemTime::now());
        let error = inspect(&repo, &name, &worktree, &options, &repo, Some(&selected)).unwrap_err();
        assert_eq!(
            error,
            "worktree had Git activity 0 hours ago (less than --min-idle 24)"
        );
        assert!(worktree.exists());
    }
}
