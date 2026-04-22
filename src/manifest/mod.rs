/// sekirei.toml パーサー

use std::collections::HashMap;
use std::path::Path;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SekireiToml {
    pub package:      Package,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name:    String,
    pub version: String,
    pub author:  Option<String>,
    /// エントリポイント (デフォルト: "src/main.sk")
    #[serde(default = "default_entry")]
    pub entry:   String,
}

fn default_entry() -> String {
    "src/main.sk".to_string()
}

impl SekireiToml {
    pub fn load(dir: &Path) -> Result<Self, ManifestError> {
        let path = dir.join("sekirei.toml");
        let content = std::fs::read_to_string(&path)
            .map_err(|e| ManifestError::Io(path.display().to_string(), e))?;
        toml::from_str(&content)
            .map_err(|e| ManifestError::Parse(e.to_string()))
    }

    pub fn entry_path(&self, base: &Path) -> std::path::PathBuf {
        base.join(&self.package.entry)
    }
}

/// sekirei.toml のデフォルトテンプレートを生成
pub fn default_toml(name: &str) -> String {
    format!(
r#"[package]
name    = "{name}"
version = "0.1.0"
entry   = "src/main.sk"

[dependencies]
"#
    )
}

/// src/main.sk のデフォルトテンプレート
pub fn default_main_sk(name: &str) -> String {
    format!(
r#"from std import io

fn main():
    io.println("Hello from {name}!")
"#
    )
}

#[derive(Debug)]
pub enum ManifestError {
    Io(String, std::io::Error),
    Parse(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(path, e) => write!(f, "cannot read '{}': {}", path, e),
            ManifestError::Parse(e)    => write!(f, "invalid sekirei.toml: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toml() {
        let src = r#"
[package]
name    = "myapp"
version = "0.1.0"
entry   = "src/main.sk"

[dependencies]
http = "1.2.0"
"#;
        let manifest: SekireiToml = toml::from_str(src).unwrap();
        assert_eq!(manifest.package.name, "myapp");
        assert_eq!(manifest.package.entry, "src/main.sk");
        assert_eq!(manifest.dependencies["http"], "1.2.0");
    }

    #[test]
    fn test_default_entry() {
        let src = r#"
[package]
name    = "minimal"
version = "0.1.0"
"#;
        let manifest: SekireiToml = toml::from_str(src).unwrap();
        assert_eq!(manifest.package.entry, "src/main.sk");
    }

    #[test]
    fn test_default_toml_format() {
        let t = default_toml("myapp");
        assert!(t.contains("myapp"));
        assert!(t.contains("[dependencies]"));
    }
}
