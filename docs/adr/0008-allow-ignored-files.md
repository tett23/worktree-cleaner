# ADR 0008: 明示指定で、無視ファイルだけがある worktree を候補にする

Status: proposed

## 文脈

[ADR 0001](0001-worktree-cleanup.md) の不変条件 7 は、無視設定にかかわらず未追跡のファイルがあれば worktree を保持する。
ビルド成果物のような無視ファイルも、この条件で保持される。
そのトレードオフの節は、再生成できるファイルを利用者が先に片付けることを前提にしている。

この ADR は、`--allow-ignored` を指定したときに限り、[ADR 0001](0001-worktree-cleanup.md) の不変条件 7 の例外を作る。
指定しないときの [ADR 0001](0001-worktree-cleanup.md) の不変条件 7 は変えない。
採択したときは、[ADR 0001](0001-worktree-cleanup.md) のステータスを `superseded [{"part":"不変条件","by":["docs/adr/0008-allow-ignored-files.md"],"kind":"supplement"}]` にする。
[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) の境界の表の「終了を確認したが無視ファイルが一つある」の行も、指定したときに限り変わる。
そのため、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のステータスも `superseded [{"part":"不変条件","by":["docs/adr/0008-allow-ignored-files.md"],"kind":"supplement"}]` にする。

しかし、エージェントが作る worktree の多くには、ビルドやテストの成果物が残る。
2026-11-16 の [調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) では、手作業で削除してよいと判定したブランチ上の worktree 53 件のすべてに無視ファイルがあった。
GitHub の照会を使った一覧では、53 件すべてが無視ファイルを理由に保持された。
利用者が成果物を一件ずつ消してから CLI を使うのは手間が大きい。
そこで、無視ファイルを削除してよいと利用者が明示したときに限り、無視ファイルを保持の理由にしないようにする。

## 調査した事項

次の観察は、[調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) に記録したコマンドと出力による。
コードの位置は main の `7827bb2` のものである。

- 手作業で削除した 112 件のうち、`.git` が消えていたものとロック付きのものを除く 88 件では、ブランチ上の 53 件すべてに無視ファイルがあった。`--github-merged --reclaim-agent-locks` の一覧は、この 53 件を無視ファイルを理由に保持した。
- ロック付きで削除した 5 件にも無視ファイルがあり、追跡ファイルの変更と無視されない未追跡のファイルはなかった。
- 無視ファイルの中身は、`git status --porcelain --ignored -unormal` で見ると、`target/`、`dist-newstyle/`、`node_modules/`、`.cache/`、`cabal.project.local`、`.DS_Store`、`*.hi`、`experiments/` 配下の `*.log` だった。
- 無視ファイルしかない worktree は、`git worktree remove`（`--force` なし）で削除できた。
- `src/git.rs:335-342` の `ensure_clean` は、`git_status_foreach` のコールバックが一度でも呼ばれると保持する。`git_status_foreach` の既定のオプションは `GIT_STATUS_OPT_INCLUDE_IGNORED` を含み（libgit2 v1.9.7 の `include/git2/status.h:210-213`）、無視ファイルでもコールバックを呼ぶため、無視ファイルだけでも保持される。tests/cli.rs:122-140 の `invariant_06_07_index_and_untracked_files_are_preserved` も、無視ファイルだけの worktree を保持することを確かめている。
- `src/git.rs:364-397` は、作業ディレクトリのすべてのファイルを辿り、インデックスにないファイルを `untracked file: <パス>` として保持する。この走査は、無視されたディレクトリの中の `.git`（入れ子のリポジトリ）と、通常のファイルとシンボリックリンク以外のファイルも保持の理由にする。

## 実装すること

`--allow-ignored` を足す。
指定したときは、無視ファイルを保持の理由にしない。
ほかの保護の条件は、指定しないときと同じとする。

例えば、`worktree-cleaner --allow-ignored` は、無視ファイルだけがある統合済みの worktree を候補として表示する。
`worktree-cleaner --allow-ignored --remove-all` は、それらの worktree を無視ファイルごと削除する。

候補の行の理由には、削除する無視ファイルの数を `; deletes N ignored files` の形で添える。
数は、走査で見つけた、インデックスにないファイルとシンボリックリンクの数とする。
作業ディレクトリのルートの `.git` は数えない。
例えば、`target/` に 2 つのファイルだけがある worktree の理由は `clean and integrated; deletes 2 ignored files` となる。

他の指定との組み合わせは次のとおりとする。

- `--include-unmerged`、`--github-merged` と組み合わせられる。
- `--reclaim-agent-locks` と組み合わせると、無視ファイルだけがあるロック付きの worktree も、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) の残りの条件を満たせば候補にする。

## アルゴリズム

`ensure_clean` を次のように変える。

1. `git_status_foreach` のコールバックで、状態のフラグが `GIT_STATUS_IGNORED` だけでない項目があれば保持する。`--allow-ignored` を指定しないときは、今と同じく項目が一つでもあれば保持する。この段では数えない。
2. インデックスの検査（サブモジュール、特別なエントリ、未解決の衝突）は今と同じとする。
3. ファイルの走査は、今と同じくすべてのディレクトリを辿る。無視されたディレクトリも辿る。
4. 走査で、インデックスにないファイルかシンボリックリンクを見つけたら、libgit2 の `git_ignore_path_is_ignored` でそのパスが無視されるかを調べる。無視されるなら数に 1 を足して走査を続ける。無視されなければ、今と同じ `untracked file: <パス>` で保持する。判定に失敗したら、`check ignore rules: <libgit2 のエラー>` で保持する。
5. 入れ子のリポジトリと特別なファイルは、無視されたディレクトリの中にあっても保持の理由とする。

削除の直前の再検査でも、同じ手順を使う。
削除は今と同じく `git_worktree_prune` で作業ディレクトリごと消す。

## 実装しないこと

- 無視ファイルだけを消して worktree を残すこと。
- 無視ファイルの種類やパスによる絞り込み（`target/` だけを許すなど）。
- 既定の判定の変更。`--allow-ignored` を指定しない実行は、この ADR の前と同じ判定をする。
- 削除した無視ファイルの復元。

## 不変条件

- I1: `--allow-ignored` を指定しない一覧、単一削除、一括削除の候補の集合は、この ADR を入れる前と同じである。
- I2: `--allow-ignored` を指定したとき、インデックスにないファイルがすべて無視され、ほかの保護の条件を満たす worktree は候補になる。
- I3: `--allow-ignored` を指定しても、無視されない未追跡のファイルが一つでもあれば保持する。
- I4: `--allow-ignored` を指定しても、追跡ファイルかインデックスに変更があれば保持する。
- I5: `--allow-ignored` を指定しても、無視されたディレクトリの中に `.git` があれば保持する。
- I6: `--allow-ignored` を指定しても、無視されたディレクトリの中に通常のファイルとシンボリックリンク以外のファイルがあれば保持する。
- I7: `--allow-ignored` で選んだ候補の理由に添える数は、作業ディレクトリのルートの `.git` を除く、インデックスにないファイルとシンボリックリンクの数と一致する。
- I8: `--allow-ignored` で選んだ候補を削除した後も、ブランチ参照は削除の直前と同じコミットを指す。
- I9: 候補を選んだ後、削除の直前までに無視されない未追跡のファイルが増えた worktree は削除しない。

| 入力（ほかの条件は成立） | 期待 | 不変条件 |
| --- | --- | --- |
| 無視ファイルなし、指定なし | 候補 | I1 |
| 無視ファイルが一つ、指定なし | 保持 | I1 |
| 無視ファイルが一つ、指定あり | 候補、`deletes 1 ignored files` | I2, I7 |
| 無視ファイルが一つと未追跡のファイルが一つ、指定あり | 保持 | I3 |
| 無視ファイルが一つとステージ済みの変更が一つ、指定あり | 保持 | I4 |
| 無視されたディレクトリに `.git` がある、指定あり | 保持 | I5 |
| 無視されたディレクトリに名前付きパイプがある、指定あり | 保持 | I6 |
| 無視ファイルが一つ、指定あり、`--remove-all` | 削除され、ブランチは残る | I2, I8 |
| 選択後に未追跡のファイルを一つ足した、指定あり、`--remove-all` | 保持 | I9 |

## 受けいれ条件

- A1: ビルド成果物だけが残った統合済みの worktree を手元で一つ用意し、`--allow-ignored --remove PATH` で削除できることと、ブランチが残ることを手で確かめる。

## テスト設計

テストの名前は `adr0008_i<番号>_<内容>` とする。

- I1 は、[ADR 0001](0001-worktree-cleanup.md) の不変条件 7 の今のテストが変わらずに通ることと、境界の表の指定なしの 2 行で確かめる。
- I2 から I7 は、一時リポジトリに `.gitignore` を置き、境界の表の行を一つずつ作って一覧を照合する。I5 は無視されたディレクトリの中で `git init` し、I6 は `mkfifo` で名前付きパイプを作る。
- I8 は、無視ファイルだけがある統合済みの worktree を `--allow-ignored --remove-all` で削除し、ブランチのコミット ID を照合する。
- I9 は、`git::inspect` を直接呼ぶ単体テストで確かめる。一度目の呼び出しで候補の検査結果を得た後に未追跡のファイルを足し、その結果を `expected` に渡す二度目の呼び出しが保持を返すことを確かめる。一覧と `--remove-all` の二つの実行の間にファイルを足す形では、`--remove-all` の選択の段で保持されるため、削除の直前の再検査を確かめられない。

## トレードオフ

- 無視ファイルには、利用者が手で作った設定（`cabal.project.local` など）も含まれうる。`--allow-ignored` はそれらも削除するため、利用者の明示を求め、既定では保持する。
- 削除の前に無視されたディレクトリの中も辿るため、`node_modules/` のような大きなディレクトリがあると判定が遅くなる。辿らなければ、入れ子のリポジトリを見落とす。
- 無視ファイルの種類による絞り込みはしないため、利用者は一覧の理由の数を見て判断する必要がある。

## 参考文献

- [ADR 0001](0001-worktree-cleanup.md)
- [ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md)
- [調査のまとめ: 2026-11-16 tett23 配下の worktree 整理の判断記録](../research/2026-11-16-worktree-cleanup-tett23.md)
- [libgit2 ignore API](https://libgit2.org/docs/reference/main/ignore/index.html)
- [Git worktree](https://git-scm.com/docs/git-worktree)
