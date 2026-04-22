/// sekirei Lexer
/// 入力ソースコードをトークン列に変換する

use std::collections::VecDeque;

// ---- トークン定義 ----

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // リテラル
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,

    // 識別子
    Ident(String),

    // キーワード
    Fn,
    Let,
    Mut,
    Struct,
    Impl,
    Return,
    If,
    Elif,
    Else,
    Match,
    For,
    While,
    Loop,
    In,
    Break,
    Continue,
    Import,
    From,
    As,
    Try,
    Catch,
    SomeKw,   // Some(...)
    NoneKw,   // None
    OkKw,     // Ok(...)
    ErrKw,    // Err(...)

    // 型キーワード
    TInt,
    TI8, TI16, TI32, TI64,
    TUint,
    TU8, TU16, TU32, TU64,
    TFloat,
    TF32, TF64,
    TString,
    TStr,
    TBool,
    TChar,
    TByte,
    TVoid,

    // 演算子
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    Percent,    // %
    Eq,         // =
    EqEq,       // ==
    NotEq,      // !=
    Lt,         // <
    LtEq,       // <=
    Gt,         // >
    GtEq,       // >=
    And,        // &&
    Or,         // ||
    Not,        // !
    Pipe,       // |
    Arrow,      // ->
    FatArrow,   // =>
    Question,   // ?
    Colon,      // :
    Dot,        // .
    DotDot,     // ..
    DotDotEq,   // ..=
    Comma,      // ,
    Semicolon,  // ;
    LParen,     // (
    RParen,     // )
    LBrace,     // {
    RBrace,     // }
    LBracket,   // [
    RBracket,   // ]

    // インデント制御
    Newline,
    Indent,
    Dedent,
    Eof,
}

// ---- スパン（ソース位置） ----

#[derive(Debug, Clone)]
pub struct Span {
    pub line: usize,
    pub col:  usize,
}

#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub span:  Span,
}

// ---- エラー ----

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub line:    usize,
    pub col:     usize,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[Lex Error] {} (line {}, col {})", self.message, self.line, self.col)
    }
}

// ---- Lexer ----

pub struct Lexer {
    source:        Vec<char>,
    pos:           usize,
    line:          usize,
    col:           usize,
    indents:       Vec<usize>,      // インデントスタック
    pending:       VecDeque<Spanned>, // 出しきれていないDEDENT
    at_line_start: bool,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source:        source.chars().collect(),
            pos:           0,
            line:          1,
            col:           0,
            indents:       vec![0],
            pending:       VecDeque::new(),
            at_line_start: true,
        }
    }

    // ---- 基本操作 ----

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.source.get(self.pos).copied();
        if let Some(ch) = c {
            self.pos += 1;
            if ch == '\n' {
                self.line += 1;
                self.col = 0;
            } else {
                self.col += 1;
            }
        }
        c
    }

    fn span(&self) -> Span {
        Span { line: self.line, col: self.col }
    }

    fn err(&self, msg: &str) -> LexError {
        LexError { message: msg.into(), line: self.line, col: self.col }
    }

    // ---- インデント計測 ----

    /// 行頭の空白を消費してインデントレベルを返す
    fn measure_indent(&mut self) -> usize {
        let mut level = 0usize;
        loop {
            match self.peek() {
                Some(' ')  => { level += 1; self.advance(); }
                Some('\t') => { level += 4; self.advance(); } // タブ = 4スペース
                _          => break,
            }
        }
        level
    }

    // ---- コメントスキップ ----

    fn skip_comment(&mut self) {
        while !matches!(self.peek(), Some('\n') | None) {
            self.advance();
        }
    }

    // ---- 文字列リテラル ----

    fn read_string(&mut self, quote: char) -> Result<Token, LexError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(self.err("unterminated string literal")),
                Some(c) if c == quote => break,
                Some('\\') => {
                    let esc = self.advance().ok_or_else(|| self.err("unterminated escape sequence"))?;
                    match esc {
                        'n'  => s.push('\n'),
                        't'  => s.push('\t'),
                        'r'  => s.push('\r'),
                        '\\' => s.push('\\'),
                        '"'  => s.push('"'),
                        '\'' => s.push('\''),
                        '0'  => s.push('\0'),
                        c    => { s.push('\\'); s.push(c); }
                    }
                }
                Some(c) => s.push(c),
            }
        }
        Ok(Token::Str(s))
    }

    // ---- 数値リテラル ----

    fn read_number(&mut self, first: char) -> Token {
        let mut s = String::from(first);
        let mut is_float = false;

        loop {
            match self.peek() {
                Some(c) if c.is_ascii_digit() => {
                    s.push(c);
                    self.advance();
                }
                Some('.') if !is_float => {
                    // 次が数字なら小数点
                    if self.peek_next().map_or(false, |n| n.is_ascii_digit()) {
                        is_float = true;
                        s.push('.');
                        self.advance();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        if is_float {
            Token::Float(s.parse().unwrap_or(0.0))
        } else {
            Token::Int(s.parse().unwrap_or(0))
        }
    }

    // ---- 識別子・キーワード ----

    fn read_ident(&mut self, first: char) -> Token {
        let mut s = String::from(first);
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            s.push(self.advance().unwrap());
        }

        match s.as_str() {
            "fn"       => Token::Fn,
            "let"      => Token::Let,
            "mut"      => Token::Mut,
            "struct"   => Token::Struct,
            "impl"     => Token::Impl,
            "return"   => Token::Return,
            "if"       => Token::If,
            "elif"     => Token::Elif,
            "else"     => Token::Else,
            "match"    => Token::Match,
            "for"      => Token::For,
            "while"    => Token::While,
            "loop"     => Token::Loop,
            "in"       => Token::In,
            "break"    => Token::Break,
            "continue" => Token::Continue,
            "import"   => Token::Import,
            "from"     => Token::From,
            "as"       => Token::As,
            "try"      => Token::Try,
            "catch"    => Token::Catch,
            "Some"     => Token::SomeKw,
            "None"     => Token::NoneKw,
            "Ok"       => Token::OkKw,
            "Err"      => Token::ErrKw,
            "true"     => Token::Bool(true),
            "false"    => Token::Bool(false),
            "null"     => Token::Null,
            // 型名
            "int"      => Token::TInt,
            "i8"       => Token::TI8,
            "i16"      => Token::TI16,
            "i32"      => Token::TI32,
            "i64"      => Token::TI64,
            "uint"     => Token::TUint,
            "u8"       => Token::TU8,
            "u16"      => Token::TU16,
            "u32"      => Token::TU32,
            "u64"      => Token::TU64,
            "float"    => Token::TFloat,
            "f32"      => Token::TF32,
            "f64"      => Token::TF64,
            "string"   => Token::TString,
            "str"      => Token::TStr,
            "bool"     => Token::TBool,
            "char"     => Token::TChar,
            "byte"     => Token::TByte,
            "void"     => Token::TVoid,
            _          => Token::Ident(s),
        }
    }

    // ---- 記号・演算子 ----

    fn read_symbol(&mut self, c: char) -> Result<Option<Token>, LexError> {
        let tok = match c {
            '+' => Token::Plus,
            '-' => {
                if self.peek() == Some('>') { self.advance(); Token::Arrow }
                else { Token::Minus }
            }
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Percent,
            '=' => {
                if self.peek() == Some('=') { self.advance(); Token::EqEq }
                else if self.peek() == Some('>') { self.advance(); Token::FatArrow }
                else { Token::Eq }
            }
            '!' => {
                if self.peek() == Some('=') { self.advance(); Token::NotEq }
                else { Token::Not }
            }
            '<' => {
                if self.peek() == Some('=') { self.advance(); Token::LtEq }
                else { Token::Lt }
            }
            '>' => {
                if self.peek() == Some('=') { self.advance(); Token::GtEq }
                else { Token::Gt }
            }
            '&' => {
                if self.peek() == Some('&') { self.advance(); Token::And }
                else { return Err(self.err("single '&' is not supported, use '&&'")); }
            }
            '|' => {
                if self.peek() == Some('|') { self.advance(); Token::Or }
                else { Token::Pipe }
            }
            '?' => Token::Question,
            ':' => Token::Colon,
            '.' => {
                if self.peek() == Some('.') {
                    self.advance();
                    if self.peek() == Some('=') { self.advance(); Token::DotDotEq }
                    else { Token::DotDot }
                } else {
                    Token::Dot
                }
            }
            ',' => Token::Comma,
            ';' => Token::Semicolon,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            '\r' => return Ok(None), // Windows CRLF対応
            c   => return Err(self.err(&format!("unexpected character: '{}'", c))),
        };
        Ok(Some(tok))
    }

    // ---- メイン：トークナイズ ----

    pub fn tokenize(&mut self) -> Result<Vec<Spanned>, LexError> {
        let mut tokens = Vec::new();

        'outer: loop {
            // pending DEDENTを先に出す
            while let Some(t) = self.pending.pop_front() {
                tokens.push(t);
            }

            // 行頭インデント処理
            if self.at_line_start {
                self.at_line_start = false;

                // 空行 / コメント行はインデント処理をスキップ
                let save_pos  = self.pos;
                let save_line = self.line;
                let save_col  = self.col;
                let level = self.measure_indent();

                match self.peek() {
                    Some('\n') | Some('#') => {
                        // 空行またはコメント行 → インデント計測を巻き戻してスキップ
                        self.pos  = save_pos;
                        self.line = save_line;
                        self.col  = save_col;
                        self.advance(); // '\n' または '#' を消費
                        if self.source.get(save_pos).copied() == Some('#') {
                            self.skip_comment();
                        }
                        self.at_line_start = true;
                        continue 'outer;
                    }
                    None => {
                        // EOF: 残りDEDENTを全部出す
                        let span = self.span();
                        while self.indents.len() > 1 {
                            self.indents.pop();
                            tokens.push(Spanned { token: Token::Dedent, span: span.clone() });
                        }
                        tokens.push(Spanned { token: Token::Eof, span });
                        break 'outer;
                    }
                    _ => {}
                }

                let span = self.span();
                let cur   = *self.indents.last().unwrap();

                if level > cur {
                    self.indents.push(level);
                    tokens.push(Spanned { token: Token::Indent, span });
                } else if level < cur {
                    while *self.indents.last().unwrap() > level {
                        self.indents.pop();
                        self.pending.push_back(Spanned { token: Token::Dedent, span: span.clone() });
                    }
                    while let Some(t) = self.pending.pop_front() {
                        tokens.push(t);
                    }
                }
            }

            let span = self.span();

            match self.peek() {
                None => {
                    // EOF: 残りDEDENTを全部出す
                    while self.indents.len() > 1 {
                        self.indents.pop();
                        tokens.push(Spanned { token: Token::Dedent, span: span.clone() });
                    }
                    tokens.push(Spanned { token: Token::Eof, span });
                    break 'outer;
                }
                Some('\n') => {
                    self.advance();
                    tokens.push(Spanned { token: Token::Newline, span });
                    self.at_line_start = true;
                }
                Some(' ') | Some('\t') => { self.advance(); } // 行中の空白は無視
                Some('#') => self.skip_comment(),
                Some('"') => {
                    self.advance();
                    let tok = self.read_string('"')?;
                    tokens.push(Spanned { token: tok, span });
                }
                Some('\'') => {
                    self.advance();
                    let tok = self.read_string('\'')?;
                    tokens.push(Spanned { token: tok, span });
                }
                Some(c) if c.is_ascii_digit() => {
                    self.advance();
                    let tok = self.read_number(c);
                    tokens.push(Spanned { token: tok, span });
                }
                Some(c) if c.is_alphabetic() || c == '_' => {
                    self.advance();
                    let tok = self.read_ident(c);
                    tokens.push(Spanned { token: tok, span });
                }
                Some(c) => {
                    self.advance();
                    if let Some(tok) = self.read_symbol(c)? {
                        tokens.push(Spanned { token: tok, span });
                    }
                }
            }
        }

        Ok(tokens)
    }
}

// ---- テスト ----

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        Lexer::new(src)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|s| s.token)
            .collect()
    }

    #[test]
    fn test_basic_tokens() {
        let tokens = lex("let x = 42");
        assert!(tokens.contains(&Token::Let));
        assert!(tokens.contains(&Token::Ident("x".into())));
        assert!(tokens.contains(&Token::Eq));
        assert!(tokens.contains(&Token::Int(42)));
    }

    #[test]
    fn test_float() {
        let tokens = lex("3.14");
        assert!(tokens.contains(&Token::Float(3.14)));
    }

    #[test]
    fn test_string() {
        let tokens = lex("\"hello\"");
        assert!(tokens.contains(&Token::Str("hello".into())));
    }

    #[test]
    fn test_indent_dedent() {
        let src = "if x:\n    let y = 1\n";
        let tokens = lex(src);
        assert!(tokens.contains(&Token::Indent));
        assert!(tokens.contains(&Token::Dedent));
    }

    #[test]
    fn test_range() {
        let tokens = lex("0..10");
        assert!(tokens.contains(&Token::Int(0)));
        assert!(tokens.contains(&Token::DotDot));
        assert!(tokens.contains(&Token::Int(10)));
    }

    #[test]
    fn test_arrow() {
        let tokens = lex("-> =>");
        assert!(tokens.contains(&Token::Arrow));
        assert!(tokens.contains(&Token::FatArrow));
    }

    #[test]
    fn test_keywords() {
        let tokens = lex("fn struct impl match");
        assert!(tokens.contains(&Token::Fn));
        assert!(tokens.contains(&Token::Struct));
        assert!(tokens.contains(&Token::Impl));
        assert!(tokens.contains(&Token::Match));
    }

    #[test]
    fn test_bool_and_null() {
        let tokens = lex("true false null");
        assert!(tokens.contains(&Token::Bool(true)));
        assert!(tokens.contains(&Token::Bool(false)));
        assert!(tokens.contains(&Token::Null));
    }

    #[test]
    fn test_comment_ignored() {
        let tokens = lex("let x = 1 # this is a comment\nlet y = 2");
        assert!(!tokens.iter().any(|t| matches!(t, Token::Ident(s) if s == "this")));
    }
}
