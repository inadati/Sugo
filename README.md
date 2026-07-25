# Sugo

双六を作成する感覚で直感的にAIハーネスを設計・管理できるAIハーネスマネージャーアプリ。MCP経由でハーネスの作成・編集・削除・実行が可能。Nipperの拡張のために開発したアプリで、多様なハーネスを直感的に設計し、そのまま実行できる。

`Claude Code` `AIハーネス` `MCP` `個人開発` `macOS`

## コンセプト

AIエージェントに複雑な作業手順を踏ませたいとき、多くの場合その手順は「分岐・ループ・ガード・始端/終端を持つ状態遷移」として表現できる。Sugoはこれを、プログラムやYAMLではなく **双六盤を組み立てる感覚のGUI** で設計できるようにするデスクトップアプリである。

盤面上の「マス（cell）」ひとつひとつに登録プロンプトを紐づけ、マス同士を矢印（エッジ）でつなぐことでハーネス（＝AIエージェントの行動手順書）を組み立てる。できあがった盤面はそのままMCP経由で実行でき、Claude Codeエージェントが盤面をたどりながらターンを進めていく。

## アーキテクチャ

Sugoは単体では動かず、姉妹アプリ **Nipper**（Claude CodeをPTY起動するチャットUIアプリ）と連携して動作する。

- **Sugo**: ハーネス（盤面）の作成・編集・永続化・進行制御を担当。GUIでのマス編集とMCPツール群を提供する
- **Nipper**: Claude Codeをターミナル経由で起動し、チャットUIとして提供。ハーネスの「ターン」を実際に駆動する
- **Claude Code**: マスに登録されたプロンプトに従って実作業を行う頭脳。マスの著作や整合性修正も担う

Sugoは自身ではPTYを持たないため、`sugo_advance` というMCP呼び出しと、Claude Codeプロジェクトのjsonlログ監視を組み合わせることで、システム側から確実に進行を制御している。Nipperが公開するローカルHTTPの注入APIを通じて、次のマスのプロンプトをClaude Codeに渡す仕組みになっている。

## 技術スタック

- **バックエンド**: Rust（Tauri 2、`rusqlite`によるSQLite永続化、`rmcp`によるMCPサーバー実装）
- **フロントエンド**: Vue 3 + TypeScript（Vite）、盤面描画に Cytoscape.js（`cytoscape-dagre` / `cytoscape-edgehandles`）
- **構成**: Rust workspace（`sugo-core` / `sugo-infra` / `sugo-mcp` / `sugo-gui/src-tauri`）

盤面の定義（マスとエッジの構造）はバージョンごとに不変なJSONドキュメントとしてDBに保存し、実行状態・イベント・レジストリなどの可変データは正規化テーブルで管理するハイブリッド設計を採用している。詳細な仕様は [SPEC.md](./SPEC.md) を参照。

## MCPツール

Sugoが公開する主なMCPツールは以下の通り。

| ツール | 役割 |
| --- | --- |
| `sugo_create_harness` | 新規ハーネス（盤面）の作成 |
| `sugo_edit_cell` | マスのプロンプト編集 |
| `sugo_validate_harness` | 整合性検証（エッジ・到達性・終端の破綻検出） |
| `sugo_update_harness` | 整合性修正の反映 |
| `sugo_start` | ハーネス実行の開始 |
| `sugo_status` | 現在の進行状況・差分の取得 |
| `sugo_advance` | 次のマスへの進行 |
| `sugo_delete_harness` | ハーネスの削除 |

## 開発

```bash
cd sugo-gui
pnpm install
pnpm tauri dev
```
