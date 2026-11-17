# ADR 0009: 明示指定で、origin のリモート追跡ブランチを基準にする

Status: proposed

## 文脈

[ADR 0001](0001-worktree-cleanup.md) は、各リポジトリのローカルブランチを基準にし、HEAD がその基準の先端と同じか祖先である worktree を候補にする。
既定の基準は、ローカルに記録された `origin/HEAD` が指すブランチと同じ名前のローカルブランチである。

この基準は、利用者がローカルの基準ブランチを最新に保つことを前提にしている。
しかし、PR を GitHub でマージし、ローカルでは fetch だけを行う使い方では、ローカルの基準ブランチが `origin` の同じブランチより遅れる。
2026-11-16 の [調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) では、この遅れのために、統合済みの worktree の大半を CLI で判定できなかった。
そこで、利用者が明示したときに、`refs/remotes/origin/<基準>` を基準にできるようにする。

この ADR は、`--remote-base` を指定したときに限り、[ADR 0001](0001-worktree-cleanup.md) の「実装すること」の基準ブランチの決め方と、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) の「GitHub による統合の確認」のマージコミットを照らす先を変える。
指定しないときの決定は変えない。
採択したときは、[ADR 0001](0001-worktree-cleanup.md) のステータスを `superseded [{"part":"実装すること","by":["docs/adr/0009-remote-tracking-base.md"],"kind":"supplement"}]` に、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のステータスを `superseded [{"part":"アルゴリズム","by":["docs/adr/0009-remote-tracking-base.md"],"kind":"supplement"}]` にする。

## 調査した事項

次の観察は、[調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) に記録したコマンドと出力による。
コードの位置は main の `7827bb2` のものである。

- ローカルの main は、`origin/main` より sofel で 1107 コミット、peg-hs で 413、golem で 241 遅れていた（`git rev-list --count main..origin/main`）。これらのリポジトリの `FETCH_HEAD` の更新時刻は、どれも調査の当日だった（`stat`）。
- 手作業で削除した 88 件のうち 82 件は、HEAD が `origin/main` に含まれるが、ローカルの main には含まれなかった（`git merge-base --is-ancestor`）。
- 既定の一覧では、そのうちブランチ上の 48 件が `branch is not merged into base` で保持された。
- `--github-merged` の一覧では、別の 13 件が `check PR merge commit ancestry: object not found - no match for id (<コミット ID>)` で保持された。この理由は、マージコミットのオブジェクトがローカルのリポジトリにないことを示す。この 13 件の HEAD は、調査の時点でローカルの main にも `origin/main` にも含まれていなかった（`git merge-base --is-ancestor`）。そのため、この 13 件はリモート追跡ブランチを基準にしても判定できず、fetch が要る。
- `src/git.rs:304-326` の `base_ref` は、基準の名前に `refs/heads/` を付けた参照を引く。
- `src/git.rs:592-596` は、HEAD が基準の参照と同じ名前なら、その worktree を `worktree uses base branch` で保持する。
- `src/git.rs:432-436` は、GitHub の照会に渡す基準の名前を、基準の参照の名前から `refs/heads/` を除いて求める。

## 実装すること

`--remote-base` を足す。
指定したときは、各リポジトリで基準の名前を今と同じ方法で決め、到達の判定に `refs/remotes/origin/<名前>` を使う。
基準の名前は、`--base` を指定すればその名前、指定しなければローカルに記録された `origin/HEAD` の参照先から求めた名前である。

例えば、`worktree-cleaner --remote-base` は、ローカルの main を進めずに、`origin/main` に取り込まれた worktree を候補として表示する。
候補の行の理由は、`clean and integrated into origin/<名前>` とする。

`--remote-base` を指定したときの、ほかの判定は次のとおりとする。

- HEAD が `refs/heads/<名前>` を指す worktree は、今と同じく `worktree uses base branch` で保持する。ローカルブランチ `<名前>` がなくても、この比較は名前で行う。
- `refs/remotes/origin/<名前>` がないリポジトリは、理由を示してそのリポジトリの worktree を保持し、ほかのリポジトリの処理を続ける。
- `--github-merged` と組み合わせたときは、GitHub に渡す基準の名前を `<名前>` とし、PR のマージコミットが `refs/remotes/origin/<名前>` の先端と同じか祖先であることを確かめる。
- `--include-unmerged` と組み合わせたときは、`refs/remotes/origin/<名前>` から到達できない HEAD を未マージとして扱う。
- 削除の直前の再検査でも、同じ参照を使う。

## 実装しないこと

- fetch などのリモートとの通信。`refs/remotes/origin/<名前>` は、利用者が最後に fetch したときの値を使う。
- ローカルの基準ブランチの更新。
- `origin` 以外のリモートの指定。
- 既定の基準の変更。`--remote-base` を指定しない実行は、この ADR の前と同じ判定をする。

## 不変条件

- I1: `--remote-base` を指定しない一覧、単一削除、一括削除の候補の集合は、この ADR を入れる前と同じである。
- I2: `--remote-base` を指定し、`--github-merged` と `--include-unmerged` を指定しないとき、ほかの保護の条件を満たすブランチ上の worktree は、HEAD が `refs/remotes/origin/<名前>` の先端と同じか祖先なら、ローカルの基準ブランチから到達できなくても候補になり、そうでなければ保持する。
- I3: `--remote-base` を指定したとき、HEAD が `refs/heads/<名前>` を指す worktree は保持する。
- I4: `--remote-base` を指定したとき、`refs/remotes/origin/<名前>` がないリポジトリの worktree はすべて保持し、ほかのリポジトリの判定は変わらない。
- I5: `--remote-base` を指定しても、`--github-merged` がなければ gh を起動しない。
- I6: `--remote-base` と `--github-merged` を指定したとき、PR のマージコミットが `refs/remotes/origin/<名前>` から到達できなければ、その PR を統合の証拠にしない。
- I7: `--remote-base` の一覧は、ファイル、登録、参照を変更しない。
- I8: `--remote-base` と `--github-merged` を指定したとき、ブランチ上の HEAD が `refs/remotes/origin/<名前>` に含まれなくても、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) の残りの条件を満たす PR のマージコミットが `refs/remotes/origin/<名前>` の先端と同じか祖先なら、ほかの保護の条件を満たす worktree は候補になる。

| 入力（ほかの条件は成立） | 期待 | 不変条件 |
| --- | --- | --- |
| HEAD がローカルの main に含まれる、指定なし | 候補 | I1 |
| HEAD が `origin/main` にだけ含まれる、指定なし | 保持 | I1 |
| HEAD が `origin/main` にだけ含まれる、指定あり | 候補 | I2 |
| HEAD が `origin/main` の先端と同じ、指定あり | 候補 | I2 |
| HEAD が `origin/main` より 1 コミット進んでいる、指定あり | 保持 | I2 |
| HEAD が `refs/heads/main` を指す、指定あり | 保持 | I3 |
| `refs/remotes/origin/main` がない、指定あり | そのリポジトリだけ保持 | I4 |
| マージコミットがローカルの main から到達できず `origin/main` から到達できる、`--github-merged` と指定あり | 候補 | I8 |
| HEAD は `origin/main` に含まれず、マージコミットも `origin/main` から到達できない、`--github-merged` と指定あり | 保持 | I2, I6 |

## 受けいれ条件

- A1: ローカルの main が `origin/main` より遅れたリポジトリで、`origin/main` に取り込まれた worktree が `--remote-base` の一覧で候補になり、指定しない一覧では保持されることを手で確かめる。

## テスト設計

テストの名前は `adr0009_i<番号>_<内容>` とする。

- 一時リポジトリに bare のリポジトリを `origin` として足し、`origin/main` だけを進めて fetch した状態を作る。ローカルの main は進めない。
- I1 と I2 は、境界の表の指定なしと指定ありの行を、一覧、単一削除、一括削除で照合する。
- I3 は、ローカルの main を checkout した linked worktree を作る。
- I4 は、`origin` の参照を持たないリポジトリと持つリポジトリを同じ親ディレクトリに置き、親ディレクトリから実行する。
- I5、I6、I8 は、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のテストと同じく、呼び出しを記録する偽の gh を使う。I8 は squash merge のように、HEAD を含まないマージコミットを `origin/main` に置いて作る。
- I7 は、一覧の前後で参照、登録、作業ファイルを照合する。

## トレードオフ

- `refs/remotes/origin/<名前>` は最後の fetch の時点の値であり、その後に GitHub で force push や revert があっても反映されない。ローカルの基準ブランチを使う場合と同じく、利用者が受け入れた状態を基準にしたものとして扱う。
- 既定の基準をリモート追跡ブランチに変えれば、指定なしで多くの worktree を判定できる。しかし、ローカルの基準を整理の条件にしている利用者の判定が変わるため、既定は変えない。

## 参考文献

- [ADR 0001](0001-worktree-cleanup.md)
- [ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md)
- [調査のまとめ: 2026-11-16 tett23 配下の worktree 整理の判断記録](../research/2026-11-16-worktree-cleanup-tett23.md)
- [Git: remote-tracking branches](https://git-scm.com/book/en/v2/Git-Branching-Remote-Branches)
