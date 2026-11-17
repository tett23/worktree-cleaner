# ADR 0014: `--include-unmerged` の候補を、GitHub の照会の失敗で保持しない

Status: accepted

## 文脈

[ADR 0002](0002-include-unmerged-worktrees.md) の `--include-unmerged` は、未マージの HEAD を同じローカルブランチで保持でき、ほかの保護条件を満たす worktree を候補にする。
[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) の `--github-merged` は、ローカルで未マージと判定した clean なブランチについて、GitHub にマージ済みの PR を問い合わせる。
[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のアルゴリズムは、照会の失敗と、origin が github.com の既知の形式でないことを、理由を示して保持すると決めた。

しかし、二つを一緒に指定すると、照会に失敗した worktree は、`--include-unmerged` だけなら候補になる場合でも保持される。
[ADR 0013](0013-all-opt-in-candidates.md) の `--all` は二つを両方含むため、`--all` の候補は `--include-unmerged` だけの候補より少なくなりうる。
これは、`--all` で候補を最も多くするという [ADR 0013](0013-all-opt-in-candidates.md) の目的に合わない。

未マージの候補は、GitHub の証拠がなくても、ブランチがコミットを保持することを根拠に消せる。
そのため、照会の失敗は、この worktree を消してよいかの判断を変えない。
そこで、`--include-unmerged` を指定し、ロックのない worktree については、照会の失敗を保持の理由にせず、未マージの候補として扱う。

この ADR は、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のアルゴリズムの「GitHub による統合の確認」に、`--include-unmerged` を指定したときの例外を足す。
採択したときは、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のステータスに `{"part":"アルゴリズム","by":["docs/adr/0014-unmerged-candidates-despite-github-failures.md"],"kind":"supplement"}` を足す。

## 調査した事項

コードの位置は main の `72c4436` のものである。

- `src/git.rs:945-947` は、HEAD が基準に含まれず、`--github-merged` も `--include-unmerged` も指定されていないか、ロックがあれば、`branch is not merged into base` で保持する。
- `src/git.rs:969-973` は、HEAD が基準に含まれず `--github-merged` を指定したときに `merged_pr(...)?` を呼ぶ。`?` のため、`merged_pr` のエラーは `--include-unmerged` の判定（`src/git.rs:974-977`）より前に保持の理由として返る。
- `merged_pr`（`src/git.rs:591-663`）がエラーを返すのは、次の場合である。
  - origin を読めないか、origin に使える URL がない（`read origin`、`origin has no usable URL`）。
  - origin の URL が github.com の既知の形式でない（`src/github.rs:19` の `origin is not a supported github.com URL` など）。
  - gh の起動か照会が失敗した（`GitHub query failed: ...`）。
  - PR のマージコミットがローカルにない（`merge commit <ID> of <URL> is not in the local repository; fetch origin to check it`）。
- 2026-11-17 に、[ADR 0013](0013-all-opt-in-candidates.md) のテスト（`tests/all.rs`）を書いたとき、origin が一時ディレクトリの bare リポジトリの URL だと、未マージのブランチ上の worktree は `--all` で `origin is not a supported github.com URL` により保持された。origin の URL を `https://github.com/owner/project.git` にすると候補になった。
- 2026-11-18 に、`~/repositories/src/github.com/tett23` で `--all` の一覧を取った（158 行）。照会の失敗で保持した行は 16 件だった。内訳は、`merge commit <ID> of <URL> is not in the local repository; fetch origin to check it` が 13 件、`GitHub query failed: exit status: 1: gh: No commit found for SHA: <ID> (HTTP 422)` が 3 件だった。

## 実装すること

`--github-merged` と `--include-unmerged` を一緒に指定し、ロックのない worktree で照会が失敗したときは、保持せず、GitHub の証拠のない未マージの候補として扱う。
ここで照会の失敗とは、上の `merged_pr` のエラーのすべてを指す。
照会が成功し、条件を満たす PR がなかったときは、今と同じく未マージの候補とする。

この候補の理由は、`clean unmerged branch <ブランチ>; GitHub check failed: <原因>` とする。
`<原因>` は、今の保持の理由と同じ文言とする。
`--allow-ignored` を指定したときは、今と同じく理由の末尾に `; deletes N ignored files` を付ける。

削除の直前の再検査では、今と同じく照会をやり直し、初回と同じ証拠かを照らす。
照会の失敗の原因は証拠に含めず、理由の表示だけに使う。
そのため、再検査でも照会に失敗したか、条件を満たす PR がなければ削除する。
再検査で条件を満たす PR が見つかったときは、証拠が初回と変わったので、今と同じく保持する。

## 実装しないこと

- ロック付きの worktree の扱いの変更。[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のとおり、`--include-unmerged` は統合の確認を代替しないため、照会に失敗したロック付きの worktree は今と同じく保持する。
- `--include-unmerged` を指定しないときの扱いの変更。照会に失敗した worktree は、今と同じく保持する。
- 照会を省くこと。照会が成功すれば、理由に PR の URL を示せるためである。
- github.com 以外のホスト（GitHub Enterprise Server など）への照会。
- origin の fetch。

## 不変条件

- I1: `--github-merged` と `--include-unmerged` を指定したとき、ロックがなく、照会に失敗し、ほかの保護条件を満たす未マージの worktree は候補になる。
- I2: I1 の候補の理由は、`clean unmerged branch <ブランチ>; GitHub check failed: ` で始まり、続けて照会の失敗の原因を含む。
- I3: `--github-merged` と `--include-unmerged` を指定したとき、照会に失敗したロック付きの worktree は保持する。
- I4: `--include-unmerged` を指定しないとき、照会に失敗した worktree の判定と理由は、この ADR を入れる前と同じである。
- I5: I1 の候補を削除するとき、再検査の照会で条件を満たす PR が見つかれば削除しない。
- I6: I1 の候補を削除した後も、ブランチ参照は削除の直前と同じコミットを指す。

| 入力（ほかの条件は成立） | 期待 | 不変条件 |
| --- | --- | --- |
| origin が github.com でない、`--github-merged --include-unmerged` | 候補、`GitHub check failed: origin is not a supported github.com URL` | I1, I2 |
| gh が終了コード 1 で終わる、`--github-merged --include-unmerged` | 候補、`GitHub check failed: GitHub query failed: ...` | I1, I2 |
| gh が成功し PR がない、`--github-merged --include-unmerged` | 候補、`clean unmerged branch <ブランチ>` | I1 |
| origin が github.com でない、`--github-merged --include-unmerged --reclaim-agent-locks`、所有者が終了したロック付き | 保持 | I3 |
| origin が github.com でない、`--github-merged` | 保持、`origin is not a supported github.com URL` | I4 |
| 初回は gh が失敗し、再検査では条件を満たす PR を返す、`--remove-all` | 保持 | I5 |
| gh が失敗する、`--github-merged --include-unmerged --remove-all` | 削除され、ブランチは残る | I6 |

## 受けいれ条件

- A1: `~/repositories/src/github.com/tett23` で `--all` の一覧を取り、マージコミットがローカルにないことか gh の失敗を理由にした保持の行がなくなり、それらの worktree が `GitHub check failed:` を含む理由の候補になることを手で確かめる。

## テスト設計

テストの名前は `adr0014_i<番号>_<内容>` とする。

- I1 と I2 は、境界の表の最初の 3 行を一時リポジトリで作って一覧を照合する。gh は、今のテストと同じく、`PATH` の先頭に置いた偽の gh とする。
- I3 は、[ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md) のテストと同じ形の、所有者が終了した Claude のロックを付けた worktree で確かめる。
- I4 は、境界の表の 5 行目と、今ある `--github-merged` のテストが変わらずに通ることで確かめる。
- I5 は、呼ばれた回数をファイルに数える偽の gh で確かめる。偽の gh は、一度目（選択の照会）は終了コード 1 で終わり、二度目（再検査の照会）は条件を満たす PR を返す。`--github-merged --include-unmerged --remove-all` の後に、worktree が残り、その行が保持であることを確かめる。
- I6 は、境界の表の最後の行を `--remove-all` で削除し、ブランチのコミット ID を照合する。

## トレードオフ

- 照会の失敗が一時的なもの（ネットワークの断絶など）でも、その worktree を未マージとして消す。消してもコミットはブランチに残るため、失うものは [ADR 0002](0002-include-unmerged-worktrees.md) の未マージの候補と同じである。
- 理由の文言が長くなる。原因を示さないと、利用者は GitHub の証拠がなかった理由を知れない。
- マージコミットがローカルにない worktree は、fetch すれば統合済みと判定できる場合がある。この ADR の後は、その worktree を未マージとして消すため、理由の文言からは統合済みかがわからない。

## 参考文献

- [ADR 0002](0002-include-unmerged-worktrees.md)
- [ADR 0003](0003-reclaim-agent-locks-and-check-merged-prs.md)
- [ADR 0013](0013-all-opt-in-candidates.md)
