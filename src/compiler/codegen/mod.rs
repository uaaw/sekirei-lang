/// sekirei Code Generator
///
/// パイプライン:
///   AST → LLVM IR テキスト (.ll)  ← irgen.rs (常に利用可)
///       → clang/llc でコンパイル  ← 外部ツール呼び出し
///
/// `--features codegen` を付けると inkwell (JIT) も使える。

pub mod irgen;

use std::path::{Path, PathBuf};
use std::process::Command;
use crate::parser::TopLevel;

#[derive(Debug)]
pub struct CodegenError {
    pub message: String,
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[Codegen Error] {}", self.message)
    }
}

pub struct Codegen;

impl Codegen {
    pub fn new() -> Self { Self }

    // --------------------------------------------------------
    // AOT: .sk → .ll → binary
    // --------------------------------------------------------

    pub fn build(&self, ast: &[TopLevel], output: &Path) -> Result<(), CodegenError> {
        let ll_path = output.with_extension("ll");
        let ir = self.emit_ir(ast);
        std::fs::write(&ll_path, &ir).map_err(|e| CodegenError {
            message: format!("cannot write IR: {}", e),
        })?;

        println!("[codegen] wrote {}", ll_path.display());

        self.compile_ll(&ll_path, output)?;
        Ok(())
    }

    // --------------------------------------------------------
    // JIT: .sk → .ll → 一時バイナリ → 実行
    // --------------------------------------------------------

    pub fn run(&self, ast: &[TopLevel]) -> Result<i32, CodegenError> {
        let tmp_dir  = std::env::temp_dir();
        let ll_path  = tmp_dir.join("sekirei_tmp.ll");
        let bin_path = tmp_dir.join("sekirei_tmp");

        let ir = self.emit_ir(ast);
        std::fs::write(&ll_path, &ir).map_err(|e| CodegenError {
            message: format!("cannot write temp IR: {}", e),
        })?;

        self.compile_ll(&ll_path, &bin_path)?;

        let status = Command::new(&bin_path)
            .status()
            .map_err(|e| CodegenError { message: format!("cannot run binary: {}", e) })?;

        Ok(status.code().unwrap_or(1))
    }

    // --------------------------------------------------------
    // IR テキスト生成
    // --------------------------------------------------------

    pub fn emit_ir(&self, ast: &[TopLevel]) -> String {
        let mut gen = irgen::IrGen::new();
        gen.emit_program(ast)
    }

    // --------------------------------------------------------
    // .ll → native binary (clang または llc + gcc)
    // --------------------------------------------------------

    fn compile_ll(&self, ll: &Path, output: &Path) -> Result<(), CodegenError> {
        // まず clang を試す
        if self.try_clang(ll, output).is_ok() {
            return Ok(());
        }
        // 次に llc + gcc
        self.try_llc_gcc(ll, output)
    }

    fn try_clang(&self, ll: &Path, output: &Path) -> Result<(), CodegenError> {
        let libs = self.find_libs()?;

        let mut args = vec![ll.to_str().unwrap().to_string()];
        args.extend(libs);
        args.extend(["-o".into(), output.to_str().unwrap().to_string(), "-lm".into()]);

        let status = Command::new("clang")
            .args(&args)
            .status()
            .map_err(|_| CodegenError { message: "clang not found".into() })?;

        if status.success() {
            println!("[codegen] compiled via clang → {}", output.display());
            Ok(())
        } else {
            Err(CodegenError { message: "clang compilation failed".into() })
        }
    }

    fn try_llc_gcc(&self, ll: &Path, output: &Path) -> Result<(), CodegenError> {
        let asm = ll.with_extension("s");

        let llc = Command::new("llc")
            .args([ll.to_str().unwrap(), "-o", asm.to_str().unwrap()])
            .status()
            .map_err(|_| CodegenError { message: "llc not found. install LLVM to compile .sk files".into() })?;

        if !llc.success() {
            return Err(CodegenError { message: "llc failed".into() });
        }

        let libs = self.find_libs()?;
        let mut args = vec![asm.to_str().unwrap().to_string()];
        args.extend(libs);
        args.extend(["-o".into(), output.to_str().unwrap().to_string(), "-lm".into()]);

        let gcc = Command::new("gcc")
            .args(&args)
            .status()
            .map_err(|e| CodegenError { message: format!("gcc failed: {}", e) })?;

        if gcc.success() {
            println!("[codegen] compiled via llc+gcc → {}", output.display());
            Ok(())
        } else {
            Err(CodegenError { message: "gcc linking failed".into() })
        }
    }

    /// cargo build で生成されたライブラリファイル群を返す (runtime, stdlib, asm)
    fn find_libs(&self) -> Result<Vec<String>, CodegenError> {
        let lib_names = ["libsekirei_runtime.a", "libsekirei_stdlib.a", "libsekirei_asm.a"];
        let out_dir_pat = "target/debug/build/sekirei-*/out/libsekirei_runtime.a";

        // out ディレクトリを特定
        let out_dir = glob::glob(out_dir_pat)
            .ok()
            .and_then(|mut it| it.next())
            .and_then(|r| r.ok())
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .ok_or_else(|| CodegenError {
                message: "runtime library not found. run 'cargo build' first".into(),
            })?;

        let mut result = Vec::new();
        for name in &lib_names {
            let p = out_dir.join(name);
            if p.exists() {
                result.push(p.to_string_lossy().into_owned());
            }
        }

        if result.is_empty() {
            return Err(CodegenError { message: "no sekirei libraries found".into() });
        }
        Ok(result)
    }
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn ir(src: &str) -> String {
        let tokens = Lexer::new(src).tokenize().expect("lex");
        let ast    = Parser::new(tokens).parse().expect("parse");
        Codegen::new().emit_ir(&ast)
    }

    #[test]
    fn test_emit_hello() {
        let src = "fn main():\n    println(\"Hello, sekirei!\")\n";
        let out = ir(src);
        assert!(out.contains("sk_user_main"), "main should become sk_user_main");
        assert!(out.contains("sk_println"),   "println should map to sk_println");
        assert!(out.contains("Hello, sekirei!"), "string literal should be in IR");
    }

    #[test]
    fn test_emit_arithmetic() {
        let src = "fn add(x: int, y: int) -> int:\n    return x + y\n";
        let out = ir(src);
        assert!(out.contains("define i64 @add"), "fn add should be defined");
        assert!(out.contains("add i64"), "addition instruction");
    }

    #[test]
    fn test_emit_if() {
        let src = "fn abs(x: int) -> int:\n    if x < 0:\n        return -x\n    else:\n        return x\n";
        let out = ir(src);
        assert!(out.contains("icmp slt"), "less-than comparison");
        assert!(out.contains("br i1"),    "conditional branch");
    }

    #[test]
    fn test_emit_for_range() {
        let src = "fn sum() -> int:\n    mut s = 0\n    for i in 0..10:\n        s = s + i\n    return s\n";
        let out = ir(src);
        assert!(out.contains("icmp slt"), "range bound check");
        assert!(out.contains("add i64"),  "accumulation");
    }

    #[test]
    fn test_emit_while() {
        let src = "fn countdown(n: int):\n    mut x = n\n    while x > 0:\n        x = x - 1\n";
        let out = ir(src);
        assert!(out.contains("icmp sgt"), "while condition");
        assert!(out.contains("br i1"),    "loop branch");
    }

    #[test]
    fn test_string_literal_in_global() {
        let src = "fn main():\n    let s = \"hello\"\n";
        let out = ir(src);
        assert!(out.contains("private unnamed_addr constant"), "string global");
        assert!(out.contains("hello"),                         "string value");
    }
}
