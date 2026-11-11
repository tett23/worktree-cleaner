//! Small, direct bindings to the installed libgit2 API used for worktree cleanup.

use crate::{Options, github, locks};
use std::ffi::{CStr, CString, OsStr, OsString, c_char, c_int, c_void};
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;

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
}

impl Inspection {
    pub fn reason(&self) -> String {
        let mut reason = if let Some(pr) = &self.github {
            format!("clean and integrated via {}", pr.url)
        } else if self.unmerged {
            format!("clean unmerged branch {}", self.branch.to_string_lossy())
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
        reason
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

fn base_ref(repo: &Repo, explicit: Option<&OsStr>) -> Result<Reference, String> {
    let branch = if let Some(name) = explicit {
        name.as_bytes().to_vec()
    } else {
        let origin_head = lookup_reference(repo, c"refs/remotes/origin/HEAD")?;
        let target = unsafe { git_reference_symbolic_target(origin_head.0) };
        if target.is_null() {
            return Err("origin/HEAD is not symbolic".to_owned());
        }
        unsafe { CStr::from_ptr(target) }
            .to_bytes()
            .strip_prefix(b"refs/remotes/origin/")
            .filter(|name| !name.is_empty())
            .ok_or("origin/HEAD does not name an origin branch")?
            .to_vec()
    };
    let mut full = b"refs/heads/".to_vec();
    full.extend(branch);
    lookup_reference(
        repo,
        &CString::new(full).map_err(|_| "invalid base branch".to_owned())?,
    )
}

unsafe extern "C" fn any_change(_path: *const c_char, _flags: u32, payload: *mut c_void) -> c_int {
    unsafe {
        *(payload as *mut bool) = true;
    }
    1
}

fn ensure_clean(repo: &Repo, root: &Path) -> Result<(), String> {
    let mut dirty = false;
    let code = unsafe { git_status_foreach(repo.0, any_change, (&mut dirty as *mut bool).cast()) };
    if dirty {
        return Err(
            "working tree or index has changes, including untracked or ignored files".to_owned(),
        );
    }
    check(code, "read worktree status")?;

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
                    return Err(format!("untracked file: {}", item.path().display()));
                }
                if metadata.is_file() {
                    fs::File::open(item.path())
                        .map_err(|e| format!("{}: {e}", item.path().display()))?;
                }
            }
        }
    }
    Ok(())
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
    target: &Reference,
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
    let base = unsafe { CStr::from_ptr(git_reference_name(target.0)) }
        .to_str()
        .ok()
        .and_then(|name| name.strip_prefix("refs/heads/"))
        .ok_or("invalid base branch")?;
    let head_hex: String = head.iter().map(|byte| format!("{byte:02x}")).collect();
    let base_oid = unsafe { git_reference_target(target.0) };
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
            _ => return Err(error("check PR merge commit ancestry")),
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
    let target = base_ref(&main, options.base.as_deref())?;
    let head = {
        let mut pointer = ptr::null_mut();
        check(
            unsafe { git_repository_head(&mut pointer, linked.0) },
            "resolve worktree HEAD",
        )?;
        Reference(pointer)
    };
    let head_name = unsafe { git_reference_name(head.0) };
    if head_name.is_null()
        || !unsafe { CStr::from_ptr(head_name) }
            .to_bytes()
            .starts_with(b"refs/heads/")
    {
        return Err("worktree HEAD is detached".to_owned());
    }
    if unsafe { CStr::from_ptr(head_name) }
        == unsafe { CStr::from_ptr(git_reference_name(target.0)) }
    {
        return Err("worktree uses base branch".to_owned());
    }
    let target_oid = unsafe { git_reference_target(target.0) };
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
    ensure_clean(&linked, &registered)?;
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
    };
    Ok(Checked {
        inspection,
        worktree,
    })
}

fn prune(worktree: &Worktree) -> Result<(), String> {
    // VALID permits an existing worktree and WORKING_TREE removes the directory.
    // Deliberately omit LOCKED so a reacquired lock still protects it.
    let mut options = GitPruneOptions {
        version: 1,
        flags: 1 | 4,
    };
    check(
        unsafe { git_worktree_prune(worktree.0, &mut options) },
        "remove worktree",
    )
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
        inspection.unmerged || inspection.github.is_some() || inspection.lock.is_some()
    };
    if (protected(&current) || protected(expected)) && &current != expected {
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
                if verified != wanted {
                    return Err(
                        "worktree or integration evidence changed during lock reclamation".into(),
                    );
                }
                Ok(())
            },
            || prune(&checked.worktree),
        )?;
    } else {
        prune(&checked.worktree)?;
    }
    Ok(current)
}
