# ADR 0007: 保持の理由を原因ごとに示す

Status: accepted

## 文脈

2026-11-16 に、`~/repositories/src/github.com/tett23` 配下の linked worktree 236 件をこの CLI で調べた。
その記録は [調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) にある。
CLI は 236 件すべてを保持と判定し、そのうち 112 件は手作業の調査で削除してよいと分かった。

手作業の調査が要ったのは、保持の理由の文言から原因を区別できなかったためである。
たとえば、未コミットの変更がある worktree と、ビルド成果物などの無視ファイルしかない worktree は、同じ文言で保持された。
この ADR は判定を変えずに、保持の理由に原因を区別できる情報を足す。
判定を変える改善は、[ADR 0008](0008-allow-ignored-files.md)、[ADR 0009](0009-remote-tracking-base.md)、[ADR 0010](0010-keep-recently-active-worktrees.md)、[ADR 0011](0011-include-detached-integrated-worktrees.md) で扱う。

## 調査した事項

観察は [調査のまとめ](../research/2026-11-16-worktree-cleanup-tett23.md) に記録したコマンドと出力による。
コードの位置は main の `7827bb2` のものである。
libgit2 の振る舞いは、手元に入っている libgit2 1.9.7 と同じ版のソース（`github.com/libgit2/libgit2` のタグ `v1.9.7`）を読んで確かめた。

### 変更の種類を区別しない

`src/git.rs:335-342` の `ensure_clean` は、`git_status_foreach` のコールバックが一度でも呼ばれると、`working tree or index has changes, including untracked or ignored files` を返す。
コールバックに渡される状態のフラグは使っていない。
調査の `--github-merged --reclaim-agent-locks` の一覧では、119 件がこの文言で保持された。
そのうち 93 件は無視ファイルしかなく、26 件には追跡ファイルの変更か無視されない未追跡のファイルがあった。

`git_status_foreach` は、オプションを渡さない `git_status_foreach_ext` である（`src/libgit2/status.c:454-457`）。
オプションがないときのフラグは `GIT_STATUS_OPT_DEFAULTS` で（`src/libgit2/status.c:274`）、その値は `GIT_STATUS_OPT_INCLUDE_IGNORED`、`GIT_STATUS_OPT_INCLUDE_UNTRACKED`、`GIT_STATUS_OPT_RECURSE_UNTRACKED_DIRS` である（`include/git2/status.h:210-213`）。
したがって、未追跡のディレクトリの中のファイルは一つずつ、無視されたディレクトリは一つの項目としてコールバックに渡る。

### `.git` ファイルがない登録の理由が OS のエラーだけになる

`src/git.rs:507-509` は、作業ディレクトリの `.git` の `symlink_metadata` が失敗すると、そのエラーの文字列をそのまま理由にする。
調査では、/tmp の 23 件で `.git` ファイルが消えており、理由は `No such file or directory (os error 2)` だった。
同じ 23 件を `git worktree list` は `prunable` と表示した。
23 件のうち 4 件は、HEAD のコミットがどのブランチとリモート追跡ブランチからも到達できなかった。

作業ディレクトリそのものがない登録では、`src/git.rs:490-493` の `git_worktree_validate` が先に失敗する。
libgit2 はこのとき `worktree directory '<パス>' does not exist` をエラーにする（`src/libgit2/worktree.c:278-282`）。

### ロックの所有者を判定できない理由を示さない

`src/locks.rs:47-53` は、`kill(pid, 0)` が ESRCH 以外のエラーを返すと、所有者の状態を不明とする。
`src/locks.rs:60-67` は、`ps` が失敗したときと、出力が空か複数行のときにも不明とする。
不明のときの理由は、どの場合も `cannot establish that lock owner exited`（`src/locks.rs:220`）である。
調査では、Claude Code のサンドボックスの中で実行すると peg-hs の 8 件がこの理由で保持され、サンドボックスの外では同じ 8 件が所有者の判定を通過した。

### gh の失敗の原因を示さない

`src/github.rs:104` は gh の標準エラーを捨て、`src/github.rs:138-139` は終了状態だけを理由にする。
調査では、sofel の 4 件が `GitHub query failed: exit status: 1` で保持された。
そのうち 1 件の HEAD で同じ API を gh で呼ぶと、`No commit found for SHA` と HTTP 422 が返った。
4 件とも、HEAD は未 push のコミットだった。

### PR のマージコミットがローカルにないことを示さない

`src/git.rs:466-469` は、`git_graph_descendant_of` が 0 と -3 以外を返すと、libgit2 のエラーを理由にする。
調査では、13 件が `check PR merge commit ancestry: object not found - no match for id (<コミット ID>)` で保持された。
この文言は libgit2 がそのコミットのオブジェクトを見つけられなかったことを示すが、利用者が何をすればよいか（fetch など）は示さない。

## 実装すること

判定は変えず、保持の理由の文言だけを変える。
一覧の行は、今と同じく 4 列とする。

### 変更の種類

ここでいう status の段は、`ensure_clean` の中で `git_status_foreach` を呼ぶ部分である。
status の段のオプションは、今と同じく `GIT_STATUS_OPT_DEFAULTS` とする。
status の段では、コールバックに渡る状態のフラグから、変更の種類を次の 3 つに分ける。

| 種類 | フラグ | 理由 |
| --- | --- | --- |
| 無視 | `GIT_STATUS_IGNORED` だけ | `ignored file: <パス>` |
| 未追跡 | `GIT_STATUS_WT_NEW` だけ | `untracked file: <パス>` |
| 追跡 | 上の二つのどちらでもないもの（`GIT_STATUS_INDEX_NEW`、`GIT_STATUS_WT_MODIFIED`、`GIT_STATUS_CONFLICTED`、`GIT_STATUS_WT_UNREADABLE` などを含む） | `tracked changes: <パス>` |

コールバックは最初の変更で止めず、すべての項目を見る。
理由には、追跡、未追跡、無視の順で最も前の種類を選び、その種類でコールバックに最初に渡ったパスを書く。
パスは、コールバックに渡る worktree のルートからの相対パスである。
無視されたディレクトリは、libgit2 が渡すとおり、末尾に `/` の付いたディレクトリのパスになる。

status の段の後の、インデックスの検査とファイルの走査の文言（`submodule in worktree`、`nested repository`、走査の `untracked file: <絶対パス>` など）は変えない。

### `.git` ファイルがない登録

`evaluate` で `git_worktree_lookup` の直後、`git_worktree_validate` の前に、登録の作業ディレクトリと、その中の `.git` があるかを調べる。
この検査はロックの有無の検査（`src/git.rs:494-500`）より前に行うので、ロック付きの登録にも当てはまる。

作業ディレクトリがないときは、理由を `worktree directory is missing` で始める。
作業ディレクトリはあり、`.git` がないときは、理由を `worktree .git file is missing` で始める。
どちらの場合も、登録の管理ディレクトリ（`<共通ディレクトリ>/worktrees/<名前>`）の `HEAD` を読み、続けて次のどれかを書く。

| `HEAD` の状態 | 続ける文言 |
| --- | --- |
| ブランチを指し、そのブランチがある | `; HEAD is branch <ブランチ名>` |
| ブランチを指し、そのブランチがない | `; HEAD is missing branch <ブランチ名>` |
| コミットを直接指し、そのコミットが `refs/heads/` か `refs/remotes/` の参照の先端と同じか祖先である | `; detached HEAD <コミット ID> is reachable from <参照名>` |
| コミットを直接指し、どの参照からも到達できない | `; detached HEAD <コミット ID> is not reachable from any branch or remote-tracking branch` |
| `HEAD` を読めないか、到達の判定に失敗した | `; cannot inspect HEAD: <エラー>` |

`<ブランチ名>` は `refs/heads/` を除いた名前とする。
`<参照名>` は `refs/heads/` か `refs/remotes/` を除いた名前とする。
到達できる参照が複数あるときは、除く前の完全な参照名をバイト列として比べ、最も小さいものを書く。
そのため、ローカルのブランチから到達できるときは、ローカルのブランチの名前が選ばれる。

### ロックの所有者を判定できない場合

不明とした理由を、次のように `cannot establish that lock owner exited` の後ろに書く。

| 場合 | 続ける文言 |
| --- | --- |
| `kill(pid, 0)` が ESRCH 以外で失敗 | `: kill(<PID>, 0) failed with <errno の名前>` |
| `ps` が 0 以外で終了 | `: ps exited with <終了状態>` |
| `ps` の出力が空か複数行 | `: ps returned no single start time` |
| macOS 以外 | `: process inspection is unsupported on this platform` |

errno の名前は、`EPERM` のような記号の名前とする。
名前の分からない値は、`errno <数>` と書く。

### gh の失敗

gh の標準エラーを、標準出力と同じく別のスレッドで最後まで読み、先頭の 64 KiB だけを残す。
UTF-8 として読めないバイトは、置き換え文字にする。
終了状態が 0 以外のときは、理由を `GitHub query failed: <終了状態>: <標準エラーの最初の空でない行>` とする。
行は前後の空白を除き、200 バイトを超えるときは、200 バイト以下で最も長くなる UTF-8 の文字の境界で切る。
標準エラーに空でない行がないときは、今と同じ `GitHub query failed: <終了状態>` とする。

### PR のマージコミットがローカルにない場合

`git_graph_descendant_of` が失敗したら、libgit2 でマージコミットを引き、オブジェクトがあるかを確かめる。
オブジェクトがないときは、理由を `merge commit <コミット ID> of <PR の URL> is not in the local repository; fetch origin to check it` とする。
そのほかの失敗は、今と同じく libgit2 のエラーを理由にする。

## 実装しないこと

- 判定の変更。どの worktree も、この ADR の前と同じく候補か保持かに分かれる。
- `.git` ファイルがない登録の削除と修復。[ADR 0001](0001-worktree-cleanup.md) の「実装しないこと」のとおり、存在しない作業ディレクトリの登録解除は対象外のままとする。
- gh の HTTP の状態コードの解析。標準エラーの行をそのまま示す。
- 候補の行の理由の変更。
- status の段の後の検査の文言の変更。
- 変更の種類ごとの件数の表示。status の段の項目の数は、未追跡のディレクトリと無視されたディレクトリで数え方が違い、ファイルの数を表さないためである。

## 不変条件

- I1: 同じリポジトリの状態とオプションでは、各 worktree の判定（候補か保持か）は、この ADR を入れる前と同じである。
- I2: status の段で保持した worktree の理由は、追跡の項目があれば `tracked changes: ` で、なければ未追跡の項目があれば `untracked file: ` で、どちらもなければ `ignored file: ` で始まる。
- I3: status の段で保持した worktree の理由のパスは、選んだ種類の項目のうち、コールバックに最初に渡ったもののパスである。
- I4: 作業ディレクトリはあり `.git` がない登録の理由は、ロックの有無にかかわらず `worktree .git file is missing; ` で始まり、`HEAD` の状態の表の文言のどれか一つで終わる。
- I5: 作業ディレクトリがない登録の理由は、ロックの有無にかかわらず `worktree directory is missing; ` で始まり、`HEAD` の状態の表の文言のどれか一つで終わる。
- I6: I4 と I5 の理由は、detached HEAD のコミットがどの `refs/heads/` と `refs/remotes/` の参照からも到達できないときに限り、`is not reachable from any branch or remote-tracking branch` を含む。
- I7: ロックの所有者を判定できずに保持した理由は、`cannot establish that lock owner exited: ` で始まり、不明とした場合の表の文言のどれか一つを含む。
- I8: gh が 0 以外で終了し、標準エラーに空でない行があるとき、理由はその最初の空でない行を、前後の空白を除き 200 バイト以下にしたものを含む。標準エラーに空でない行がないとき、理由は `GitHub query failed: <終了状態>` と一致する。
- I9: PR のマージコミットのオブジェクトがローカルにないとき、理由はそのコミット ID と PR の URL を含む。
- I10: どの理由も、一覧の行の 4 列を崩さない（理由の欄のタブは `\t` の 2 文字に、改行は `\n` の 2 文字になる）。
- I11: gh の標準エラーが 64 KiB を超えても、gh の終了を待つ処理は標準エラーの書き込みで止まらず、30 秒のタイムアウトより前に理由を返す。
- I12: 到達できる参照が複数あるとき、I4 と I5 の理由の `<参照名>` は、完全な参照名をバイト列として比べて最も小さいものから作る。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 変更なし、無視ファイルなし | 判定は今と同じ | I1 |
| 無視されたディレクトリ `target/` だけがある | `ignored file: target/` | I1, I2, I3 |
| 未追跡のファイル `a` と無視されたディレクトリ `target/` がある | `untracked file: a` | I2, I3 |
| ステージ済みの変更 `b` と未追跡のファイル `a` がある | `tracked changes: b` | I2, I3 |
| 未追跡のディレクトリ `d/` に 2 ファイルがある | `untracked file: d/<ファイル名>`（ディレクトリではなくファイルのパス） | I2, I3 |
| `.git` がなく、HEAD がブランチ `x` を指す | `worktree .git file is missing; HEAD is branch x` | I4 |
| `.git` がなく、HEAD が消えたブランチ `x` を指す | `worktree .git file is missing; HEAD is missing branch x` | I4 |
| `.git` がなく、ロック付き | `worktree .git file is missing; ` で始まる | I4 |
| `.git` がなく、detached HEAD が `origin/main` の祖先で、どのローカルブランチからも到達できない | `is reachable from origin/main` で終わる | I4, I6, I12 |
| `.git` がなく、detached HEAD が `main` と `origin/main` の両方の祖先 | `is reachable from main` で終わる | I4, I12 |
| `.git` がなく、detached HEAD がどの参照からも到達できない | `is not reachable from any branch or remote-tracking branch` で終わる | I4, I6 |
| 作業ディレクトリがなく、HEAD がブランチ `x` を指す | `worktree directory is missing; HEAD is branch x` | I5 |
| `.git` がなく、管理ディレクトリの `HEAD` を読めない | `; cannot inspect HEAD: ` を含む | I4 |
| `kill` が EPERM | `kill(<PID>, 0) failed with EPERM` を含む | I7 |
| `ps` が 1 で終了 | `ps exited with exit status: 1` を含む | I7 |
| `ps` の出力が空 | `ps returned no single start time` を含む | I7 |
| `ps` の出力が 2 行 | `ps returned no single start time` を含む | I7 |
| gh が 1 で終了し、標準エラーが 1 行 | その行を含む | I8 |
| gh が 1 で終了し、標準エラーが空 | `GitHub query failed: exit status: 1` と一致する | I8 |
| gh の標準エラーの最初の行が 201 バイト | 200 バイトまでに切る | I8 |
| gh の標準エラーが 64 KiB を超える | タイムアウトせず、最初の行を含む | I8, I11 |
| gh の標準エラーの行にタブがある | 理由の欄ではタブが `\t` になる | I10 |
| マージコミットがローカルにない | コミット ID と PR の URL を含む | I9 |

macOS 以外の場合の文言は、macOS では起きないため、境界の表に入れずに単体テストで確かめる。

## 受けいれ条件

- A1: 一時リポジトリの linked worktree の `.git` を消し、一覧の理由が `worktree .git file is missing; HEAD is branch <名前>` になることを手で確かめる。
- A2: サンドボックスの外で `~/repositories/src/github.com/tett23/sofel` を `--github-merged` で調べ、未 push の HEAD の worktree の理由に gh の標準エラーの行（`No commit found for SHA` を含む行）が出ることを手で確かめる。未 push の HEAD の worktree がなければ、一時の worktree で未 push のコミットを作る。

## テスト設計

テストの名前は `adr0007_i<番号>_<内容>` とする。

- I1 は、[ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) の今のテストが、判定を変えずに通ることで確かめる。理由の文言を照合している今のテストは、新しい文言に合わせて直す。
- I2 と I3 は、一時リポジトリの linked worktree に、境界の表の変更の組み合わせを一つずつ作り、一覧の理由の全体を照合する。
- I4 から I6 と I12 は、linked worktree を作った後に `.git` ファイルを消す。ブランチ、消したブランチ、`origin/main` の祖先の detached HEAD、どの参照からも到達できない detached HEAD を試す。作業ディレクトリそのものを消す場合と、`git worktree lock` でロックした場合と、管理ディレクトリの `HEAD` を消す場合も試す。
- I7 は、`src/locks.rs` のプロセスの照会を差し替える単体テストで、不明とした場合の表の 4 つを照合する。
- I8、I10、I11 は、`src/github.rs` の今のテストと同じく偽の gh を使い、標準エラーの行、空の標準エラー、201 バイトの行、64 KiB を超える出力、タブを含む行を試す。
- I9 は、偽の gh にローカルにないマージコミット ID を返させる。

## トレードオフ

- 変更の種類を選ぶため、status の段は最初の変更で止まらず、すべての項目を見る。変更の多い worktree では、今より判定が遅くなる。
- `.git` ファイルがない登録では、到達の判定のためにすべてのブランチとリモート追跡ブランチを調べる。参照の多いリポジトリでは、その登録の判定が遅くなる。
- 理由の文言を照合している利用者の処理は、文言の変更に合わせて直す必要がある。判定の列（`candidate`、`removed`、`keep`）は変わらない。

## 参考文献

- [調査のまとめ: 2026-11-16 tett23 配下の worktree 整理の判断記録](../research/2026-11-16-worktree-cleanup-tett23.md)
- [libgit2 v1.9.7 include/git2/status.h](https://github.com/libgit2/libgit2/blob/v1.9.7/include/git2/status.h)
- [libgit2 v1.9.7 src/libgit2/status.c](https://github.com/libgit2/libgit2/blob/v1.9.7/src/libgit2/status.c)
- [libgit2 v1.9.7 src/libgit2/worktree.c](https://github.com/libgit2/libgit2/blob/v1.9.7/src/libgit2/worktree.c)
- [git-status](https://git-scm.com/docs/git-status)
