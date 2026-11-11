# worktree-cleaner

worktree-cleaner is a Rust project for removing Git linked worktrees after their work has been integrated. Cleanup is currently in design. The initial crate contains no cleanup commands.

## Development

Install stable Rust and Cargo. The planned Git backend is the system libgit2 library. The initial crate uses only the Rust standard library.

Install [adr-guard](https://github.com/tett23/adr-guard) and enable the repository hook after cloning.

```sh
git config core.hooksPath .githooks
cargo build --locked
cargo test --locked
```

CI checks document changes and protects committed decisions, audit reports and benchmark records. Changes confined to recognized documentation files skip the Rust build and tests.

## License

[MIT](LICENSE)
