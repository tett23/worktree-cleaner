# worktree-cleaner

worktree-cleaner lists Git linked worktrees whose commits have been integrated into a local base branch. It can remove their working directories and Git registrations while retaining the branches.

## Usage

Run the command inside a repository to inspect its linked worktrees. From a directory containing several repositories, the same command inspects each immediate child that is a main worktree. You can also select this directory with `--root`. Registered worktrees can live outside that directory.

```sh
worktree-cleaner --root ~/repositories/src/github.com/tett23
worktree-cleaner --root ~/repositories/src/github.com/tett23 --remove-all
```

The first command lists each worktree with its repository, path, decision and reason. The second removes eligible worktrees after checking each one again. To remove just one, pass `--remove PATH` in the same search scope. Finish other operations in the selected worktrees before removal because checking and deleting files are separate operations.

Each repository uses the local branch named by its locally recorded `origin/HEAD`. For example, `origin/main` selects the local `main` branch. Supply `--base main` to choose that local branch explicitly. If the base cannot be resolved, the repository's worktrees are kept.

By default, a candidate must be on a local branch whose tip is reachable from the base tip. The cleaner keeps the base branch's worktree, locked worktrees and the worktree containing the current directory. Staged or unstaged changes, untracked or ignored files, nested repositories, submodules, an unfinished Git operation or an inconclusive check also prevent removal.

Use `--include-unmerged` to list clean worktrees whose commits are still on an unmerged local branch. Add `--remove PATH` or `--remove-all` to remove them while keeping the branch. The cleaner checks the branch name and commit again before removal. You can check out that branch later to recover the committed files.

## Installation

Install stable Rust and the system libgit2 development library, then build from this checkout.

```sh
cargo install --path . --locked
```

If libgit2 is outside the linker's search path, set `RUSTFLAGS="-L native=/path/to/libgit2/lib"` before building.

## Development

Install [adr-guard](https://github.com/tett23/adr-guard) and enable the repository hook after cloning. The integration tests use Git to create disposable repositories.

```sh
git config core.hooksPath .githooks
cargo build --locked
cargo test --locked
```

CI checks document changes and protects committed decisions, audit reports and benchmark records. Changes confined to recognized documentation files skip the Rust build and tests.

## License

[MIT](LICENSE)
