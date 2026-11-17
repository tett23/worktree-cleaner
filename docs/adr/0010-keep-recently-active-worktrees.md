# ADR 0010: 最近 Git の管理ファイルが更新された worktree を保持する

Status: proposed

## 文脈

[ADR 0001](0001-worktree-cleanup.md) は、clean で統合済みの worktree を、使用中かどうかにかかわらず候補にする。
使用中の worktree として保持するのは、CLI を実行したカレントディレクトリを含むものだけである。

エージェントのセッションは、PR をマージした後も同じ worktree で作業を続けることがある。
その worktree は clean で統合済みのため、別のセッションから CLI で一括削除すると、作業中のセッションの作業ディレクトリが消える。
2026-11-16 の [調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) では、管理ディレクトリ（`<共通ディレクトリ>/worktrees/<名前>`）の `HEAD` か `index` が当日に更新された 24 件を、手作業の判定では作業中として残した。
24 件はどれも、手作業の基準では統合済みで clean だった。
そこで、管理ディレクトリのファイルが最近更新された worktree を、既定で保持する。

この ADR は、既定の判定を変える。
[ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) の不変条件と境界の表のうち、作ったばかりの worktree を候補とする行は、`--min-idle 0` を指定したときの性質になる。
採択したときは、[ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) のステータスを、それぞれ `superseded [{"part":"不変条件","by":["docs/adr/0010-keep-recently-active-worktrees.md"],"kind":"supplement"}]` にする。
[ADR 0002](0002-include-unmerged-worktrees.md) から [ADR 0004](0004-continue-after-removal-failure.md) にも、作ったばかりの worktree を候補とする境界の行があるためである（[ADR 0002](0002-include-unmerged-worktrees.md) の「従来の候補」と「追加候補」、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) の「候補」、[ADR 0004](0004-continue-after-removal-failure.md) の削除の成功の行）。

## 調査した事項

観察は、[調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) に記録したコマンドと出力か、ここに日付を書いたコマンドによる。
コードの位置は main の `7827bb2` のものである。

- 調査の当日、`.claude/worktrees/adr-guard-v0.13.0` という名前の 19 件の管理ディレクトリでは、`HEAD` か `index` の更新時刻が 06:26 から 07:56 の間だった。adr-guard リポジトリの 5 件は、1 から 3 時間前に更新されていた。
- そのうち worktree-cleaner の 1 件では、07:38 に起動した zsh がその中をカレントディレクトリにしていた（`lsof -d cwd`）。
- 手作業で削除した 112 件の管理ディレクトリの `HEAD` と `index` の更新は、どれも 63 時間以上前だった。
- linked worktree の管理ディレクトリ（`<共通ディレクトリ>/worktrees/<名前>`）には、`HEAD`、`index`、`logs/HEAD` がある。2026-11-17 に golem の `adr-guard-v0.13.0` の管理ディレクトリで `ls` と `stat` を実行し、3 つのファイルがあり、更新時刻がそろっていることを確かめた。
- 2026-11-17 に `lsof -d cwd -Fp` の `p` の行を数えると、Claude Code のサンドボックスの中では 3 件、外では 691 件だった。サンドボックスの中では、ほかのプロセスのカレントディレクトリを見られない。
- `src/git.rs:568-574` はロックの検査、`src/git.rs:575` は基準の解決、`src/git.rs:584-591` は detached HEAD の検査、`src/git.rs:592-596` は基準ブランチ自身の検査である。
- 不正なオプションを与えると、CLI は `worktree-cleaner: <理由>` を標準エラーに出し、終了コード 1 で終わる（`src/main.rs` の `main`。2026-11-17 に `worktree-cleaner --bogus` で確かめた）。

## 実装すること

### 最終操作時刻

各 worktree の管理ディレクトリの `HEAD`、`index`、`logs/HEAD` のうち、存在するファイルの更新時刻の最新を、その worktree の最終操作時刻とする。
経過は、検査の時点の現在時刻から最終操作時刻を引いた長さとする。
現在時刻は、初回の判定と削除の直前の再検査のそれぞれで、その検査の時点に読み直す。

### `--min-idle`

`--min-idle <時間数>` を足す。
値は 0 以上の十進の整数で、単位は時間（hour）とする。
既定は 24 とする。

`--min-idle` が 1 以上のとき、次のどれかに当たる worktree を保持する。

| 場合 | 理由 |
| --- | --- |
| 経過が `--min-idle` の時間数に満たない | `worktree had Git activity <N> hours ago (less than --min-idle <M>)` |
| 最終操作時刻が現在時刻より後 | `worktree Git activity time is in the future` |
| 3 つのファイルがどれもない | `cannot read worktree activity: no HEAD, index or logs/HEAD` |
| 存在するファイルのどれかの更新時刻を読めない | `cannot read worktree activity: <パス>: <エラー>` |

`<N>` は、経過を時間数に直して切り捨てた整数とする。
`<M>` は `--min-idle` の値である。

`--min-idle 0` は、この保護を使わない。
そのときは、上の表のどの場合も保持の理由にせず、ファイルの更新時刻も読まない。

`--min-idle` の値が十進の整数でないとき（`-1`、`1.5`、`24h`、空）と、64 ビットの符号なし整数にあふれるときは、一覧も削除もせずに、`worktree-cleaner: invalid value for --min-idle: <値>` を標準エラーに出し、終了コード 1 で終える。

### 判定の順序

この検査は、`evaluate` でロックの所有者の検査（`src/git.rs:568-574`）の後、基準の解決（`src/git.rs:575`）の前に行う。
ロックの有無の検査（`src/git.rs:494-500`）はそれより前にあるため、`--reclaim-agent-locks` なしのロック付きの worktree は、今と同じく `worktree is locked` で保持される。
そのため、最近操作があった worktree は、detached HEAD、基準ブランチ自身、基準を解決できない、未マージ、clean でない、のどれに当たっても、この ADR の理由で保持される。
[ADR 0007](0007-explain-keep-reasons-by-cause.md) の `.git` がない登録の検査は、この検査より前にある。

削除の直前の再検査でも、同じ判定を同じ順序で行う。

### ほかの ADR のテスト

既存のテストと、この ADR の実装より後に実装する ADR のテストのうち、作ったばかりの worktree が候補になることを確かめるものには、`--min-idle 0` を付ける。

## 実装しないこと

- プロセスのカレントディレクトリや、開いているファイルの検査。サンドボックスの中ではほかのプロセスを見られないため、確かな保護にならない。
- 作業ディレクトリの中のファイルの更新時刻の検査。追跡ファイルの変更と未追跡のファイルは、clean の判定がすでに保持の理由にする。
- 1 時間より細かい単位での `--min-idle` の指定。

## 不変条件

- I1: ほかの指定が同じとき、`--min-idle 0` を足した一覧、単一削除、一括削除の候補の集合は、この ADR を入れる前の候補の集合と同じである。
- I2: `--min-idle` が 1 以上のとき、経過が `--min-idle` の時間数に満たない worktree は保持する。
- I3: `--min-idle` が 1 以上のとき、経過が `--min-idle` の時間数以上で、ほかの保護の条件を満たす worktree は候補になる。
- I4: `--min-idle` が 1 以上のとき、最終操作時刻が現在時刻より後の worktree は保持する。
- I5: `--min-idle` が 1 以上のとき、3 つのファイルがどれもないか、存在するファイルのどれかの更新時刻を読めない worktree は保持する。
- I6: `--min-idle` が 1 以上のとき、候補を選んだ後、削除の直前までに最終操作時刻が更新され、経過が `--min-idle` の時間数に満たなくなった worktree は削除しない。
- I7: この判定は、ファイル、登録、参照、ファイルの更新時刻を変更しない。
- I8: `--min-idle` に不正な値を与えた実行は、一覧の行を出さず、どの worktree も削除せず、終了コード 1 で終わる。
- I9: 理由の `<N>` は、経過を時間数に直して切り捨てた整数である。
- I10: `--min-idle` が 1 以上で、ロックの有無の検査を通り、経過が `--min-idle` の時間数に満たない worktree の理由は、detached HEAD、基準ブランチ自身、基準の解決の失敗、未マージ、clean でない、のどれに当たっても `worktree had Git activity ` で始まる。

| 入力（ほかの条件は成立） | 期待 | 不変条件 |
| --- | --- | --- |
| 経過 0 時間、`--min-idle 0` | 候補 | I1 |
| 最終操作時刻が 1 時間後、`--min-idle 0` | 候補 | I1 |
| 3 つのファイルがどれもない、`--min-idle 0` | 候補 | I1 |
| 経過 23 時間 59 分、既定 | 保持、`23 hours ago` | I2, I9 |
| 経過ちょうど 24 時間、既定 | 候補 | I3 |
| 経過 25 時間、既定 | 候補 | I3 |
| 経過 25 時間、`--min-idle 48` | 保持、`25 hours ago` | I2, I9 |
| 最終操作時刻が 1 時間後、既定 | 保持 | I4 |
| `index` と `logs/HEAD` がなく、`HEAD` が 30 時間前、既定 | 候補 | I3 |
| 3 つのファイルがどれもない、既定 | 保持 | I5 |
| `index` の更新時刻を読めない、既定 | 保持 | I5 |
| detached HEAD で、経過 1 時間、既定 | `worktree had Git activity` で保持 | I2, I10 |
| 選択後に `index` の更新時刻を今にした、既定、`--remove-all` | 保持 | I6 |
| `--min-idle -1` | 終了コード 1、行なし | I8 |
| `--min-idle 1.5` | 終了コード 1、行なし | I8 |
| `--min-idle 18446744073709551616` | 終了コード 1、行なし | I8 |

## 受けいれ条件

- A1: エージェントのセッションが作業中の worktree が既定の一覧で保持され、理由に経過の時間数が出ることを手で確かめる。

## テスト設計

テストの名前は `adr0010_i<番号>_<内容>` とする。

- 管理ディレクトリのファイルの更新時刻は、テストの中で設定する（Rust の標準ライブラリの `File::set_modified`）。
- I1 は、[ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) の今のテストと、この ADR より前に実装した ADR のテストに `--min-idle 0` を足して通すことと、境界の表の `--min-idle 0` の行で確かめる。
- I2 から I5 と I9 は、境界の表の行を一つずつ作る。CLI を通すテストでは、現在時刻との差が動くため、境界の行に 1 分以上の余裕を持たせる（23 時間 59 分、24 時間 1 分など）。ちょうど 24 時間の行は、判定の関数に現在時刻を渡す単体テストで確かめる。
- 更新時刻を読めない行は、管理ディレクトリの `index` の代わりに、読めない状態を作れる単体テストで確かめる。
- I6 は、`git::inspect` を直接呼ぶ単体テストで、一度目の呼び出しの後に `index` の更新時刻を今にし、その結果を `expected` に渡す二度目の呼び出しが保持を返すことを確かめる。
- I7 は、一覧の前後で管理ディレクトリのファイルの更新時刻を照合する。
- I8 は、境界の表の不正な値の行を CLI で試す。
- I10 は、経過 1 時間の worktree を、detached HEAD、基準ブランチの checkout、未マージ、未追跡のファイルありの 4 通りで作り、理由を照合する。

## トレードオフ

- 既定で保持が増えるため、作業を終えた直後の worktree は、24 時間たつか `--min-idle 0` を指定するまで削除されない。誤って作業中の worktree を消すと復元できないため、既定を保護の側に置く。
- Git の操作をせずにファイルを読むだけのセッションや、ビルドだけを行うセッションは検出できない。
- 管理ディレクトリの更新時刻は、Git 以外の手段でも変えられる。この判定は、作業中であることの証拠ではなく、目安として扱う。
- 既定の判定が変わるため、既存のテストと、後に実装する ADR のテストに `--min-idle 0` が要る。

## 参考文献

- [ADR 0001](0001-worktree-cleanup.md)
- [ADR 0004](0004-continue-after-removal-failure.md)
- [ADR 0007](0007-explain-keep-reasons-by-cause.md)
- [調査のまとめ: 2026-11-16 tett23 配下の worktree 整理の判断記録](../research/2026-11-16-worktree-cleanup-tett23.md)
- [Git worktree: DETAILS](https://git-scm.com/docs/git-worktree#_details)
- [std::fs::File::set_modified](https://doc.rust-lang.org/std/fs/struct.File.html#method.set_modified)
