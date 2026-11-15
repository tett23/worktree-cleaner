# ADR 0005: adr-guard を v0.13.1 に上げ、ADR のステータス行を新形式に移す

Status: accepted

## 文脈

CI の `adr` ジョブは、リポジトリを作ったときから `tett23/adr-guard@v0.11.0` の action を使っている。
この版を決めた ADR はない。
`.githooks/pre-commit` は `exec adr-guard pre-commit --allow-status --allow-links` を実行する。
[ADR 0001](0001-worktree-cleanup.md) から [ADR 0004](0004-continue-after-removal-failure.md) のステータス行は、すべて `ステータス: 採択` の日本語の形（以下、旧形式）で書いてある。

adr-guard は v0.12.0、v0.13.0、v0.13.1 に更新された（[v0.12.0](https://github.com/tett23/adr-guard/releases/tag/v0.12.0)、[v0.13.0](https://github.com/tett23/adr-guard/releases/tag/v0.13.0)、[v0.13.1](https://github.com/tett23/adr-guard/releases/tag/v0.13.1) のリリース）。

v0.12.0 は、ステータス行の新しい形（以下、新形式）と、状態遷移の検査を加えた（tett23/adr-guard の [ADR 0022: 状態と遷移を別々の代数的データ型で表す](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0022-status-transitions.md)、[ADR 0023: trace で元の決定から修正ADRをたどる](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0023-trace-amendments.md)）。

- 新形式は `Status: <トークン>` で、トークンは `proposed`、`accepted`、`accepted-pending`、`rejected`、`discarded`、`deprecated`、または `superseded [<変更の JSON 配列>]` である。変更は `{"part":<変更した部分の名前か null>,"by":[<修正した ADR のリポジトリのルートからのパス>]}` の形で書く。
- 新しい ADR は、最初に `Status: proposed` でコミットしなければならない。追加した ADR と、ステータス行を変えた ADR には、新形式が要る。旧形式の行は、変えなければそのまま読める。
- pre-commit と `check-range` は、ステータス行の状態遷移（`proposed` から `accepted` へ、`accepted` から `superseded` へ、など）を検査する。この検査は、`--allow-status`、環境変数 `ADR_GUARD_ALLOW`、コミットのトレーラーでは免除されない。
- 旧形式を新形式に書き換える移行スクリプト `scripts/migrate-adr-status` が adr-guard のソースに加わった。リリースのアーカイブには入っていない。
- 元の決定から修正した ADR をたどる `adr-guard trace` が加わった。

v0.13.0 は、移行スクリプトに、書き換えずに差分だけを示す `--dry-run` を加えた。
移行スクリプトは移行できない ADR を最後まで数え、`移行できる: N件、移行済み: K件、移行できない: M件` を標準エラーに表示する（tett23/adr-guard の [ADR 0024: 移行スクリプトに --dry-run を設け、失敗を最後まで数える](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0024-migration-dry-run-and-error-count.md)）。
新形式には `superseded-pending`、変更の `kind` と `note`、`https://` で始まる修正先、`trace --list` の6列が加わった（tett23/adr-guard の [ADR 0025: 新形式のステータス行に、実装待ちの修正、補足、注記、別のリポジトリの修正先を加える](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0025-extend-canonical-status-amendments.md)）。
旧形式の修正先の読み方も広がった（tett23/adr-guard の [ADR 0026: 旧形式のステータス行の修正先を、リンク先と裸の番号からも読む](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0026-read-more-legacy-references.md)、[ADR 0027: 旧形式の `置換（…）` を、修正を受けた状態として読む](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0027-read-legacy-replaced-status.md)）。

v0.13.1 は、移行スクリプトが作る部分名の不具合を直した（tett23/adr-guard の [ADR 0028: 旧形式の部分名から修飾子と区切りの記号を除き、決められない部分名を移行しない](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0028-fix-legacy-part-names.md)）。
v0.13.0 の移行スクリプトは、部分名に修飾子（`ADR NNNN` の直前の語）、先頭の「。」、末尾の「→」を残すことがあった。
v0.13.1 はこれらを除き、部分名を決められない行は「部分名を決められません」と報告して書かない。

手元の adr-guard は v0.13.1 になっている。
手元の pre-commit は、新しい ADR を旧形式で書くと拒否し、コミット済みの ADR のステータス行を旧形式のまま書き換えても拒否する（「調査した事項」）。
CI と手元の版をそろえ、ADR が4件のうちに、すべてのステータス行を新形式に移す。

## 調査した事項

次のことを、手元で実行して確かめた。

- `adr-guard -V` は `adr-guard 0.13.1` を表示した。
- `git log -- .github/workflows/ci.yml` によれば、`tett23/adr-guard@v0.11.0` は 457482a（Initialize Rust project and continuous integration）で入り、その後に CI を変えたのは 2f24639 だけである。`docs/adr/` の4件の ADR は、adr-guard の版に触れていない。
- 移行スクリプトは、adr-guard の v0.13.1 のソースの `scripts/migrate-adr-status` を使った。このスクリプトは、`cargo run --example migrate_adr_status` で example の `migrate_adr_status` をビルドして実行する。以下では、ビルドした `migrate_adr_status` を直接実行した。
- このリポジトリのルート（origin/main の 93cbf6b）で `migrate_adr_status --dry-run` を実行すると、終了コード0で終わり、標準エラーの最後の行は `移行できる: 4件、移行済み: 0件、移行できない: 0件` だった。4件とも `Status: accepted` に移る。作業ツリーは書きかわらなかった。
  - この ADR の草稿を置いた作業ツリーでは、最後の行は `移行できる: 4件、移行済み: 1件、移行できない: 0件` だった。この ADR は新形式なので、移行済みに数えられる。
  - 差分の行は、v0.13.0（`0096636`）の移行スクリプトを同じコミットで実行したときと同じだった。
- 同じコミットの複製で `migrate_adr_status --write` を実行すると、4件の ADR のステータス行だけが書きかわった（`4 files changed, 4 insertions(+), 4 deletions(-)`）。その後の `--dry-run` は `移行できる: 0件、移行済み: 4件、移行できない: 0件` を表示した。
  - 書き換えた後の複製で、`adr-guard links`、`adr-guard check-status`、`adr-guard check-numbers` は終了コード0で終わった。`adr-guard unimplemented` は何も表示しなかった。
  - 書き換えをステージして `adr-guard pre-commit --allow-status --allow-links` は終了コード0で終わった。コミットした後、`adr-guard check-range HEAD~1 HEAD --allow-status --allow-links` も終了コード0で終わった。
- 移行していない複製（別のリポジトリ shorten-path で試した）で、手元の adr-guard（v0.13.1）の pre-commit は次を拒否した。
  - `ステータス: 提案` の新しい ADR。診断は `` 新形式のステータス行は `Status: ` で始めてください `` だった。
  - `Status: accepted` の新しい ADR。診断は `許可されていない状態遷移: Absent -> accepted` だった。
  - コミット済みの ADR の `ステータス: 採択` を、旧形式のまま `ステータス: 採択→…` に書き換えた変更。診断は `` 新形式のステータス行は `Status: ` で始めてください `` で、終了コードは1だった。
  - `Status: proposed` の新しい ADR は許可した。
- 実装の手順を、origin/main の複製で次のとおりに試した。どの段階でも pre-commit（`--allow-status --allow-links`）は終了コード0で終わった。
  1. この ADR を `Status: proposed` のまま足してコミットした。
  2. 移行スクリプトの `--write` を実行した。最後の行は `移行できる: 4件、移行済み: 1件、移行できない: 0件` で、この ADR は移行済みに数えられた。
  3. この ADR を `Status: accepted` にして、移行とあわせて一つのコミットにした。
  - その後、`adr-guard check-range <origin/main> HEAD --allow-status --allow-links`、`adr-guard links`、`adr-guard check-status`、`adr-guard check-numbers` は終了コード0で終わった。
- v0.13.1 の `action.yml` の入力の名前は、v0.11.0 と同じ（`base`、`head`、`version`、`allow-status`、`allow-links`、`links`、`check-status`、`check-numbers`、`allow-renumber`）である。

## 実装すること

- `.github/workflows/ci.yml` の `adr` ジョブの action を `uses: tett23/adr-guard@v0.13.1` にする。`base`、`head`、`allow-status: true`、`allow-links: true` の入力と、`actions/checkout` の設定はそのままとする。
- このリポジトリのルートで、adr-guard の v0.13.1 の移行スクリプトを `--dry-run` で実行し、`移行できない: 0件` で終わることを確かめてから、`--write` で全 ADR のステータス行を新形式に書き換える。adr-guard のソースから実行するときは `scripts/migrate-adr-status --dry-run` と `scripts/migrate-adr-status --write` である。書き換えの結果は手で直さない。
- `README.md` の Development の節に、adr-guard v0.13.1 以降を入れること、ADR のステータス行を `Status: <token>` の形で書くこと、新しい ADR を `Status: proposed` でコミットすること、pre-commit と CI が状態遷移を検査することを英語で書く。

## 実装しないこと

- `.githooks/pre-commit` の変更。コマンドとオプションは v0.13.1 でも変わらない。
- Codex の hook（`adr-guard codex-hook`）の導入。v0.11.0 にもあったが、このリポジトリでは使っていない。
- Claude Code の hook の変更。全体の設定（`~/.claude/settings.json`）にあり、このリポジトリでは管理しない。
- 過去の版の更新を記録した ADR のステータスの変更。そのような ADR はない。
- 移行スクリプトの結果を超えた、ステータス行の書き直し。
- ADR の本文と、ステータス行以外の行の変更。
- `docs/specifications.md` の変更。仕様書は worktree-cleaner の振る舞いを書いており、ADR の手順と adr-guard には触れていない。
- `changes` ジョブと `rust` ジョブの変更。

## 不変条件

なし。設定の値と移行の手順を決める ADR で、入力によって変わる振る舞いを足さない。

## テスト設計

設定と文書の変更なので、自動テストは書かない。実装の後に、次を確かめる。

- 書き換える前に移行スクリプトの `--dry-run` が終了コード0で終わり、`移行できない: 0件` を表示する。この ADR は新形式なので「移行済み」に数えられる。書き換えた後の `--dry-run` は `移行できる: 0件` を表示する。
- 手元の adr-guard（v0.13.1）で、`adr-guard links --fix` を実行したあと、`adr-guard links`、`adr-guard check-status`、`adr-guard check-numbers` が終了コード0で終わる。
- 変更をステージして `.githooks/pre-commit` が成功する。実装のコミットには、全 ADR の移行と、この ADR の `proposed` から `accepted` への変更が含まれる。
- コミット後、`adr-guard check-range origin/main HEAD --allow-status --allow-links` が終了コード0で終わる。
- `.github/workflows/ci.yml` が YAML として読み込める。
- 変更のプルリクエストの CI で、`adr` ジョブが v0.13.1 の action で実行され、成功する。ログで `adr-guard-v0.13.1-x86_64-unknown-linux-gnu.tar.gz` を取得していることを確かめる。`ci.yml` を変えるので、`rust` ジョブも実行される。

## トレードオフ

- ADR のステータス行が英語のトークンと JSON になり、日本語の形より読みにくい。代わりに、状態遷移の検査と `adr-guard trace` が使え、旧形式と新形式が混ざらない。
- 状態遷移の検査は `ADR_GUARD_ALLOW` とトレーラーで免除されないので、誤ったステータスを緊急に戻す手段が減る。
- 手元の adr-guard が v0.12.0 より古いと、新形式の行と状態遷移が手元で検査されず、CI で初めて失敗する。README に必要な版を書いて補う。

## 参考文献

- tett23/adr-guard [ADR 0022: 状態と遷移を別々の代数的データ型で表す](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0022-status-transitions.md)
- tett23/adr-guard [ADR 0023: trace で元の決定から修正ADRをたどる](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0023-trace-amendments.md)
- tett23/adr-guard [ADR 0024: 移行スクリプトに --dry-run を設け、失敗を最後まで数える](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0024-migration-dry-run-and-error-count.md)
- tett23/adr-guard [ADR 0025: 新形式のステータス行に、実装待ちの修正、補足、注記、別のリポジトリの修正先を加える](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0025-extend-canonical-status-amendments.md)
- tett23/adr-guard [ADR 0026: 旧形式のステータス行の修正先を、リンク先と裸の番号からも読む](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0026-read-more-legacy-references.md)
- tett23/adr-guard [ADR 0027: 旧形式の `置換（…）` を、修正を受けた状態として読む](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0027-read-legacy-replaced-status.md)
- tett23/adr-guard [ADR 0028: 旧形式の部分名から修飾子と区切りの記号を除き、決められない部分名を移行しない](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0028-fix-legacy-part-names.md)
- tett23/adr-guard [リリース v0.12.0](https://github.com/tett23/adr-guard/releases/tag/v0.12.0)
- tett23/adr-guard [リリース v0.13.0](https://github.com/tett23/adr-guard/releases/tag/v0.13.0)
- tett23/adr-guard [リリース v0.13.1](https://github.com/tett23/adr-guard/releases/tag/v0.13.1)
