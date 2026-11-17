# ADR 0011: 明示指定で、基準に含まれる detached HEAD の worktree を候補にする

Status: proposed

## 文脈

[ADR 0001](0001-worktree-cleanup.md) の不変条件 5 は、HEAD がローカルブランチを指す場合だけを候補にする。
[ADR 0002](0002-include-unmerged-worktrees.md) は、detached HEAD の worktree を対象にしないと決めた。
その調査では、detached HEAD の 10 件はどれも HEAD が基準ブランチの祖先か同じコミットだった。

detached HEAD の worktree は、エージェントが PR の検証や main との統合の確認に使い、作業の後にそのまま残すことが多い。
2026-11-16 の [調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) では、手作業で削除した 112 件のうち、ロックがなく `.git` も残っていた 88 件では、35 件が detached HEAD で、どれも HEAD がローカルの main か `origin/main` に含まれていた。
そのうち 34 件は `origin/main` にだけ含まれていたため、この ADR だけでなく [ADR 0009](0009-remote-tracking-base.md) の `--remote-base` も指定しないと候補にならない。
HEAD のコミットが基準に含まれていれば、worktree を消してもそのコミットは基準から到達できる。
そこで、利用者が明示したときに、基準に含まれる detached HEAD の worktree を候補にする。

この ADR は、`--include-detached` を指定したときに限り、[ADR 0001](0001-worktree-cleanup.md) の不変条件 5 の例外を作り、[ADR 0002](0002-include-unmerged-worktrees.md) が範囲の外とした detached HEAD の対象化を行う。
指定しないときの決定は変えない。
採択したときは、[ADR 0001](0001-worktree-cleanup.md) のステータスを `superseded [{"part":"不変条件","by":["docs/adr/0011-include-detached-integrated-worktrees.md"],"kind":"supplement"}]` に、[ADR 0002](0002-include-unmerged-worktrees.md) のステータスを `superseded [{"part":"実装しないこと","by":["docs/adr/0011-include-detached-integrated-worktrees.md"],"kind":"supplement"}]` にする。

## 調査した事項

次の観察は、[調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) に記録したコマンドと出力による。
コードの位置は main の `7827bb2` のものである。

- 手作業で削除した 88 件のうち 35 件は detached HEAD だった。35 件の HEAD は、1 件がローカルの main に、34 件が `origin/main` に含まれていた（`git merge-base --is-ancestor`）。
- 35 件のうち 20 件には無視ファイルがあり、15 件には無視ファイルもなかった。
- CLI は 35 件すべてを、既定の一覧でも `--github-merged --reclaim-agent-locks` の一覧でも `worktree HEAD is detached` で保持した。
- `src/git.rs:584-591` は、HEAD の参照の名前が `refs/heads/` で始まらなければ `worktree HEAD is detached` を返す。この検査は、到達の判定と clean の判定より前に行う。
- 2026-11-17 に一時リポジトリで、detached HEAD の linked worktree `w` の中で `git update-ref refs/worktree/x HEAD` を実行した（git 2.56.0、参照の保存形式は既定のファイル）。参照は管理ディレクトリの `refs/worktree/x` に作られ、主 worktree での `git for-each-ref` には出なかった。
- 保持した 124 件には、HEAD が基準に含まれない detached HEAD の worktree があった。たとえば sofel の 13 件は、PR がなく、HEAD がどのブランチとリモート追跡ブランチからも到達できなかった（`git for-each-ref --contains <HEAD> refs/heads refs/remotes`）。

## 実装すること

`--include-detached` を足す。
指定したときは、detached HEAD の worktree で、HEAD のコミットが基準の先端と同じか祖先であり、ほかの保護の条件を満たすものを候補にする。
基準は、[ADR 0001](0001-worktree-cleanup.md) の基準、または [ADR 0009](0009-remote-tracking-base.md) の `--remote-base` を指定したときの基準とする。

例えば、`worktree-cleaner --include-detached` は、main に取り込まれた検証用の detached HEAD の worktree を候補として表示する。
候補の行の理由は、`clean detached HEAD <コミット ID> is integrated into <基準の参照の短い名前>` とする。

ほかの指定との関係は次のとおりとする。

- `--include-unmerged` を指定しても、基準に含まれない detached HEAD の worktree は候補にしない。消した後にコミットを保持する参照がないためである。
- `--github-merged` を指定しても、detached HEAD の worktree については GitHub に問い合わせない。
- `--reclaim-agent-locks` を指定しても、ロック付きの detached HEAD の worktree は候補にしない。[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) の、ローカルブランチに接続した HEAD を求める条件を変えない。
- `--allow-ignored`（[ADR 0008](0008-allow-ignored-files.md)）と組み合わせられる。

## アルゴリズム

HEAD の参照の名前が `refs/heads/` で始まらないとき、`--include-detached` を指定していなければ、今と同じく保持する。
指定していれば、HEAD の参照が `HEAD` そのものであり、コミットを直接指すことを確かめる。
確かめられなければ、`worktree HEAD is neither a branch nor a commit` で保持する。
続いて、管理ディレクトリの `refs/` の下（`refs/worktree/`、`refs/bisect/` など、worktree 固有の参照）にファイルが一つでもあれば、`worktree has worktree-specific refs` で保持する。
worktree を消すとこれらの参照も消え、参照だけが指していたコミットが辿れなくなるためである。
次に、基準の先端と HEAD のコミットが同じか、`git_graph_descendant_of` で基準の先端が HEAD のコミットの子孫であることを確かめる。
どちらでもなければ保持する。
その後、今と同じ clean の判定と保護の条件を適用する。

[ADR 0010](0010-keep-recently-active-worktrees.md) の最終操作時刻の検査は、detached HEAD の検査より前にある。

削除の操作では、初回の判定で、detached HEAD の候補であることと、HEAD のコミット ID を記録する。
削除の直前の再判定で、HEAD がまだ detached で、コミット ID が記録と同じで、基準に含まれることを確かめた場合だけ削除する。

## 実装しないこと

- 基準に含まれない detached HEAD の worktree の削除。参照を作ってから消すことも行わない。
- 既定の判定の変更。`--include-detached` を指定しない実行は、この ADR の前と同じ判定をする。
- worktree 固有の HEAD の reflog の保存。
- 参照の保存形式が reftable のリポジトリでの worktree 固有の参照の検出。reftable のときに参照がどこに置かれ、libgit2 1.9.7 がそのリポジトリを開けるかは未確認である。

## 不変条件

- I1: `--include-detached` を指定しない一覧、単一削除、一括削除の候補の集合は、この ADR を入れる前と同じである。
- I2: `--include-detached` を指定したとき、HEAD のコミットが基準の先端と同じか祖先で、ほかの保護の条件を満たす detached HEAD の worktree は候補になる。
- I3: HEAD のコミットが基準の先端と同じでも祖先でもない detached HEAD の worktree は、どのオプションの組み合わせでも保持する。
- I4: ロック付きの detached HEAD の worktree は、どのオプションの組み合わせでも保持する。
- I5: `--include-detached` で選んだ候補を削除した後も、削除の直前の HEAD のコミットは基準の参照から到達できる。
- I6: 候補を選んだ後、削除の直前までに HEAD が別のコミットに移るか、ブランチを指すようになった worktree は削除しない。
- I7: `--include-detached` を指定しても、`--github-merged` の実行は detached HEAD の worktree について gh を起動しない。
- I8: 管理ディレクトリの `refs/` の下にファイルがある detached HEAD の worktree は、どのオプションの組み合わせでも保持する。

| 入力（ほかの条件は成立） | 期待 | 不変条件 |
| --- | --- | --- |
| detached HEAD が基準の先端と同じ、指定なし | 保持 | I1 |
| detached HEAD が基準の先端と同じ、指定あり | 候補 | I2 |
| detached HEAD が基準の先端の祖先、指定あり | 候補 | I2 |
| detached HEAD が基準の先端の祖先、指定ありと `--remove-all` | 削除され、そのコミットは基準から到達できる | I2, I5 |
| detached HEAD が基準より 1 コミット進んでいる、指定あり | 保持 | I3 |
| detached HEAD が基準より 1 コミット進んでいる、指定ありと `--include-unmerged` | 保持 | I3 |
| detached HEAD が基準の祖先でロック付き、指定ありと `--reclaim-agent-locks` | 保持 | I4 |
| detached HEAD が基準の祖先、指定ありと `--github-merged` | 候補、gh は起動しない | I2, I7 |
| detached HEAD が基準の祖先で、`refs/worktree/x` がある、指定あり | 保持 | I8 |
| 選択後に HEAD を別のコミットに移した、指定ありと `--remove-all` | 保持 | I6 |
| 選択後に HEAD でブランチを checkout した、指定ありと `--remove-all` | 保持 | I6 |

## 受けいれ条件

- A1: main に取り込まれたコミットを checkout した detached HEAD の worktree を手元で一つ用意し、`--include-detached --remove PATH` で削除できることと、そのコミットが main から到達できることを手で確かめる。

## テスト設計

テストの名前は `adr0011_i<番号>_<内容>` とする。

- I1 は、[ADR 0001](0001-worktree-cleanup.md) の不変条件 5 の detached HEAD のテストが変わらずに通ることと、境界の表の指定なしの行で確かめる。
- I2、I3、I5 は、一時リポジトリで `git worktree add --detach` を使い、基準の先端、祖先、基準より進んだコミットの 3 つを作る。削除の後に `git merge-base --is-ancestor` で I5 を確かめる。
- I4 は、`git worktree lock` で理由付きのロックを設定し、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のテスト（tests/reclaim.rs）と同じく、範囲の外の PID をロックの理由に書いて所有者の終了を成立させる。
- I8 は、worktree の中で `git update-ref refs/worktree/x HEAD` を実行して作る。
- I6 は、`git::inspect` を直接呼ぶ単体テストで、一度目の呼び出しの後に `git checkout --detach <別のコミット>` か `git checkout <ブランチ>` を行い、その結果を `expected` に渡す二度目の呼び出しが保持を返すことを確かめる。
- I7 は、呼び出しを記録する偽の gh を使う。

## トレードオフ

- detached HEAD の worktree を消すと、作業ディレクトリのパスと、管理ディレクトリの worktree 固有の状態（HEAD の reflog、`ORIG_HEAD`、`FETCH_HEAD` など）が消える。HEAD のコミットは基準から到達できるが、どのコミットを検証していたかの記録は残らない。
- 既定で対象にすれば指定は要らないが、detached HEAD を作業の目印として使う利用者の判定が変わるため、明示の指定とする。

## 参考文献

- [ADR 0001](0001-worktree-cleanup.md)
- [ADR 0002](0002-include-unmerged-worktrees.md)
- [ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md)
- [ADR 0008](0008-allow-ignored-files.md)
- [ADR 0009](0009-remote-tracking-base.md)
- [調査のまとめ: 2026-11-16 tett23 配下の worktree 整理の判断記録](../research/2026-11-16-worktree-cleanup-tett23.md)
- [Git worktree](https://git-scm.com/docs/git-worktree)
