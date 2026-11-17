# worktree-cleaner

worktree-cleaner lists Git linked worktrees whose commits have been integrated into a base branch, which is a local branch unless you opt into its remote-tracking counterpart. It can remove their working directories and Git registrations while retaining the branches.

## Usage

Run the command inside a repository to inspect its linked worktrees. From a directory containing several repositories, the same command inspects each immediate child that is a main worktree. You can also select this directory with `--root`. Registered worktrees can live outside that directory.

```sh
worktree-cleaner --root ~/repositories/src/github.com/tett23
worktree-cleaner --root ~/repositories/src/github.com/tett23 --remove-all
```

The first command lists each worktree with its repository, path, decision and reason. The second removes eligible worktrees after checking each one again. To remove just one, pass `--remove PATH` in the same search scope. Finish other operations in the selected worktrees before removal because checking and deleting files are separate operations.

Each repository uses the local branch named by its locally recorded `origin/HEAD`. For example, `origin/main` selects the local `main` branch. Supply `--base main` to choose that local branch explicitly. If the base cannot be resolved, the repository's worktrees are kept.

If you merge pull requests on GitHub and only fetch locally, the local base branch falls behind `origin`. Pass `--remote-base` to measure reachability against `refs/remotes/origin/<name>` instead, where `<name>` is chosen in the same way as above. The cleaner never fetches, so this reference holds whatever your last fetch recorded. Worktrees integrated this way are listed as `clean and integrated into origin/<name>`. The worktree that has the local branch `<name>` checked out is still kept, and a repository without `refs/remotes/origin/<name>` keeps all of its worktrees. With `--include-unmerged`, a HEAD that `origin/<name>` cannot reach counts as unmerged. With `--github-merged`, the merge commit must be reachable from `origin/<name>`. Without `--remote-base`, decisions are unchanged.

```sh
git -C ~/repositories/src/github.com/tett23/project fetch origin
worktree-cleaner --root ~/repositories/src/github.com/tett23 --remote-base
```

By default, a candidate must be on a local branch whose tip is reachable from the base tip. The cleaner keeps the base branch's worktree, locked worktrees by default, and the worktree containing the current directory. Staged or unstaged changes, untracked or ignored files, nested repositories, submodules, an unfinished Git operation or an inconclusive check also prevent removal.

The cleaner also keeps a worktree with recent Git activity by default. The activity time is the latest modification time among the `HEAD`, `index` and `logs/HEAD` files in the worktree's administrative directory, `worktrees/<name>` in the repository's Git directory. A worktree is kept if this time is less than 24 hours ago, lies in the future, or cannot be read because none of the files exists or a time is unreadable. For recent activity, the reason field shows how many whole hours ago it happened. Because this check runs before the branch and cleanliness checks, a recently active worktree reports this reason even if it is detached, unmerged or dirty. Removal checks the time again just before deleting. Use `--min-idle HOURS` to change the period, or `--min-idle 0` to turn the check off.

```sh
worktree-cleaner --root ~/repositories/src/github.com/tett23 --min-idle 0
```

Use `--include-unmerged` to list clean worktrees whose commits are still on an unmerged local branch. Add `--remove PATH` or `--remove-all` to remove them while keeping the branch. The cleaner checks the branch name and commit again before removal. You can check out that branch later to recover the committed files.

Use `--allow-ignored` when ignored files such as build output may be deleted. A worktree whose only untracked files are ignored then becomes a candidate if it meets every other condition, and removal deletes those files with the worktree. The reason column ends with the number of ignored files to be deleted, for example `clean and integrated; deletes 2 ignored files`. Untracked files that are not ignored, staged or unstaged changes, and nested repositories or special files inside ignored directories still prevent removal. The option combines with `--include-unmerged`, `--github-merged` and `--reclaim-agent-locks`.

Use `--github-merged` to check a clean, divergent branch against GitHub.com with an authenticated `gh` executable. This also works for squash or rebase merges if the merged PR has the exact local branch tip and its merge commit is in the base branch. The listing shows the matching PR URL. The cleaner queries it again before removal. Without this option, cleanup runs offline.

On macOS, `--reclaim-agent-locks` checks Claude agent locks whose recorded owner has exited. Use it only for private worktrees created on this host, with no concurrent use or lock changes. A reused PID qualifies only when the recorded start times establish their order despite timezone uncertainty. The option keeps manual locks and worktrees with uncommitted files, and also those with ignored files unless `--allow-ignored` is given. A locked worktree needs evidence of integration even when `--include-unmerged` is set.

```sh
worktree-cleaner --root ~/repositories/src/github.com/tett23 --reclaim-agent-locks --github-merged
```

This command previews eligible worktrees without releasing any lock. Add `--remove PATH` or `--remove-all` to reclaim eligible locks and remove the corresponding worktrees.

## Installation

Install stable Rust and the system libgit2 development library, then build from this checkout.

```sh
cargo install --path . --locked
```

If libgit2 is outside the linker's search path, set `RUSTFLAGS="-L native=/path/to/libgit2/lib"` before building.

## Development

Install [adr-guard](https://github.com/tett23/adr-guard) v0.13.1 or later and enable the repository hook after cloning. The integration tests use Git to create disposable repositories.

Each ADR has exactly one status line of the form `Status: <token>`, where the token is `proposed`, `accepted`, `accepted-pending`, `rejected`, `discarded`, `deprecated`, or `superseded [<JSON array of changes>]`. Commit a new ADR with `Status: proposed` first, and change its status in a later commit. The pre-commit hook and CI check the state transitions of status lines ([ADR 0005](docs/adr/0005-update-adr-guard-to-v0.13.1.md)).

```sh
git config core.hooksPath .githooks
cargo build --locked
cargo test --locked
```

CI checks document changes and protects committed decisions, audit reports and benchmark records. Changes confined to recognized documentation files skip the Rust build and tests.

## License

[MIT](LICENSE)

## Removal results

A failed removal is reported as a `keep` row with its cause, and processing continues with the remaining candidates. A failed deletion may leave partially removed files or registration data, which the reason field reports along with any lock recovery result.

After a removal run, standard error shows the attempted, removed and failed counts. Any failed attempt produces exit code 1 after the remaining candidates have been processed. Worktrees excluded during the initial inspection do not count as failed attempts. If a result cannot be written to standard output, cleanup stops before attempting the next candidate. Listing runs and errors during candidate selection do not print a removal summary.
