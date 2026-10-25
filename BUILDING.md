# ビルドと検証

Rust 1.95.0、Python 3.11以降、`flip-link` 0.1.12を使用します。CoreX Studioは別リポジトリで管理し、ファームのビルドには不要です。

```sh
rustup toolchain install 1.95.0 --profile minimal
rustup target add thumbv7em-none-eabihf --toolchain 1.95.0
rustup component add llvm-tools-preview --toolchain 1.95.0
cargo install flip-link --version 0.1.12 --locked
./build.sh all
python3 tools/package_release.py
./tools/test.sh
```

初回はCargo依存関係のダウンロードが必要です。Cコンパイラ、pkg-config、libclangに加えて、暗号ライブラリのビルドにArm組み込み向けGCC（`arm-none-eabi-gcc`）が必要です。Ubuntuでは次を先にインストールします。

```sh
sudo apt-get install build-essential pkg-config libclang-dev gcc-arm-none-eabi libnewlib-arm-none-eabi
```

Vial定義の圧縮に使うliblzmaはソースから静的にビルドし、ホストと異なるCPU用のシステムライブラリを誤ってリンクしないようにしています。

## 対象を選ぶ

```sh
./build.sh right s140v6
./build.sh left s140v6
./build.sh stock-left
```

第1引数は `right`・`left`・`stock-left`・`all`、第2引数は `s140v6`・`s140v7`・`both` です。省略時は右側の両SoftDevice版を生成します。純正左は専用の開始アドレスを使い、第2引数の影響を受けません。

出力は `build/firmware/` です。`package_release.py` は5種類すべてのビルドを検証してからUF2・チェックサム・manifestを `firmware/` にコピーします。書き込み可能範囲、割り込みベクタ、圧縮Vial定義、設定スキーマを照合します。設定ツール用の識別情報 `configuration_profile` も、コンパイル済み定義とファームの行列から生成します。PCに接続された本体への書き込みは行いません。

通常はリポジトリ内のキャッシュを使います。`CARGO_TARGET_DIR` を指定してビルドした場合、配布検証には標準の `build/target/` に揃えて再ビルドしてください。

## 互換性

CoreX右のVial UIDは `436f726558525931`、v0.11.4のファーム識別値は `2820`、設定スキーマは `0xA805CEFB` です。この更新ではUIDや設定領域を変更しません。Vial定義には実基板に合わせた5セルの位置補正があり、Studioも同じ対応表で読み書きします。

`tools/git-metadata/git` は、保存互換性に影響するRMKの基準commit識別値を固定します。実ソースは同梱の修正版です。詳細は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) を参照してください。

内部の `raytac-r-a14`・`raytac-l-rev1` は基板用ビルドfeatureです。利用者向けの名称やBLE名ではありません。`keyboard.toml` はStudioの共通座標と純正左の基準も兼ねるため残しています。CoreX用GPIO・行列は `keyboard-raytac.toml` を選択します。ルートの `build.sh` を使い、右をfeatureなしで直接ビルドしないでください。

## 左右登録の実装

v0.11.4では、未登録の相手を探索する受付を明示的な操作から60秒間だけ開きます。電源ONやRESETは受付を開きません。保存済みの相手への再接続は従来どおり行います。

右の固定キー判定は `right/src/pairing_keys.rs` と `pairing_keys_logic.rs`、左の固定キー判定はRMKの `split/ble/input.rs` が担当します。右のスキャン位置は `[1,5]` と `[3,4]`、Studio/Vial上の表示位置では `[1,5]` と `[3,3]` です。初期配列のHとSpace隣の親指キーに当たり、キーコードやレイヤーの割り当てには依存しません。左はローカル位置 `[0,0]` と `[0,5]` のTab・Tです。

右の長押し成立後は、2キーを離した際の再起動・ブートローダ移行・全設定初期化・User操作を抑止します。通常のキーと修飾キーのreleaseは処理します。長押し成立前のキー入力や、押した時点で実行済みの任意マクロを取り消す仕組みではありません。

受付・保存・入力準備の状態を左右通信からLEDへ通知します。受付中は2灯とも青く点滅、登録と保存の完了後は緑に2秒点灯、期限切れ・登録失敗時は黄色く3回点滅して消灯します。LEDの青い点滅は独立した固定回数の演出ではなく、実際の受付状態に追従します。

## テストの範囲

`tools/test.sh` はポインター、ジェスチャー、左右通信、設定処理などのホストテストと配布UF2の検証を実行します。共有状態のあるRMKテストは1テストずつ別プロセスで実行します。固定キーの5秒判定、追加キーによる取消し、受付の期限、登録・保存結果とLEDの状態遷移も検証します。CIでは別ジョブで5種類の実ファームもビルドします。実機での電源再投入、Bluetooth再接続、入力、電池消費の確認は別途必要です。
