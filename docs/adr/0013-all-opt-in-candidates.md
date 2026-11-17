# ADR 0013: `--all` で、候補を広げる指定をすべて有効にする

Status: proposed

## 文脈

worktree-cleaner の既定の判定は保守的であり、候補を広げるには指定を一つずつ足す。
候補を広げる指定は、次の 7 つである。

| 指定 | 決めた ADR | 広げる範囲 |
| --- | --- | --- |
| `--include-unmerged` | [ADR 0002](0002-include-unmerged-worktrees.md) | 未マージのブランチ上の worktree |
| `--github-merged` | [ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) | GitHub でマージ済みの PR の先端にある worktree |
| `--reclaim-agent-locks` | [ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) | 所有者が終了した Claude エージェントのロック付きの worktree |
| `--allow-ignored` | [ADR 0008](0008-allow-ignored-files.md) | 無視ファイルだけがある worktree |
| `--remote-base` | [ADR 0009](0009-remote-tracking-base.md) | `origin/<名前>` に取り込まれた worktree |
| `--min-idle 0` | [ADR 0010](0010-keep-recently-active-worktrees.md) | 最近の操作がある worktree |
| `--include-detached` | [ADR 0011](0011-include-detached-integrated-worktrees.md) | 基準に含まれる detached HEAD の worktree |

この ADR では、この 7 つを「候補を広げる指定」と呼ぶ。

しかし、エージェントが残した worktree を片付けるときは、候補を広げる指定を毎回すべて並べることになる。
実際、2026-11-17 の整理でも、7 つのうち 5 つを並べた一覧を使った（調査した事項）。
並べ忘れると、消せる worktree が保持されたまま残る。
そこで、候補を広げる指定をまとめて有効にする `--all` を足し、一つの指定で候補が最も多くなる判定を選べるようにする。

この ADR は、既存の指定の組み合わせを一つの名前で選べるようにするだけであり、判定の条件は変えない。
そのため、既存の ADR のステータスは変えない。

## 調査した事項

2026-11-17 に、main の `979f6dc` をビルドした worktree-cleaner を `~/repositories/src/github.com/tett23` で実行した。

- `--github-merged --reclaim-agent-locks` の一覧は 164 行で、候補は 2 件だった。
- `--remote-base --github-merged --reclaim-agent-locks --allow-ignored --include-detached` の一覧は 128 行で、候補は 7 件だった。二つの実行の間に、`/private/tmp/sofel-*` の worktree の登録がこの CLI の外で消えたため、行の数が違う。
- 7 件の候補の理由は、`clean and integrated into origin/main; deletes N ignored files` が 5 件、`clean detached HEAD <コミット ID> is integrated into origin/main; deletes N ignored files` が 2 件だった。N は 0 が 2 件、458 から 6219 が 5 件だった。
- 同じ一覧で、`no matching integrated pull request for this branch tip` で保持した行が 10 件あった。`--github-merged` は clean なブランチだけを GitHub に問い合わせるため（[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md)）、これらは `--include-unmerged` を足せば候補になりうる。
- 同じ一覧で、最近の操作を理由に保持した行が 45 件あった（`worktree had Git activity N hours ago (less than --min-idle 24)`）。
- `src/main.rs:79-121` の `options` は、各指定を一度だけ受け付け、同じ指定が二度あれば `unexpected or repeated option: <指定>` で終わる。
- `src/locks.rs` は、macOS 以外ではロックの所有者を判定できないとして保持する（`#[cfg(not(target_os = "macos"))]`）。そのため、`--reclaim-agent-locks` は macOS 以外でもエラーにならない。

## 実装すること

`--all` を足す。
`--all` を指定した実行は、`--all` を候補を広げる指定の 7 つに置き換えた実行と同じ判定をする。
つまり、`--include-unmerged --include-detached --github-merged --reclaim-agent-locks --allow-ignored --remote-base --min-idle 0` と同じである。

ただし、`--min-idle <時間数>` を `--all` と一緒に指定したときは、その時間数を使う。
例えば、`--all --min-idle 24` は、最近の操作の保護だけを既定のまま残す。

`--all` は、`--root`、`--base`、`--min-idle`、`--remove PATH`、`--remove-all` と組み合わせられる。
例えば、`worktree-cleaner --all` は候補を一覧にし、`worktree-cleaner --all --remove-all` はそれらを削除する。

`--all` と、候補を広げる指定のどれかを一緒に指定してもよい。
その実行は、`--all` だけを指定した実行と同じ判定をする。
`--all` を二度指定したときは、ほかの指定と同じく `unexpected or repeated option: --all` で終了コード 1 で終わる。

使い方の行（`--help`）、README、仕様書に `--all` を足し、`--all` が有効にする指定と、`--min-idle` を一緒に指定したときの振る舞いを書く。

## 実装しないこと

- ローカルの基準ブランチと `origin/<名前>` の両方を見る判定。`--all` は `--remote-base` と同じく `origin/<名前>` だけを基準にする。
- origin の fetch。[ADR 0009](0009-remote-tracking-base.md) のとおり、利用者が先に fetch する。
- 現在地を含む worktree の保護の解除。`--all` を指定しても、[ADR 0001](0001-worktree-cleanup.md) のとおり保持する。
- 候補を広げる指定のうち一部を打ち消す指定（`--no-allow-ignored` など）。`--min-idle` だけは、上のとおり一緒に指定した値を使う。
- 今後足す、候補を広げる指定を `--all` に自動で含めること。足すときは、その ADR で `--all` に含めるかを決める。

## 不変条件

- I1: `--all` を指定し、`--min-idle` を指定しない一覧、単一削除、一括削除の候補と理由は、`--all` を候補を広げる指定の 7 つに置き換えた実行の候補と理由と同じである。
- I2: `--all` と `--min-idle <N>` を指定した実行の候補と理由は、`--all` を `--min-idle 0` を除く 6 つに置き換えた実行の候補と理由と同じである。
- I3: `--all` と、`--min-idle` を除く候補を広げる指定のどれかを一緒に指定した実行の候補と理由は、`--all` だけを指定した実行と同じである。
- I4: `--all` を二度指定した実行は、一覧も削除もせずに終了コード 1 で終わる。
- I5: `--all` を指定しない実行の候補と理由は、この ADR を入れる前と同じである。

| 入力（ほかの条件は成立） | 期待 | 不変条件 |
| --- | --- | --- |
| 無視ファイルだけがあり、`origin/main` にだけ含まれる detached HEAD の worktree、`--all` | 候補、理由は 7 つを並べたときと同じ | I1 |
| 未マージのブランチ上の clean な worktree、`--all` | 候補（ブランチは残る） | I1 |
| 1 時間前に操作した統合済みの worktree、`--all` | 候補 | I1 |
| 1 時間前に操作した統合済みの worktree、`--all --min-idle 24` | 保持（最近の操作） | I2 |
| 1 時間前に操作した統合済みの worktree、`--all --min-idle 0` | 候補 | I2 |
| `--all --allow-ignored` | `--all` と同じ | I3 |
| `--all --all` | 終了コード 1、標準出力は空 | I4 |
| 無視ファイルがある統合済みの worktree、指定なし | 保持 | I5 |

## 受けいれ条件

- A1: `~/repositories/src/github.com/tett23` で `--all` の一覧と、候補を広げる指定の 7 つを並べた一覧を続けて取り、二つの出力が一致することを手で確かめる。

## テスト設計

テストの名前は `adr0013_i<番号>_<内容>` とする。

- I1 は、一時リポジトリに境界の表の最初の 3 行の worktree を作り、`--all` の一覧と 7 つを並べた一覧の標準出力が一致することを確かめる。`--github-merged` の問い合わせには、今のテストと同じく偽の gh を使う。続けて `--all --remove-all` で削除し、ブランチが残ることを確かめる。
- I2 は、管理ディレクトリの `HEAD` の更新時刻を 1 時間前にした統合済みの worktree で、`--all --min-idle 24` では保持し、`--all --min-idle 0` では候補になることを確かめる。どちらも、`--min-idle 0` を除く 6 つに同じ `--min-idle` を足した一覧と標準出力が一致することも確かめる。
- I3 は、`--min-idle` を除く候補を広げる指定を一つずつ `--all` に足した実行の標準出力が、`--all` だけの実行と一致することを確かめる。
- I4 は、`--all --all` の終了コードが 1 で、標準出力が空であることを確かめる。
- I5 は、今あるテストが変わらずに通ることで確かめる。

## トレードオフ

- `--all` は最近の操作の保護を外すため、エージェントや利用者がまだ使っている worktree も、ほかの条件を満たせば消す。2026-11-17 の一覧では、この保護で保持した行が 45 件あった。保護を残したいときは、`--all --min-idle 24` のように時間数を一緒に指定する。
- `--all` と `--remove-all` は名前が似ている。`--all` は候補の範囲を、`--remove-all` は動作を決める。取り違えても、`--all` だけなら一覧しかしないため、意図しない削除は起きない。
- `--all` は無視ファイルも消す。2026-11-17 の一覧では、一件で 6219 個の無視ファイルを消す候補があった。利用者は、理由の末尾の数を見てから削除する必要がある。
- `--all` は `--include-unmerged` を含むため、未マージのブランチ上の worktree も消す。コミットした内容はブランチに残るが、利用者が後でブランチを消すと、そこから辿れなくなる。
- `--all` は `--github-merged` を含むため、ネットワークと認証済みの gh を使う。gh がなければ、該当する worktree は理由を示して保持する。
- 基準を `origin/<名前>` に固定するため、ローカルの基準ブランチが `origin/<名前>` より進んでいるリポジトリでは、`--all` を付けないほうが候補が多い場合がある。

## 参考文献

- [ADR 0001](0001-worktree-cleanup.md)
- [ADR 0002](0002-include-unmerged-worktrees.md)
- [ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md)
- [ADR 0008](0008-allow-ignored-files.md)
- [ADR 0009](0009-remote-tracking-base.md)
- [ADR 0010](0010-keep-recently-active-worktrees.md)
- [ADR 0011](0011-include-detached-integrated-worktrees.md)
