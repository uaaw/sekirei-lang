mod lexer;
mod parser;
mod sema;
mod codegen;
use sekirei::manifest;

use std::path::Path;
use std::process;

fn usage() {
    eprintln!("Usage:");
    eprintln!("  sekirei run <file.sk>               JITコンパイルして実行");
    eprintln!("  sekirei build <file.sk> [-o <out>]  バイナリにコンパイル");
    eprintln!("  sekirei emit-ir <file.sk>            LLVM IRを標準出力に表示");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        usage();
        process::exit(1);
    }

    let subcmd = &args[1];
    let file   = Path::new(&args[2]);

    let src = std::fs::read_to_string(file).unwrap_or_else(|e| {
        eprintln!("error: cannot read '{}': {}", file.display(), e);
        process::exit(1);
    });

    let tokens = lexer::Lexer::new(&src).tokenize().unwrap_or_else(|e| {
        eprintln!("{}", e);
        process::exit(1);
    });

    let ast = parser::Parser::new(tokens).parse().unwrap_or_else(|e| {
        eprintln!("{}", e);
        process::exit(1);
    });

    let mut sema = sema::SemanticAnalyzer::new();
    if let Err(e) = sema.analyze(&ast) {
        eprintln!("{}", e);
        process::exit(1);
    }

    let cg = codegen::Codegen::new();

    match subcmd.as_str() {
        "run" => {
            let code = cg.run(&ast).unwrap_or_else(|e| {
                eprintln!("{}", e);
                process::exit(1);
            });
            process::exit(code);
        }
        "build" => {
            let output = if args.len() >= 5 && args[3] == "-o" {
                args[4].clone()
            } else {
                file.with_extension("").to_string_lossy().into_owned()
            };
            cg.build(&ast, Path::new(&output)).unwrap_or_else(|e| {
                eprintln!("{}", e);
                process::exit(1);
            });
        }
        "emit-ir" => {
            print!("{}", cg.emit_ir(&ast));
        }
        _ => {
            eprintln!("error: unknown subcommand '{}'", subcmd);
            usage();
            process::exit(1);
        }
    }
}
