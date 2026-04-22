use sekirei::manifest;

use std::path::{Path, PathBuf};
use std::process;

fn usage() {
    eprintln!("skp - sekirei package manager");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  skp init [name]          新規プロジェクトを作成");
    eprintln!("  skp install [package]    パッケージをインストール (未実装)");
    eprintln!("  skp publish              パッケージを sekipi.org に公開 (未実装)");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        usage();
        process::exit(1);
    }

    match args[1].as_str() {
        "init"    => cmd_init(&args[2..]),
        "install" => cmd_install(&args[2..]),
        "publish" => cmd_publish(),
        cmd => {
            eprintln!("error: unknown command '{}'", cmd);
            usage();
            process::exit(1);
        }
    }
}

// ---- skp init ----

fn cmd_init(args: &[String]) {
    let name = args.first().map(String::as_str).unwrap_or("myproject");

    // プロジェクトディレクトリを作成
    let project_dir = PathBuf::from(name);
    let src_dir     = project_dir.join("src");

    if project_dir.exists() {
        eprintln!("error: directory '{}' already exists", name);
        process::exit(1);
    }

    std::fs::create_dir_all(&src_dir).unwrap_or_else(|e| {
        eprintln!("error: cannot create directory: {}", e);
        process::exit(1);
    });

    // sekirei.toml を生成
    let toml_content = manifest::default_toml(name);
    write_file(&project_dir.join("sekirei.toml"), &toml_content);

    // src/main.sk を生成
    let main_sk = manifest::default_main_sk(name);
    write_file(&src_dir.join("main.sk"), &main_sk);

    println!("✓ Created project '{}'", name);
    println!();
    println!("  {}/", name);
    println!("  ├── sekirei.toml");
    println!("  └── src/");
    println!("      └── main.sk");
    println!();
    println!("Run with:");
    println!("  cd {} && sekirei run src/main.sk", name);
}

fn write_file(path: &Path, content: &str) {
    std::fs::write(path, content).unwrap_or_else(|e| {
        eprintln!("error: cannot write '{}': {}", path.display(), e);
        process::exit(1);
    });
}

// ---- skp install (未実装) ----

fn cmd_install(args: &[String]) {
    if args.is_empty() {
        // sekirei.toml の dependencies を全部インストール
        println!("[skp] installing dependencies from sekirei.toml...");
        println!("[skp] (not yet implemented - sekipi.org registry coming soon)");
    } else {
        for pkg in args {
            println!("[skp] installing '{}'...", pkg);
            println!("[skp] (not yet implemented - sekipi.org registry coming soon)");
        }
    }
}

// ---- skp publish (未実装) ----

fn cmd_publish() {
    println!("[skp] publishing to sekipi.org...");
    println!("[skp] (not yet implemented - sekipi.org registry coming soon)");
}
