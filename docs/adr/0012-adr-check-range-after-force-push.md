# ADR 0012: force push の後は、main との merge-base から ADR を検査する

Status: accepted

## 文脈

CI の `adr` ジョブは、push のとき、検査の範囲の始まり（`base`）を `.github/workflows/ci.yml` の step（`id: range`）で決め、adr-guard の action に渡す。
この step は [ADR 0006](0006-adr-check-range-for-new-branches.md) が決めた。
push で `github.event.before` が空でも `0000000000000000000000000000000000000000` でもなければ、step は `before` をそのまま `base` にする。
それ以外（新しいブランチやタグの最初の push）は、`git merge-base origin/main "$HEAD"` を `base` にする。

しかし、force push では `before` は push の前のブランチの先端で、push の後の先端（`HEAD`）の祖先ではない。
PR のブランチを rebase して push し直すと、この状態になる。
実際、2026-11-17 に force push で動いた push の `adr` ジョブは、6 回とも全履歴を検査して失敗した（「調査した事項」）。
失敗の原因は、[ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) を旧形式のステータス行で追加または変更したコミットへの指摘である。
旧形式のステータス行と、それを adr-guard v0.13.1 が拒否する理由は、[ADR 0006](0006-adr-check-range-for-new-branches.md) の「文脈」に書いてある。

## 調査した事項

2026-11-17 に、[ADR 0007](0007-explain-keep-reasons-by-cause.md)、[ADR 0008](0008-allow-ignored-files.md)、[ADR 0010](0010-keep-recently-active-worktrees.md)、[ADR 0011](0011-include-detached-integrated-worktrees.md) の実装の PR（tett23/worktree-cleaner#12 から #15）をマージする間に、push の `adr` ジョブが 6 回失敗した。
`gh run list --event push --created 2026-11-17` と `gh run view <run> --log` で、次を確かめた。
どの run も、`changes` と `rust` のジョブは成功し、`adr` のジョブだけが失敗した。

| run | ブランチ | `BASE_SHA`（`before`） | `HEAD_SHA` |
| --- | --- | --- | --- |
| 44061207472 | implement-adr-0008 | 5ff2b7a | deb000c |
| 44061305671 | implement-adr-0010 | 628108f | 5b71061 |
| 44063201918 | implement-adr-0010 | 5b71061 | 5d77b36 |
| 44063958993 | implement-adr-0011 | 28c8356 | d3d744d |
| 44064005309 | implement-adr-0007 | acc8171 | 590a028 |
| 44064893016 | implement-adr-0007 | 590a028 | 5bb9a5b |

- 6 つの組のどれも、手元の clone で `git merge-base --is-ancestor <BASE_SHA> <HEAD_SHA>` が終了コード 1 を返した。どれも force push である。
- 6 つの run のどれも、`check-range` は 6e0f781、2f24639、60e002b、6db675d、9cac767、8858ce5、b8e0a4f の 7 つのコミットに ``新形式のステータス行は `Status: ` で始めてください`` を出し、終了コード 1 で終わった。どれも [ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) を旧形式で追加または変更したコミットで、[ADR 0006](0006-adr-check-range-for-new-branches.md) の「調査した事項」の run 43765923699 と同じである。
- 同じ `HEAD_SHA` の `pull_request` の run（44061216526、44061314194、44063208318、44063968708、44064010138、44064902861）は、すべて成功した。
- adr-guard v0.13.1 の `check_range`（tett23/adr-guard の `src/main.rs`）は、`git rev-parse --verify -q <base>^{commit}` が失敗すると、範囲を `<base>..<head>` ではなく `<head>` にする。`<head>` は、ルートコミットからのすべてのコミットを指す。
- `adr` ジョブの `actions/checkout` は、`+refs/heads/*:refs/remotes/origin/*` と `+refs/tags/*:refs/tags/*` を fetch する（run 44063201918 のログ）。
- 手元の clone で、`git merge-base --is-ancestor` は、祖先なら 0、祖先でなければ 1、`<base>` が `0000000000000000000000000000000000000000`、空、存在しないコミットのどれかなら 128 を返した。

推測: force push の後は、push の前の先端を指す ref がリモートに無い。
そのため、その先端は `adr` ジョブの clone に fetch されず、`check-range` は全履歴を検査した、と考える。
CI の clone に `before` が無かったことは、ログからは直接確かめていない（未確認）。
この推測が誤っていても、下の「実装すること」の判定は変わらない。
判定は、`before` が clone にあるかではなく、`before` が `HEAD` の祖先かで分かれ、祖先でなければどちらの場合も merge-base から検査するためである。

## 実装すること

- `.github/workflows/ci.yml` の `adr` ジョブの `id: range` の step で、push のときの `base` を次のとおり決める。
  - `git merge-base --is-ancestor "$BEFORE" "$HEAD"` が成功すれば、`before` を `base` にする。
  - 成功しなければ、`git merge-base origin/main "$HEAD"` を `base` にする。
- この判定は、[ADR 0006](0006-adr-check-range-for-new-branches.md) の「`before` が空でも `0000…` でもない」という判定を置き換える。`before` が空、`0000…`、clone に無いコミット、`HEAD` の祖先でないコミットのどれでも、`git merge-base --is-ancestor` は成功しないためである。
- `pull_request` のときの `base` と、`head` の決め方は変えない。
- step の前の注釈を、この ADR と [ADR 0006](0006-adr-check-range-for-new-branches.md) を指すように直す。
- 採択の後、[ADR 0006](0006-adr-check-range-for-new-branches.md) のステータスを `superseded [{"part":"実装すること","by":["docs/adr/0012-adr-check-range-after-force-push.md"]}]` にする。

## 実装しないこと

- 履歴の ADR を追加したコミットの書き換え（rebase など）。
- force push の禁止。
- push の `adr` ジョブをやめること。
- `changes` ジョブの範囲の決め方の変更。`changes` ジョブは、`before` がコミットとして存在しなければ空の tree と比べる。上の 6 つの run でも、`changes` ジョブは成功した。
- 仕様書と README の変更。どちらも CI の振る舞いを書いていない（[ADR 0006](0006-adr-check-range-for-new-branches.md) の実装でも変えていない）。

## 不変条件

以下で、`HEAD` は push の後の先端（`github.sha`）、`before` は `github.event.before` を指す。

- `I1`: `pull_request` のとき、`base` は `github.event.pull_request.base.sha` である。
- `I2`: push のとき、`base` は `HEAD` の祖先か `HEAD` そのものである（`git merge-base --is-ancestor "$base" "$HEAD"` が成功する）。
- `I3`: push で、`before` が `HEAD` の祖先か `HEAD` そのものなら、`base` は `before` である。
- `I4`: push で、`before` が `HEAD` の祖先でも `HEAD` そのものでもなければ、`base` は `git merge-base origin/main "$HEAD"` である。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| `pull_request` | `base` は PR の base | `I1` |
| push、`before` が `HEAD` の親（ふつうの push） | `base` は `before` | `I2, I3` |
| push、`before` が `HEAD` と同じ | `base` は `before` | `I2, I3` |
| push、`before` が空 | `base` は `origin/main` との merge-base | `I2, I4` |
| push、`before` が `0000000000000000000000000000000000000000`（新しいブランチ） | `base` は `origin/main` との merge-base | `I2, I4` |
| push、`before` が clone に無いコミット（force push） | `base` は `origin/main` との merge-base | `I2, I4` |
| push、`before` が clone にあり、`HEAD` の祖先でないコミット（force push） | `base` は `origin/main` との merge-base | `I2, I4` |

## 受けいれ条件

- `A1`: 実装のブランチを force push したときの、push の `adr` ジョブが成功する。
- `A2`: 実装のブランチを初めて push したときの、push の `adr` ジョブが成功する（[ADR 0006](0006-adr-check-range-for-new-branches.md) の振る舞いを保つ）。
- `A3`: 実装の PR の `pull_request` の `adr` ジョブが成功する。

## テスト設計

自動のテストは書かない。
判定は `ci.yml` の中の数行のシェルで、テストのために別のスクリプトに切り出すと、ワークフローの読みやすさを損なうためである。
実装の後に、次を手で確かめる。

- `I1` から `I4`: `ci.yml` の step の `run` を、一時的なディレクトリの Git リポジトリで、不変条件の表のすべての行の `EVENT`、`BEFORE`、`HEAD` を与えて動かし、`base` の出力が「期待」と一致することを確かめる。一時的なリポジトリには `origin/main` に当たる ref を作る。
- `A2`: 実装のブランチを初めて push し、push の `adr` ジョブのログの `BASE_SHA` が `origin/main` との merge-base で、ジョブが成功することを確かめる。
- `A1`: 実装のブランチの先端のコミットを `git commit --amend` で作り直して force push し、push の `adr` ジョブのログの `BASE_SHA` が `origin/main` との merge-base で、ジョブが成功することを確かめる。
- `A3`: 実装の PR の `pull_request` の `adr` ジョブが成功することを確かめる。
- `.github/workflows/ci.yml` が YAML として読めることを確かめる。

## トレードオフ

- force push の後の push は、`main` との merge-base から先のコミットを、すでに検査したものも含めて検査し直す。検査し直すのは、ブランチの上のコミットだけである。
- `git merge-base --is-ancestor` の終了コードは、「祖先でない」（1）と「コミットが無い」（128）を区別するが、この step は区別しない。どちらでも merge-base から検査すれば足りるためである。
- `origin/main` と `HEAD` に共通の祖先が無いと、`git merge-base` が失敗し、step が失敗する。この場合は検査の範囲を決められないので、ジョブを失敗させてよい。
- [ADR 0006](0006-adr-check-range-for-new-branches.md) と同じく、既定のブランチの名前を変えたときは、この step も変える必要がある。

## 参考文献

- [ADR 0006](0006-adr-check-range-for-new-branches.md)
- tett23/adr-guard v0.13.1 の `src/main.rs` の `check_range`
- tett23/adr-guard の [ADR 0022: 状態と遷移を別々の代数的データ型で表す](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0022-status-transitions.md)
- [git-merge-base(1)](https://git-scm.com/docs/git-merge-base) の `--is-ancestor`
