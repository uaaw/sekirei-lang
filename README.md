# sekirei

プログラミング言語。

```
fn main():
    println("こんにちは、sekirei!")
```

## 概要

sekirei（拡張子 `.sk`）は「Python の読みやすさ + 静的型付けの安全性」をコンセプトに設計された言語です。`.sk` ファイルを LLVM IR に変換し、`clang` または `llc + gcc` を使ってネイティブバイナリを生成します。メモリ管理はマーク・アンド・スイープ GC が自動で行います。

コンパイラ本体は Rust で実装されています。ランタイムと標準ライブラリは C / C++ で、低レベルプリミティブ（アトミック操作・高速メモリ操作）は AArch64 アセンブリで実装されています。

## インストール

**必要環境:**

- Rust（edition 2021、cargo）
- clang（推奨）または llc + gcc
- gcc / g++（ランタイムのビルドに必要）

**ソースからビルド:**

```sh
git clone https://github.com/sekirei-lang/sekirei
cd sekirei
cargo build --release
```

ビルドで生成される実行ファイル:

| バイナリ | 用途 |
|----------|------|
| `sekirei` | コンパイラ・実行ツール |
| `skp` | パッケージマネージャ |

## クイックスタート

**ファイルを実行（JIT 風）:**

```sh
sekirei run hello.sk
```

**バイナリにコンパイル:**

```sh
sekirei build hello.sk -o hello
./hello
```

**LLVM IR を確認:**

```sh
sekirei emit-ir hello.sk
```

**新規プロジェクトを作成:**

```sh
skp init myproject
cd myproject
sekirei run src/main.sk
```

## 言語リファレンス

### 変数

```
let x = 42          # イミュータブル（変更不可）
mut count = 0       # ミュータブル（変更可能）
let pi: float = 3.14
```

型アノテーションは省略可能です。コンパイラがコンテキストから型を推論します。

### 関数

```
fn add(x: int, y: int) -> int:
    return x + y

fn greet(name: string):
    println("Hello, " + name + "!")
```

### 制御フロー

```
# if / elif / else
fn classify(n: int) -> string:
    if n > 0:
        return "正の数"
    elif n < 0:
        return "負の数"
    else:
        return "ゼロ"

# while ループ
mut i = 0
while i < 10:
    i = i + 1

# for ループ（範囲）
for i in 0..10:
    println("i = " + str(i))

# 閉区間レンジ
for i in 0..=5:
    println(str(i))
```

### パターンマッチング

```
match value:
    0 => println("ゼロ")
    1 => println("いち")
    _ => println("その他")
```

### 構造体とメソッド

```
struct Point:
    x: float
    y: float

impl Point:
    fn distance(self) -> float:
        return math.sqrt(self.x * self.x + self.y * self.y)
```

### ラムダ式

```
let double = |x: int| -> int: x * 2
let result = double(21)
```

### エラーハンドリング

```
let value = risky_operation()?   # エラーを伝播

result.catch |e|:
    println("エラー: " + str(e))
```

### インポート

```
from std import io              # 標準ライブラリ
from std import math
from skp import http            # サードパーティパッケージ（sekipi.org）
from ./utils import helpers     # ローカルファイル
```

### 型一覧

| sekirei の型 | 説明 |
|---|---|
| `int` | 64ビット符号付き整数 |
| `i8` `i16` `i32` `i64` | 幅を明示した整数型 |
| `uint` `u8` `u16` `u32` `u64` | 符号なし整数型 |
| `float` | 64ビット浮動小数点数 |
| `f32` `f64` | 幅を明示した浮動小数点型 |
| `string` | UTF-8 文字列 |
| `bool` | `true` / `false` |
| `void` | 戻り値なし |
| `T?` | Nullable 型 |
| `Option<T>` | 省略可能な値（`Some` / `None`） |
| `Result<T, E>` | 成功または失敗（`Ok` / `Err`） |
| `[T]` | リスト |
| `{K: V}` | 辞書 |

### 組み込み関数

| 関数 | 説明 |
|---|---|
| `print(s)` | 改行なしで出力 |
| `println(s)` | 改行ありで出力 |
| `read_line()` | 標準入力から1行読む |
| `len(x)` | 文字列またはリストの長さ |
| `str(x)` | 文字列に変換 |
| `int(x)` | 整数に変換 |
| `float(x)` | 浮動小数点数に変換 |

### 標準ライブラリ

```
from std import math

math.sqrt(x)    # 平方根
math.pow(x, y)  # べき乗
math.abs(x)     # 絶対値
math.floor(x)   # 床関数
math.ceil(x)    # 天井関数
math.sin(x)     # 正弦
math.cos(x)     # 余弦
math.log(x)     # 自然対数
```

```
from std import io

io.println("hello")
io.read_line()
```
