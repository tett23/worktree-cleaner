# 2026-11-16 tett23 配下の worktree 整理の判断記録

この文書は、`~/repositories/src/github.com/tett23` 配下の linked worktree を整理したときの判断と、その過程で見つかった CLI の改善点を記録する。
CLI の判定だけでは削除できなかった worktree を手作業で判定したため、その基準を CLI に取り込むかどうかを検討する材料として残す。
記録した事実は、本文に示したコマンドを 2026-11-16 08:00 から 08:20（JST）に実行して確かめたものである。
2026-11-17 に、同じ作業の間に保存した一覧の出力と集計の表（各 worktree の状態、PR、更新時刻）を集計し直し、いくつかの数を足した。
足した数は、どれも 2026-11-16 の作業中に取った出力からのものであり、2026-11-17 に測り直したものではない。

## 結果の要約

整理の前後で、linked worktree の数は 236 件から 124 件になった。
削除した 112 件の内訳は次のとおりで、ブランチ参照はどれも削除していない。

| 区分 | 件数 |
| --- | ---: |
| 統合済みで clean（無視ファイルはあってもよい）、ロックなし | 88 |
| 統合済みで clean、所有者が終了した Claude エージェントのロックを解除 | 5 |
| macOS の /tmp 掃除で `.git` ファイルと追跡ファイルが消えていた登録 | 19 |

リポジトリ別では sofel 75 件、golem 18 件、peg-hs 12 件、ai-chat-logs 4 件、shem 3 件である。
個々のパスは付録に載せる。

## CLI の出力

最初に、main の先端（`93cbf6b`）をビルドした CLI で一覧だけを実行した。
`worktree-cleaner --root ..` は 236 件すべてを keep と判定し、最初の保持理由の内訳は次のとおりだった。

| 理由 | 件数 |
| --- | ---: |
| branch is not merged into base | 115 |
| worktree HEAD is detached | 67 |
| No such file or directory (os error 2) | 23 |
| working tree or index has changes, including untracked or ignored files | 15 |
| worktree is locked | 12 |
| Git operation is in progress | 4 |

`--github-merged --reclaim-agent-locks` を加えても、候補は 0 件だった。
このとき理由の内訳は、無視ファイルを含む変更ありが 119 件、PR のマージコミットがローカルにない（`check PR merge commit ancestry: object not found - no match for id (<コミット ID>)`）が 13 件、`GitHub query failed: exit status: 1` が 4 件などに変わった。
この実行は Claude Code のサンドボックスの外で行った。
サンドボックスの中では gh がキーチェーンの認証情報を読めず、GitHub への照会がすべて失敗した。

## 手作業の判定基準

そこで、各 worktree について Git の状態と GitHub の PR を集め、次の基準で判定した。
状態の収集には `git worktree list --porcelain`、`git status --porcelain --ignored -unormal`、`git merge-base --is-ancestor` を使った。
PR は `gh pr list --state all --limit 1000 --json number,state,headRefName,headRefOid,mergedAt,baseRefName,mergeCommit,url` でリポジトリごとに一度だけ取得し、HEAD のコミット ID とブランチ名で突き合わせた。

削除の条件は、次のすべてを満たすことである。

1. HEAD がローカルの基準ブランチ、`origin/<基準>`、または HEAD と同じ先端でマージ済みの PR のいずれかに含まれる。
2. 追跡ファイルの変更と、無視されていない未追跡ファイルがない。無視ファイルはあってもよい。
3. 進行中の Git 操作（`MERGE_HEAD` など）がない。
4. ロックがないか、ロックが所有者の終了した Claude エージェントのものである。
5. worktree の管理ディレクトリの `HEAD` と `index` が、24 時間以上更新されていない。
6. どのプロセスも、worktree の中をカレントディレクトリにしていない（`lsof -d cwd` で確認）。

削除は `git worktree remove` で行い、`--force` を付けなかった。
Git はこのとき無視ファイルを削除の妨げにしないため、条件 2 の確認を Git 側でも重ねて行うことになる。
削除の直前には、HEAD、ブランチ、ロック理由、clean であることを再確認した。

### 無視ファイルを削除の妨げにしなかった理由

無視ファイルの中身を確かめたところ、再生成できるものだけだった。
具体的には `target/`、`dist-newstyle/`、`node_modules/`、`.cache/`、`cabal.project.local`、`.DS_Store`、`*.hi`、`experiments/` 配下の `*.log` である。
[ADR 0001](../adr/0001-worktree-cleanup.md) の不変条件 7 は無視ファイルがあれば保持すると定めており、この判定は CLI の既定より広い。

### origin の基準を統合の根拠にした理由

ローカルの main が `origin/main` より大きく遅れていたため、ローカルの基準だけでは統合済みの worktree の多くを判定できなかった。
遅れは `git rev-list --count main..origin/main` で、sofel が 1107 コミット、peg-hs が 413、golem が 241 だった。
これらのリポジトリの `.git/FETCH_HEAD` の更新時刻（`stat`）は、sofel が 06:35、peg-hs が 06:49、golem が 07:32 で、どれも当日だった。
`origin/main` は前回の fetch で記録されたリモート追跡ブランチであり、そこに含まれる HEAD は GitHub の main に取り込まれている。
削除した 88 件のうち 82 件は、この根拠だけで統合済みと判定した。

### 作業中の worktree を除いた理由

`.claude/worktrees/adr-guard-v0.13.0` という名前の 19 件は、どれも当日 06:26 から 07:56 に `HEAD` か `index` が更新されていた。
そのうち worktree-cleaner の 1 件では、07:38 に起動した zsh がその中をカレントディレクトリにしていた。
adr-guard リポジトリの 5 件も 1 から 3 時間前に更新されていた。
これらは別のセッションが作業中と判断し、24 件を残した。
24 件は、どれもほかの削除の条件（条件 1 から 4）を満たしていた。
削除した 112 件の管理ディレクトリの `HEAD` と `index` の更新は、どれも 63 時間以上前だった。

## ロックの判定

ロックのある 12 件のうち、所有者の終了と統合を確かめた 5 件だけを解除して削除した。

peg-hs の 8 件のロック理由は、すべて `claude agent <登録名> (pid 74012 start Thu Nov  5 16:31:45 2026)` の形式だった。
`ps -p 74012 -o user=,lstart=,command=` は、同じ利用者の `siriinferenced` が `Wed Nov 11 09:44:36 2026` に起動したことを示した。
記録された起動日時との差は 48 時間を超えるため、[ADR 0003](../adr/0003-reclaim-agent-locks-and-check-merged-prs.md) の基準でも PID の再利用であり、ロックの所有者は終了している。
8 件のうち 5 件は PR がマージ済みで、無視ファイルしかなかったため、ロックを解除して削除した。
残りの 3 件には追跡ファイルの変更があるため、ロックも作業ディレクトリもそのまま残した。
削除に失敗した場合は元の理由で再ロックする手順を用意したが、失敗はなかった。

golem の /tmp にある 4 件のロック理由は `added with --lock` が 3 件、`active-adr0372` が 1 件で、Claude エージェントの形式ではない。
4 件とも PR がオープンなので、手動のロックとして残した。

## `.git` が消えた登録の判定

/tmp に置かれた 23 件では、作業ディレクトリの `.git` ファイルと追跡ファイルがすべて消えていた。
CLI はこれらを `No such file or directory (os error 2)` と表示していた。
登録側の管理ディレクトリを `GIT_DIR` と `GIT_WORK_TREE` に指定して状態を見ると、追跡ファイルはすべて削除扱いで、残っていたのはビルド成果物とキャッシュだけだった。
この消え方は macOS の定期的な /tmp の掃除によると推測するが、原因は未確認である。

23 件のうち 19 件は、HEAD がブランチまたはリモート追跡ブランチから到達できた。
これらは `git worktree repair` で `.git` ファイルを戻してから、`git worktree remove --force` で削除した。
`--force` は、削除扱いの追跡ファイルがあるために必要だった。

残りの 4 件は sofel の detached HEAD で、HEAD のコミットはどの参照からも到達できなかった。

| パス | HEAD | 基準にないコミット |
| --- | --- | ---: |
| `/private/tmp/sofel-109-subset-check` | `e632b90e` | 1 |
| `/private/tmp/sofel-135-138-integration` | `738686f4` | 3 |
| `/private/tmp/sofel-static-colrv1` | `6224451b` | 1 |
| `/private/tmp/sofel-subset-var-integration` | `bac18cb3` | 1 |

これらの登録を消すとコミットが参照を失うため、削除しなかった。
また、登録は `git worktree list` で `prunable` と表示されており、`git worktree prune` や `git gc` で消えるおそれがあった。
そのため `git worktree repair` で `.git` ファイルだけを戻し、`prunable` の表示が消えたことを確かめた。

## 保持した worktree

残した 124 件の内訳は次のとおりである。

| 理由 | 件数 |
| --- | ---: |
| 統合を確認できない（オープンの PR、PR なし、マージ後の追加コミット） | 53 |
| 追跡ファイルの変更、または無視されていない未追跡ファイルがある | 39 |
| 作業中（24 時間以内の更新） | 24 |
| 参照のない detached HEAD で `.git` が消えていた | 4 |
| 進行中のマージ（`MERGE_HEAD`） | 4 |

未コミットの内容は、ADR の下書き、ソースの変更、検証用スクリプトなどだった。
統合を確認できない 53 件のうち 8 件は、マージ済み PR の先端より 1 から 4 コミット進んでいた。
その多くは `Merge remote-tracking branch 'origin/main'` や `Integrate current main before final checks` のような main の取り込みだったが、内容の差は確かめずに残した。
また、統合を確認できない 53 件のうち、sofel の detached HEAD の 13 件は、PR がなく、HEAD がどのブランチとリモート追跡ブランチからも到達できなかった（`git for-each-ref --contains <HEAD> refs/heads refs/remotes`）。

## CLI の改善候補

ここからは、今回の作業で観察した事実と、そこから考えた改善案を分けて書く。
改善案は検討の材料であり、採否は ADR で決める。

### 無視ファイルだけの worktree を削除できない

観察: 削除した 88 件のうち、ブランチ上の 53 件はどれも無視ファイルがあった。
`--github-merged --reclaim-agent-locks` の実行では、53 件すべてがこの無視ファイルを理由に保持された。
既定の一覧では、そのうち 48 件がローカルの基準の遅れによって `branch is not merged into base` で、5 件が無視ファイルで保持された。
`--github-merged --reclaim-agent-locks` の一覧で変更ありを理由に保持された 119 件を `git status --porcelain --ignored -unormal` の行で分けると、93 件は無視ファイルしかなく、26 件には追跡ファイルの変更か無視されない未追跡のファイルがあった。
保持理由の文言 `working tree or index has changes, including untracked or ignored files` は、無視ファイルだけの場合と、未コミットの変更がある場合を区別しない。

案: 保持理由を、追跡ファイルの変更、未追跡ファイル、無視ファイルに分けて表示する。
さらに、無視ファイルだけなら削除してよいことを明示するオプションを設ける。

### 基準に含まれる detached HEAD の worktree を削除できない

観察: 削除した 88 件のうち、残りの 35 件は detached HEAD だった。
35 件の HEAD は、1 件がローカルの main に、34 件が `origin/main` に含まれていた。
35 件のうち 20 件には無視ファイルがあり、15 件には無視ファイルもなかった。
CLI は 35 件すべてを、既定の一覧でも `--github-merged --reclaim-agent-locks` の一覧でも `worktree HEAD is detached` で保持した。

案: HEAD のコミットが基準に含まれる detached HEAD の worktree を、明示の指定で候補にする。
このとき削除で失われるのは、作業ディレクトリのパスと、管理ディレクトリの worktree 固有の状態（HEAD の reflog、`ORIG_HEAD`、`FETCH_HEAD`、worktree 固有の参照など）である。

### ローカルの基準ブランチが古いと判定できない

観察: 統合済みの 82 件は、`origin/main` に含まれていたが、ローカルの main には含まれていなかった。
`--github-merged` でも、PR のマージコミットのオブジェクトがローカルのリポジトリにないため、別の 13 件が `object not found` で保持された。
この 13 件の HEAD は、ローカルの main にも `origin/main` にも含まれていなかった（`git merge-base --is-ancestor`）。
整理後に同じコマンドを実行すると、作業中として残した `adr-guard-v0.13.0` の 13 件が同じ理由で保持された。

案: 基準として `origin/<基準>` のリモート追跡ブランチを選べるようにする。
また、マージコミットがローカルにない場合は `object not found` ではなく、fetch が要ることを示す理由を表示する。

### `.git` が消えた登録の理由が分かりにくい

観察: 23 件の理由は `No such file or directory (os error 2)` だけで、どのファイルがないのかを示さなかった。
`git worktree list` はこれらを `prunable` と表示していた。

案: `.git` ファイルがない登録を、独立した理由として表示する。
HEAD が参照から到達できるかどうかも併せて表示すれば、整理してよい登録と、コミットを失う登録を区別できる。

### サンドボックスの中では所有者の終了を判定できない

観察: Claude Code のサンドボックスの中で実行すると、peg-hs の 8 件は `cannot establish that lock owner exited` と表示された。
サンドボックスの外では、同じ 8 件の所有者判定を通過した。
`src/locks.rs` の `probe_process` は、`kill(pid, 0)` が ESRCH 以外のエラーを返すと理由を区別せずに不明とする。

案: 不明とした理由に errno（EPERM など）を含める。

### `GitHub query failed: exit status: 1` が原因を示さない

観察: sofel の 4 件がこの理由で保持されたが、gh の標準エラーは表示されなかった。
4 件とも、HEAD はどのリモート追跡ブランチにも含まれない未 push のコミットだった。
そのうち `/private/tmp/sofel-0064-harmony` の HEAD（`5295b838`）で `gh api repos/tett23/sofel/commits/<HEAD>/pulls` を実行すると、`No commit found for SHA` と HTTP 422 が返った。

案: gh の標準エラーの先頭を理由に含める。
HTTP 422 の場合は、HEAD が GitHub にないことを示す理由を表示する。

### 作業中の worktree を検出しない

観察: 削除の条件 1 から 4 を満たす worktree のうち 24 件は、管理ディレクトリの `HEAD` か `index` が当日に更新され、うち 1 件では別のシェルが動いていた。
CLI はこれらを区別せず、条件がそろえば候補にする。

案: 管理ディレクトリの `HEAD` や `index` の更新時刻が一定時間内のものを保持する、または一覧に最終更新日時を表示する。

### `git worktree repair` が指定外の登録も直す

観察: `git worktree repair <パス>` を一つのパスで実行すると、同じリポジトリのほかの壊れた登録の `.git` ファイルも戻った。
最初に指定したパスについては終了コードが 0 以外だった。
CLI が修復を扱う場合は、この挙動を前提に再検査する必要がある。

## 付録: 削除した worktree

統合の根拠の「なし（ブランチで保持）」は、`.git` が消えた登録のうち、統合は確認できないが、HEAD がブランチから到達できたものを指す。

| リポジトリ | パス | ブランチ | 区分 | 統合の根拠 |
| --- | --- | --- | --- | --- |
| ai-chat-logs | `/private/tmp/ai-chat-logs-codex-db` | `codex/impl-0033` | `.git` 消失 | ローカルの基準 |
| ai-chat-logs | `/private/tmp/ai-chat-logs-codex-import` | `codex/impl-0034` | `.git` 消失 | ローカルの基準 |
| ai-chat-logs | `/private/tmp/ai-chat-logs-codex-ui` | `codex/impl-0032` | `.git` 消失 | ローカルの基準 |
| ai-chat-logs | `/private/tmp/ai-chat-logs-presentation` | `codex/impl-0037` | `.git` 消失 | ローカルの基準 |
| golem | `/private/tmp/golem-0354-table-bom` | `codex/0354-table-bom` | 通常 | origin の基準 |
| golem | `/private/tmp/golem-960-number` | `(detached)` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-967-visual-fix` | `(detached)` | 通常 | origin の基準 |
| golem | `/private/tmp/golem-fix-small-footnote` | `codex/fix-small-footnote` | 通常 | origin の基準 |
| golem | `/private/tmp/golem-impl-0319` | `codex/impl-0319` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-impl-0324` | `codex/impl-0325` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-impl-0328` | `codex/impl-0338` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-impl-0332` | `codex/impl-0332` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-impl-0336` | `codex/impl-0336` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-impl-0339` | `codex/impl-0339` | `.git` 消失 | なし（ブランチで保持） |
| golem | `/private/tmp/golem-impl-0339-comments` | `codex/impl-0339-comments` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-impl-0341` | `codex/impl-0341` | `.git` 消失 | なし（ブランチで保持） |
| golem | `/private/tmp/golem-impl-0342` | `codex/impl-0342` | `.git` 消失 | なし（ブランチで保持） |
| golem | `/private/tmp/golem-impl-0343` | `codex/impl-0343` | `.git` 消失 | なし（ブランチで保持） |
| golem | `/private/tmp/golem-impl-0344` | `codex/impl-0344` | `.git` 消失 | なし（ブランチで保持） |
| golem | `/private/tmp/golem-impl-0345` | `codex/impl-0345` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-impl-0346` | `codex/impl-0346` | `.git` 消失 | origin の基準 |
| golem | `/private/tmp/golem-impl-0348` | `codex/impl-0348` | `.git` 消失 | origin の基準 |
| peg-hs | `/private/tmp/peg-hs-0097-env` | `verify-0097-literal-continuation` | 通常 | origin の基準 |
| peg-hs | `/private/tmp/peg-hs-0097-haskell` | `verify-0097-haskell-derivations` | 通常 | origin の基準 |
| peg-hs | `/private/tmp/peg-hs-0097-printer-composition` | `verify-0097-printer-composition` | 通常 | origin の基準 |
| peg-hs | `/private/tmp/peg-hs-0097-subst` | `verify-0097-template-substitution` | 通常 | origin の基準 |
| peg-hs | `/private/tmp/peg97-check-2e411a8` | `(detached)` | 通常 | origin の基準 |
| peg-hs | `/private/tmp/peg97-check-40096a8` | `(detached)` | 通常 | origin の基準 |
| peg-hs | `/private/tmp/peg97-verification-d97ab31` | `(detached)` | 通常 | origin の基準 |
| peg-hs | `peg-hs/.claude/worktrees/agent-a5299d311a39be515` | `impl-0093` | ロック解除 | ローカルの基準 |
| peg-hs | `peg-hs/.claude/worktrees/agent-a5636d6adbf0656af` | `impl-0095` | ロック解除 | origin の基準 |
| peg-hs | `peg-hs/.claude/worktrees/agent-a5e1fc6548760fa6c` | `impl-0094` | ロック解除 | origin の基準 |
| peg-hs | `peg-hs/.claude/worktrees/agent-a7940f395cdd54184` | `impl-0090` | ロック解除 | ローカルの基準 |
| peg-hs | `peg-hs/.claude/worktrees/agent-aac789754560d1946` | `impl-0092` | ロック解除 | ローカルの基準 |
| shem | `shem/.claude/worktrees/agent-a4ac5c3b93202de5d` | `impl/0027-operators-strokes-clip` | 通常 | ローカルの基準 |
| shem | `shem/.claude/worktrees/agent-aba5d87fe8c1e65fb` | `impl/0027-jpeg` | 通常 | ローカルの基準 |
| shem | `shem/.claude/worktrees/agent-adbbcb1996d560d73` | `worktree-agent-adbbcb1996d560d73` | 通常 | ローカルの基準 |
| sofel | `/private/tmp/sofel-0027-api` | `codex/adr0027-upstream-api-cases` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0027-glyf-geometry` | `codex/adr0027-glyf-geometry-corpus` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0027-random-location` | `codex/adr0027-random-location-reference` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0029-ft-gvar-fuzz` | `codex/adr0029-ft-gvar-fuzz` | 通常 | ローカルの基準 |
| sofel | `/private/tmp/sofel-0060-headless` | `codex/adr0060-headless-shaping` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0060-ltag` | `codex/adr0060-ltag` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0063-fallback` | `codex/adr0063-fallback-synthesis` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0064-cff-native` | `codex/adr0064-cff-native` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0064-ci-fix` | `codex/adr0064-ci-fix` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0064-hint-reference` | `codex/adr0064-hint-reference` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0064-latin-d` | `codex/adr0064-latin-d` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0064-latin-edges` | `codex/adr0064-latin-edges` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0064-latin-stems` | `codex/adr0064-latin-stems` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0064-native-autohint` | `codex/adr0064-auto-style` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0065-afm` | `codex/adr0065-afm` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0065-cid-type1` | `codex/adr0065-cid-type1` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0065-input` | `codex/adr0065-woff-input` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0065-type1-face` | `codex/adr0065-type1-face` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0065-type42` | `codex/adr0065-type42` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0065-type42-render` | `codex/adr0065-type42-render` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0066-acceptance` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0067-gvar-pin` | `codex/adr0061-cvar-cvt` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0068-gpu-records` | `codex/adr0068-gpu-records` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0072-colr` | `codex/adr0072-colr-reference` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0075-layout-tags` | `codex/adr0075-layout-tag-enumeration` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-0076-stroke` | `codex/adr0076-outline-stroke` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-215-mono-sync` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-224-reconcile-v2` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-236-integration` | `codex/adr0066-clip-and-invariants-integration` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-239-sync-nov12` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-240-merge-main` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-245-gx-integration` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-245-reconcile` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-247-fixed-list` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-248-reconcile` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-257-pi-constant` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-263-ci-fix` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-285-integration` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-286-resolve` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-290-int` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-313-head-check` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-321-resolve` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-325-cr-lines` | `codex/type1-validation` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-adr0061-avar-identity` | `codex/adr0061-avar-identity` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-adr0061-fvar-locations` | `codex/adr0061-fvar-locations` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-adr0061-gdef-row-order` | `codex/adr0061-gdef-row-order` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-adr0061-instance` | `codex/adr0061-partial-instance` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-adr0061-varc-empty` | `codex/adr0061-varc-clean` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-adr0073-gallery` | `codex/adr0073-rust-gallery` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-avar2-ci-fix` | `codex/adr0061-avar2-ci-fix` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-avar2-full` | `codex/adr0061-avar2-full` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-bdf-validator` | `codex/bdf-validator` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-check-emoji-main` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-check302` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-colr-full` | `codex/adr0061-colr-full` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-gallery-77` | `gallery/noto-v2047` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-gvar-multiple-pins` | `codex/adr0061-gvar-multiple-pins` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-hvar-axis-ranges` | `codex/adr0061-base-axis-range` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-int295` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-int296` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-int299` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-integrate-254-now` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-integrate-256-7e8f` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-integrate-256-bc58` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-integrate291` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-integrate293` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-morx-v3` | `codex/adr0060-morx-v3` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-name-overrides` | `codex/adr0022-name-overrides` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-pr249-main-integration` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-resolve-294` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-subset-baseline-6479ac6f` | `(detached)` | 通常 | origin の基準 |
| sofel | `/private/tmp/sofel-wide-coverage` | `codex/adr0022-wide-context-rules` | 通常 | origin の基準 |
| sofel | `sofel/target/wt/adopt-sofel-cli` | `(detached)` | 通常 | origin の基準 |
| sofel | `sofel/target/wt/adr0048-finish-reference` | `codex/adr0048-finish-reference` | 通常 | ローカルの基準 |
| sofel | `sofel/target/wt/pr103-final-color-merge` | `(detached)` | 通常 | ローカルの基準 |
