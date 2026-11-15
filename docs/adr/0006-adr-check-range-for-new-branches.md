# ADR 0006: 新しいブランチの push では、main との merge-base から ADR を検査する

Status: accepted-pending

## 文脈

CI の `adr` ジョブは、`push` と `pull_request` の両方で動く（`.github/workflows/ci.yml` の `on: [push, pull_request]`）。
push のときは、検査の範囲の始まり（`base`）に `github.event.before` を渡している。

新しいブランチを初めて push すると、`github.event.before` は `0000000000000000000000000000000000000000` になる。
adr-guard の `check-range` は、`base` がコミットとして存在しなければ、`head` から辿れるすべてのコミットを検査する。

[ADR 0005](0005-update-adr-guard-to-v0.13.1.md) で、CI の adr-guard を v0.13.1 に上げた。
v0.13.1 の `check-range` は、ADR を旧形式のステータス行（`ステータス: 採択` など）で追加したコミットを拒否する（tett23/adr-guard の [ADR 0022](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0022-status-transitions.md)）。
このリポジトリの履歴には、[ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) を旧形式で追加したコミットがある。
そのため、v0.13.1 に上げた後は、新しいブランチを push するたびに、push の `adr` ジョブが必ず失敗する。

## 調査した事項

[ADR 0005](0005-update-adr-guard-to-v0.13.1.md) の実装の PR（tett23/worktree-cleaner#6）で、次を確かめた。

- `pull_request` の `adr` ジョブは成功した。
- 実装のブランチを初めて push したときの `adr` ジョブ（run 43765923699）は失敗した。ログの `BASE_SHA` は `0000000000000000000000000000000000000000` で、`check-range` は 6e0f781、2f24639、60e002b、6db675d、9cac767、8858ce5、b8e0a4f の各コミットについて ``新形式のステータス行は `Status: ` で始めてください`` を出し、終了コード1で終わった。どれも [ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) を旧形式で追加・変更したコミットである。

tett23/NablaSKK は、同じ問題を CI の step で解いている（tett23/NablaSKK の `origin/main` の `.github/workflows/ci.yml` の `adr` ジョブ）。
その step は、`pull_request` なら PR の base、push で `before` が `0000…` でなければ `before`、それ以外なら `git merge-base origin/main "$HEAD"` を `base` にして、action に渡す。

## 実装すること

- `.github/workflows/ci.yml` の `adr` ジョブに、検査の範囲を決める step（`id: range`）を、adr-guard の action の前に足す。
  - `pull_request` のときは、`github.event.pull_request.base.sha` を `base` にする。
  - push で、`github.event.before` が空でも `0000000000000000000000000000000000000000` でもなければ、それを `base` にする。
  - それ以外（新しいブランチやタグの最初の push）は、`git merge-base origin/main "$HEAD"` を `base` にする。
  - `head` は、`pull_request` なら `github.event.pull_request.head.sha`、それ以外は `github.sha` とする。
- adr-guard の action の `base` と `head` に、この step の出力を渡す。ほかの入力は変えない。
- step の前に、この ADR を指す注釈を書く。

## 実装しないこと

- 履歴の ADR を追加したコミットの書き換え（rebase など）。
- push の `adr` ジョブをやめること。
- `changes` ジョブの範囲の決め方の変更。`changes` ジョブは、すでに merge-base を使っている。

## 不変条件

なし。
CI の設定の値だけを決める ADR で、どの入力でも成り立つ性質を書けないためである。

## テスト設計

自動のテストは書かない。
実装の後に、次を確かめる。

- 実装のブランチを初めて push したときの `adr` ジョブが成功する。
- 実装の PR の `pull_request` の `adr` ジョブが成功する。
- `.github/workflows/ci.yml` が YAML として読める。

## トレードオフ

- 新しいブランチの最初の push は、`main` との merge-base から先のコミットだけを検査する。`main` に入る前のコミットは、PR の CI と `main` への push でも検査されるため、検査の漏れは生じない。
- `origin/main` を使うので、既定のブランチの名前を変えたときは、この step も変える必要がある。

## 参考文献

- [ADR 0005](0005-update-adr-guard-to-v0.13.1.md)
- tett23/adr-guard の [ADR 0022: 状態と遷移を別々の代数的データ型で表す](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0022-status-transitions.md)
- tett23/NablaSKK の `.github/workflows/ci.yml` の `adr` ジョブ
