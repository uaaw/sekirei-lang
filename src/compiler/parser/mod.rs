/// sekirei Parser
/// トークン列を AST に変換する (再帰降下法)

use crate::lexer::{Span, Spanned, Token};

// ============================================================
// AST ノード定義
// ============================================================

#[derive(Debug, Clone)]
pub enum Expr {
    // リテラル
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    None,

    // 識別子
    Ident(String),

    // ブロック式 (if/match のインラインボディなど)
    Block(Vec<Stmt>),

    // 二項演算
    BinOp { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },

    // 単項演算
    UnOp { op: UnOp, expr: Box<Expr> },

    // 関数呼び出し
    Call { func: Box<Expr>, args: Vec<Expr> },

    // 無名関数: (x: int, y: int) -> int => body
    Lambda { params: Vec<Param>, ret: Option<Type>, body: Box<Expr> },

    // if式
    If {
        cond:  Box<Expr>,
        then:  Box<Expr>,
        elifs: Vec<(Expr, Expr)>,
        else_: Option<Box<Expr>>,
    },

    // match式
    Match { expr: Box<Expr>, arms: Vec<MatchArm> },

    // ? 演算子
    Try(Box<Expr>),

    // catch
    Catch { expr: Box<Expr>, handler: CatchHandler },

    // try ブロック
    TryBlock(Vec<Stmt>),

    // フィールドアクセス
    Field { expr: Box<Expr>, name: String },

    // インデックスアクセス
    Index { expr: Box<Expr>, idx: Box<Expr> },

    // 構造体リテラル
    StructLit { name: String, fields: Vec<(String, Expr)> },

    // Some / Ok / Err
    Some(Box<Expr>),
    Ok(Box<Expr>),
    Err(Box<Expr>),

    // 範囲: 0..10 / 0..=10
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
}

#[derive(Debug, Clone)]
pub enum CatchHandler {
    Default(Box<Expr>),              // catch "default"
    WithErr(String, Box<Expr>),      // catch |e| expr
    WithErrBlock(String, Vec<Stmt>), // catch |e|: block
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body:    Expr,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    Int(i64),
    Str(String),
    Bool(bool),
    None,
    Some(String),
    Ok(String),
    Err(String),
    Ident(String),
    Or(Vec<Pattern>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let      { name: String, ty: Option<Type>, value: Expr },
    Mut      { name: String, ty: Option<Type>, value: Expr },
    Assign   { target: Expr, value: Expr },
    Expr(Expr),
    Return(Option<Expr>),
    Break,
    Continue,
    For    { var: String, iter: Expr, body: Vec<Stmt> },
    While  { cond: Expr,  body: Vec<Stmt> },
    Loop(Vec<Stmt>),
}

#[derive(Debug, Clone)]
pub enum TopLevel {
    Fn       { name: String, params: Vec<Param>, ret: Option<Type>, body: Vec<Stmt> },
    Struct   { name: String, fields: Vec<(String, Type)> },
    Impl     { name: String, methods: Vec<TopLevel> },
    // import utils  /  import net.http
    Import   { path: Vec<String>, alias: Option<String> },
    // from std import io  /  from skp import http  /  from std.io import print
    FromImport {
        source: ImportSource,
        path:   Vec<String>,   // std/skp以降のパス (空もあり)
        names:  Vec<(String, Option<String>)>,
    },
}

/// importのソース種別
#[derive(Debug, Clone, PartialEq)]
pub enum ImportSource {
    Std,    // from std ...
    Skp,    // from skp ...
    Local,  // from ./foo ... (将来拡張)
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty:   Type,
    pub is_self: bool,
}

#[derive(Debug, Clone)]
pub enum Type {
    Int, Float, String, Str, Bool, Char, Byte, Void,
    I8, I16, I32, I64,
    Uint, U8, U16, U32, U64,
    F32, F64,
    List(Box<Type>),
    Dict(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Set(Box<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Nullable(Box<Type>),  // T?
    Union(Vec<Type>),     // T | U
    Fn(Vec<Type>, Box<Type>),
    Named(String),
    Generic(String, Vec<Type>),
}

#[derive(Debug, Clone)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

#[derive(Debug, Clone)]
pub enum UnOp { Neg, Not }

// ============================================================
// パーサーエラー
// ============================================================

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line:    usize,
    pub col:     usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[Parse Error] {} (line {}, col {})", self.message, self.line, self.col)
    }
}

// ============================================================
// パーサー
// ============================================================

pub struct Parser {
    tokens: Vec<Spanned>,
    pos:    usize,
}

impl Parser {
    pub fn new(tokens: Vec<Spanned>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ---- 基本操作 ----

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).map(|s| &s.token).unwrap_or(&Token::Eof)
    }

    fn peek_at(&self, offset: usize) -> &Token {
        self.tokens.get(self.pos + offset).map(|s| &s.token).unwrap_or(&Token::Eof)
    }

    fn span(&self) -> Span {
        self.tokens.get(self.pos)
            .map(|s| s.span.clone())
            .unwrap_or(Span { line: 0, col: 0 })
    }

    fn advance(&mut self) -> Token {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].token.clone();
            self.pos += 1;
            t
        } else {
            Token::Eof
        }
    }

    /// 現在のトークンが t なら消費して true、違えば false
    fn eat(&mut self, t: &Token) -> bool {
        if self.peek() == t {
            self.advance();
            true
        } else {
            false
        }
    }

    /// 現在のトークンが t でなければエラー
    fn expect(&mut self, expected: &Token) -> Result<(), ParseError> {
        if self.peek() == expected {
            self.advance();
            std::result::Result::Ok(())
        } else {
            Err(self.error(&format!("expected {:?}, got {:?}", expected, self.peek())))
        }
    }

    fn error(&self, msg: &str) -> ParseError {
        let s = self.span();
        ParseError { message: msg.to_string(), line: s.line, col: s.col }
    }

    fn skip_newlines(&mut self) {
        while *self.peek() == Token::Newline {
            self.advance();
        }
    }

    // ============================================================
    // トップレベル
    // ============================================================

    pub fn parse(&mut self) -> Result<Vec<TopLevel>, ParseError> {
        let mut items = Vec::new();
        self.skip_newlines();
        while *self.peek() != Token::Eof {
            items.push(self.parse_top_level()?);
            self.skip_newlines();
        }
        std::result::Result::Ok(items)
    }

    fn parse_top_level(&mut self) -> Result<TopLevel, ParseError> {
        match self.peek() {
            Token::Fn     => self.parse_fn(false),
            Token::Struct => self.parse_struct(),
            Token::Impl   => self.parse_impl(),
            Token::Import => self.parse_import(),
            Token::From   => self.parse_from_import(),
            t => Err(self.error(&format!("unexpected token at top level: {:?}", t))),
        }
    }

    // fn name(params) -> ret: block
    fn parse_fn(&mut self, is_method: bool) -> Result<TopLevel, ParseError> {
        self.expect(&Token::Fn)?;
        let name = self.parse_ident()?;
        self.expect(&Token::LParen)?;
        let params = self.parse_params(is_method)?;
        self.expect(&Token::RParen)?;

        let ret = if self.eat(&Token::Arrow) {
            Some(self.parse_type()?)
        } else {
            std::option::Option::None
        };

        self.expect(&Token::Colon)?;
        let body = self.parse_block()?;
        std::result::Result::Ok(TopLevel::Fn { name, params, ret, body })
    }

    // struct Name:\n  INDENT fields DEDENT
    fn parse_struct(&mut self) -> Result<TopLevel, ParseError> {
        self.expect(&Token::Struct)?;
        let name = self.parse_ident()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut fields = Vec::new();
        while *self.peek() != Token::Dedent && *self.peek() != Token::Eof {
            self.skip_newlines();
            if *self.peek() == Token::Dedent { break; }
            let fname = self.parse_ident()?;
            self.expect(&Token::Colon)?;
            let ftype = self.parse_type()?;
            fields.push((fname, ftype));
            self.eat(&Token::Newline);
        }

        self.expect(&Token::Dedent)?;
        std::result::Result::Ok(TopLevel::Struct { name, fields })
    }

    // impl Name:\n  INDENT fn* DEDENT
    fn parse_impl(&mut self) -> Result<TopLevel, ParseError> {
        self.expect(&Token::Impl)?;
        let name = self.parse_ident()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut methods = Vec::new();
        while *self.peek() != Token::Dedent && *self.peek() != Token::Eof {
            self.skip_newlines();
            if *self.peek() == Token::Dedent { break; }
            methods.push(self.parse_fn(true)?);
        }

        self.expect(&Token::Dedent)?;
        std::result::Result::Ok(TopLevel::Impl { name, methods })
    }

    // import a.b.c (as alias)?
    fn parse_import(&mut self) -> Result<TopLevel, ParseError> {
        self.expect(&Token::Import)?;
        let path = self.parse_dotted_path()?;
        let alias = if self.eat(&Token::As) {
            Some(self.parse_ident()?)
        } else {
            std::option::Option::None
        };
        self.eat(&Token::Newline);
        std::result::Result::Ok(TopLevel::Import { path, alias })
    }

    // from std import io
    // from std.io import print, read_line
    // from skp import http
    // from skp.http import get, post
    fn parse_from_import(&mut self) -> Result<TopLevel, ParseError> {
        self.expect(&Token::From)?;

        // 先頭の "std" / "skp" でソースを判定
        let first = self.parse_ident()?;
        let source = match first.as_str() {
            "std" => ImportSource::Std,
            "skp" => ImportSource::Skp,
            _     => return Err(self.error(&format!(
                "unknown import source '{}'. use 'std' or 'skp'", first
            ))),
        };

        // std.io や skp.http のようにサブパスがあれば続けて読む
        let mut path = Vec::new();
        while self.eat(&Token::Dot) {
            path.push(self.parse_ident()?);
        }

        self.expect(&Token::Import)?;

        let mut names = Vec::new();
        loop {
            let name = self.parse_ident()?;
            let alias = if self.eat(&Token::As) {
                Some(self.parse_ident()?)
            } else {
                std::option::Option::None
            };
            names.push((name, alias));
            if !self.eat(&Token::Comma) { break; }
        }

        self.eat(&Token::Newline);
        std::result::Result::Ok(TopLevel::FromImport { source, path, names })
    }

    fn parse_dotted_path(&mut self) -> Result<Vec<String>, ParseError> {
        let mut parts = vec![self.parse_ident()?];
        while self.eat(&Token::Dot) {
            parts.push(self.parse_ident()?);
        }
        std::result::Result::Ok(parts)
    }

    // ============================================================
    // パラメータ・型
    // ============================================================

    fn parse_params(&mut self, is_method: bool) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();

        // self パラメータ
        if is_method {
            if let Token::Ident(s) = self.peek() {
                if s == "self" {
                    self.advance();
                    params.push(Param { name: "self".into(), ty: Type::Named("Self".into()), is_self: true });
                    if *self.peek() == Token::RParen { return std::result::Result::Ok(params); }
                    self.expect(&Token::Comma)?;
                }
            }
        }

        while *self.peek() != Token::RParen {
            if !params.is_empty() {
                self.expect(&Token::Comma)?;
            }
            if *self.peek() == Token::RParen { break; }
            let name = self.parse_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            params.push(Param { name, ty, is_self: false });
        }

        std::result::Result::Ok(params)
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let base = self.parse_base_type()?;

        // T? (nullable)
        if self.eat(&Token::Question) {
            return std::result::Result::Ok(Type::Nullable(Box::new(base)));
        }

        // T | U (union)
        if self.eat(&Token::Pipe) {
            let mut types = vec![base];
            loop {
                types.push(self.parse_base_type()?);
                if !self.eat(&Token::Pipe) { break; }
            }
            return std::result::Result::Ok(Type::Union(types));
        }

        std::result::Result::Ok(base)
    }

    fn parse_base_type(&mut self) -> Result<Type, ParseError> {
        let ty = match self.advance() {
            Token::TInt    => Type::Int,
            Token::TI8     => Type::I8,
            Token::TI16    => Type::I16,
            Token::TI32    => Type::I32,
            Token::TI64    => Type::I64,
            Token::TUint   => Type::Uint,
            Token::TU8     => Type::U8,
            Token::TU16    => Type::U16,
            Token::TU32    => Type::U32,
            Token::TU64    => Type::U64,
            Token::TFloat  => Type::Float,
            Token::TF32    => Type::F32,
            Token::TF64    => Type::F64,
            Token::TString => Type::String,
            Token::TStr    => Type::Str,
            Token::TBool   => Type::Bool,
            Token::TChar   => Type::Char,
            Token::TByte   => Type::Byte,
            Token::TVoid   => Type::Void,
            Token::Ident(name) => {
                if self.eat(&Token::Lt) {
                    let mut args = Vec::new();
                    loop {
                        args.push(self.parse_type()?);
                        if !self.eat(&Token::Comma) { break; }
                    }
                    self.expect(&Token::Gt)?;
                    match name.as_str() {
                        "list"   => Type::List(Box::new(args.remove(0))),
                        "set"    => Type::Set(Box::new(args.remove(0))),
                        "dict"   => {
                            let k = args.remove(0);
                            let v = args.remove(0);
                            Type::Dict(Box::new(k), Box::new(v))
                        }
                        "tuple"  => Type::Tuple(args),
                        "Option" => Type::Option(Box::new(args.remove(0))),
                        "Result" => {
                            let ok  = args.remove(0);
                            let err = args.remove(0);
                            Type::Result(Box::new(ok), Box::new(err))
                        }
                        _ => Type::Generic(name, args),
                    }
                } else {
                    Type::Named(name)
                }
            }
            Token::Fn => {
                self.expect(&Token::LParen)?;
                let mut args = Vec::new();
                while *self.peek() != Token::RParen {
                    if !args.is_empty() { self.expect(&Token::Comma)?; }
                    args.push(self.parse_type()?);
                }
                self.expect(&Token::RParen)?;
                self.expect(&Token::Arrow)?;
                let ret = self.parse_type()?;
                Type::Fn(args, Box::new(ret))
            }
            t => return Err(self.error(&format!("expected type, got {:?}", t))),
        };
        std::result::Result::Ok(ty)
    }

    // ============================================================
    // ブロック・文
    // ============================================================

    /// ':' のあとに呼ぶ: NEWLINE INDENT stmt+ DEDENT
    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut stmts = Vec::new();
        while *self.peek() != Token::Dedent && *self.peek() != Token::Eof {
            self.skip_newlines();
            if *self.peek() == Token::Dedent { break; }
            stmts.push(self.parse_stmt()?);
        }

        self.expect(&Token::Dedent)?;
        std::result::Result::Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let stmt = match self.peek() {
            Token::Let      => self.parse_let()?,
            Token::Mut      => self.parse_mut_decl()?,
            Token::Return   => {
                self.advance();
                if matches!(self.peek(), Token::Newline | Token::Eof | Token::Dedent) {
                    Stmt::Return(std::option::Option::None)
                } else {
                    Stmt::Return(Some(self.parse_expr()?))
                }
            }
            Token::Break    => { self.advance(); Stmt::Break }
            Token::Continue => { self.advance(); Stmt::Continue }
            Token::For      => self.parse_for()?,
            Token::While    => self.parse_while()?,
            Token::Loop     => self.parse_loop_stmt()?,
            _ => {
                // 式 or 代入
                let expr = self.parse_expr()?;
                if self.eat(&Token::Eq) {
                    let value = self.parse_expr()?;
                    self.eat(&Token::Newline);
                    return std::result::Result::Ok(Stmt::Assign { target: expr, value });
                }
                Stmt::Expr(expr)
            }
        };
        self.eat(&Token::Newline);
        std::result::Result::Ok(stmt)
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Let)?;
        let name = self.parse_ident()?;
        let ty = if self.eat(&Token::Colon) { Some(self.parse_type()?) } else { std::option::Option::None };
        self.expect(&Token::Eq)?;
        let value = self.parse_expr()?;
        std::result::Result::Ok(Stmt::Let { name, ty, value })
    }

    fn parse_mut_decl(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Mut)?;
        let name = self.parse_ident()?;
        let ty = if self.eat(&Token::Colon) { Some(self.parse_type()?) } else { std::option::Option::None };
        self.expect(&Token::Eq)?;
        let value = self.parse_expr()?;
        std::result::Result::Ok(Stmt::Mut { name, ty, value })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::For)?;
        let var = self.parse_ident()?;
        self.expect(&Token::In)?;
        let iter = self.parse_expr()?;
        self.expect(&Token::Colon)?;
        let body = self.parse_block()?;
        std::result::Result::Ok(Stmt::For { var, iter, body })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::While)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::Colon)?;
        let body = self.parse_block()?;
        std::result::Result::Ok(Stmt::While { cond, body })
    }

    fn parse_loop_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Loop)?;
        self.expect(&Token::Colon)?;
        let body = self.parse_block()?;
        std::result::Result::Ok(Stmt::Loop(body))
    }

    // ============================================================
    // 式 (優先度: 低 → 高)
    // ============================================================

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_range()
    }

    // expr .. expr  /  expr ..= expr
    fn parse_range(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_catch()?;
        if self.eat(&Token::DotDot) {
            let end = self.parse_catch()?;
            return std::result::Result::Ok(Expr::Range {
                start: Box::new(expr), end: Box::new(end), inclusive: false,
            });
        }
        if self.eat(&Token::DotDotEq) {
            let end = self.parse_catch()?;
            return std::result::Result::Ok(Expr::Range {
                start: Box::new(expr), end: Box::new(end), inclusive: true,
            });
        }
        std::result::Result::Ok(expr)
    }

    // catch expr  /  catch |e| expr  /  catch |e|: block
    fn parse_catch(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_try_op()?;

        if self.eat(&Token::Catch) {
            let handler = if self.eat(&Token::Pipe) {
                // catch |e| ...
                let e = self.parse_ident()?;
                self.expect(&Token::Pipe)?;
                if self.eat(&Token::Colon) {
                    CatchHandler::WithErrBlock(e, self.parse_block()?)
                } else {
                    CatchHandler::WithErr(e, Box::new(self.parse_expr()?))
                }
            } else {
                // catch <default_value>
                CatchHandler::Default(Box::new(self.parse_expr()?))
            };
            return std::result::Result::Ok(Expr::Catch { expr: Box::new(expr), handler });
        }

        std::result::Result::Ok(expr)
    }

    // expr?  (後置 ? は複数可: expr???)
    fn parse_try_op(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_or()?;
        while self.eat(&Token::Question) {
            expr = Expr::Try(Box::new(expr));
        }
        std::result::Result::Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_and()?;
        while self.eat(&Token::Or) {
            let rhs = self.parse_and()?;
            lhs = Expr::BinOp { op: BinOp::Or, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        std::result::Result::Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_cmp()?;
        while self.eat(&Token::And) {
            let rhs = self.parse_cmp()?;
            lhs = Expr::BinOp { op: BinOp::And, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        std::result::Result::Ok(lhs)
    }

    fn parse_cmp(&mut self) -> Result<Expr, ParseError> {
        let lhs = self.parse_add()?;
        let op = match self.peek() {
            Token::EqEq  => BinOp::Eq,
            Token::NotEq => BinOp::NotEq,
            Token::Lt    => BinOp::Lt,
            Token::LtEq  => BinOp::LtEq,
            Token::Gt    => BinOp::Gt,
            Token::GtEq  => BinOp::GtEq,
            _            => return std::result::Result::Ok(lhs),
        };
        self.advance();
        let rhs = self.parse_add()?;
        std::result::Result::Ok(Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) })
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Token::Plus  => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _            => break,
            };
            self.advance();
            let rhs = self.parse_mul()?;
            lhs = Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        std::result::Result::Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star    => BinOp::Mul,
                Token::Slash   => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _              => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        std::result::Result::Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::Minus => {
                self.advance();
                Ok(Expr::UnOp { op: UnOp::Neg, expr: Box::new(self.parse_unary()?) })
            }
            Token::Not => {
                self.advance();
                Ok(Expr::UnOp { op: UnOp::Not, expr: Box::new(self.parse_unary()?) })
            }
            _ => self.parse_postfix(),
        }
    }

    // 後置演算子: .field  .method(args)  (args)  [idx]
    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                Token::Dot => {
                    self.advance();
                    let name = self.parse_ident()?;
                    if self.eat(&Token::LParen) {
                        let args = self.parse_args()?;
                        self.expect(&Token::RParen)?;
                        expr = Expr::Call {
                            func: Box::new(Expr::Field { expr: Box::new(expr), name }),
                            args,
                        };
                    } else {
                        expr = Expr::Field { expr: Box::new(expr), name };
                    }
                }
                Token::LParen => {
                    self.advance();
                    let args = self.parse_args()?;
                    self.expect(&Token::RParen)?;
                    expr = Expr::Call { func: Box::new(expr), args };
                }
                Token::LBracket => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Index { expr: Box::new(expr), idx: Box::new(idx) };
                }
                _ => break,
            }
        }
        std::result::Result::Ok(expr)
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        while *self.peek() != Token::RParen {
            if !args.is_empty() { self.expect(&Token::Comma)?; }
            if *self.peek() == Token::RParen { break; }
            args.push(self.parse_expr()?);
        }
        std::result::Result::Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Int(n)   => { self.advance(); Ok(Expr::Int(n)) }
            Token::Float(f) => { self.advance(); Ok(Expr::Float(f)) }
            Token::Str(s)   => { self.advance(); Ok(Expr::Str(s)) }
            Token::Bool(b)  => { self.advance(); Ok(Expr::Bool(b)) }
            Token::Null     => { self.advance(); Ok(Expr::Null) }
            Token::NoneKw   => { self.advance(); Ok(Expr::None) }

            Token::SomeKw => {
                self.advance();
                self.expect(&Token::LParen)?;
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Some(Box::new(e)))
            }
            Token::OkKw => {
                self.advance();
                self.expect(&Token::LParen)?;
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Ok(Box::new(e)))
            }
            Token::ErrKw => {
                self.advance();
                self.expect(&Token::LParen)?;
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Err(Box::new(e)))
            }

            Token::If    => self.parse_if_expr(),
            Token::Match => self.parse_match_expr(),
            Token::Try   => self.parse_try_block_expr(),

            Token::LParen => {
                if self.is_lambda_start() {
                    self.parse_lambda()
                } else {
                    self.advance();
                    let e = self.parse_expr()?;
                    self.expect(&Token::RParen)?;
                    Ok(e)
                }
            }

            Token::Ident(name) => {
                self.advance();
                // 構造体リテラル: Name { field: expr, ... }
                // 注意: 次が { のときだけ、かつ代入文の右辺などでの誤検知を避けるため
                //       次のトークンが { で、その次が IDENT : ならリテラル
                if *self.peek() == Token::LBrace && self.is_struct_lit_start() {
                    self.advance(); // {
                    let mut fields = Vec::new();
                    while *self.peek() != Token::RBrace {
                        if !fields.is_empty() { self.expect(&Token::Comma)?; }
                        if *self.peek() == Token::RBrace { break; }
                        let fname = self.parse_ident()?;
                        self.expect(&Token::Colon)?;
                        let fval = self.parse_expr()?;
                        fields.push((fname, fval));
                    }
                    self.expect(&Token::RBrace)?;
                    Ok(Expr::StructLit { name, fields })
                } else {
                    Ok(Expr::Ident(name))
                }
            }

            t => Err(self.error(&format!("unexpected token in expression: {:?}", t))),
        }
    }

    // ラムダかどうか先読み: ( ) -> や ( ident : なら true
    fn is_lambda_start(&self) -> bool {
        // pos は '(' の位置
        match self.peek_at(1) {
            // () ->
            Token::RParen => matches!(self.peek_at(2), Token::Arrow),
            // (ident :
            Token::Ident(_) => matches!(self.peek_at(2), Token::Colon),
            _ => false,
        }
    }

    // 構造体リテラル先読み: { の次が ident : ならリテラル
    fn is_struct_lit_start(&self) -> bool {
        // peek() は '{'
        matches!(self.peek_at(1), Token::Ident(_)) &&
        matches!(self.peek_at(2), Token::Colon)
    }

    // (params) -> type => body
    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::LParen)?;
        let params = self.parse_params(false)?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Arrow)?;
        let ret = Some(self.parse_type()?);
        self.expect(&Token::FatArrow)?;
        let body = self.parse_expr()?;
        std::result::Result::Ok(Expr::Lambda { params, ret, body: Box::new(body) })
    }

    // if expr: body (elif expr: body)* (else: body)?
    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::If)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::Colon)?;

        let then = self.parse_expr_or_block()?;

        let mut elifs = Vec::new();
        while self.eat(&Token::Elif) {
            let c = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            elifs.push((c, self.parse_expr_or_block()?));
        }

        let else_ = if self.eat(&Token::Else) {
            self.expect(&Token::Colon)?;
            Some(Box::new(self.parse_expr_or_block()?))
        } else {
            std::option::Option::None
        };

        std::result::Result::Ok(Expr::If { cond: Box::new(cond), then: Box::new(then), elifs, else_ })
    }

    // match expr:\n  INDENT arm* DEDENT
    fn parse_match_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Match)?;
        let expr = self.parse_expr()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut arms = Vec::new();
        while *self.peek() != Token::Dedent && *self.peek() != Token::Eof {
            self.skip_newlines();
            if *self.peek() == Token::Dedent { break; }
            let pattern = self.parse_pattern()?;
            self.expect(&Token::FatArrow)?;
            let body = self.parse_expr_or_block()?;
            self.eat(&Token::Newline);
            arms.push(MatchArm { pattern, body });
        }

        self.expect(&Token::Dedent)?;
        std::result::Result::Ok(Expr::Match { expr: Box::new(expr), arms })
    }

    // try:\n  block
    fn parse_try_block_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Try)?;
        self.expect(&Token::Colon)?;
        std::result::Result::Ok(Expr::TryBlock(self.parse_block()?))
    }

    /// : の後: NEWLINE なら block、そうでなければ inline expr
    fn parse_expr_or_block(&mut self) -> Result<Expr, ParseError> {
        if *self.peek() == Token::Newline {
            std::result::Result::Ok(Expr::Block(self.parse_block()?))
        } else {
            self.parse_expr()
        }
    }

    // ============================================================
    // パターン
    // ============================================================

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let pat = self.parse_single_pattern()?;
        if self.eat(&Token::Pipe) {
            let mut pats = vec![pat];
            loop {
                pats.push(self.parse_single_pattern()?);
                if !self.eat(&Token::Pipe) { break; }
            }
            return std::result::Result::Ok(Pattern::Or(pats));
        }
        std::result::Result::Ok(pat)
    }

    fn parse_single_pattern(&mut self) -> Result<Pattern, ParseError> {
        match self.peek().clone() {
            Token::Ident(s) if s == "_" => { self.advance(); Ok(Pattern::Wildcard) }
            Token::Int(n)   => { self.advance(); Ok(Pattern::Int(n)) }
            Token::Str(s)   => { self.advance(); Ok(Pattern::Str(s)) }
            Token::Bool(b)  => { self.advance(); Ok(Pattern::Bool(b)) }
            Token::NoneKw   => { self.advance(); Ok(Pattern::None) }
            Token::SomeKw => {
                self.advance();
                self.expect(&Token::LParen)?;
                let n = self.parse_ident()?;
                self.expect(&Token::RParen)?;
                Ok(Pattern::Some(n))
            }
            Token::OkKw => {
                self.advance();
                self.expect(&Token::LParen)?;
                let n = self.parse_ident()?;
                self.expect(&Token::RParen)?;
                Ok(Pattern::Ok(n))
            }
            Token::ErrKw => {
                self.advance();
                self.expect(&Token::LParen)?;
                let n = self.parse_ident()?;
                self.expect(&Token::RParen)?;
                Ok(Pattern::Err(n))
            }
            Token::Ident(name) => { self.advance(); Ok(Pattern::Ident(name)) }
            t => Err(self.error(&format!("expected pattern, got {:?}", t))),
        }
    }

    // ============================================================
    // ユーティリティ
    // ============================================================

    fn parse_ident(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Ident(s) => std::result::Result::Ok(s),
            t => Err(self.error(&format!("expected identifier, got {:?}", t))),
        }
    }
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(src: &str) -> Vec<TopLevel> {
        let tokens = Lexer::new(src).tokenize().expect("lex failed");
        Parser::new(tokens).parse().expect("parse failed")
    }

    #[test]
    fn test_fn_simple() {
        let ast = parse("fn add(x: int, y: int) -> int:\n    return x + y\n");
        assert!(matches!(&ast[0], TopLevel::Fn { name, .. } if name == "add"));
    }

    #[test]
    fn test_let_stmt() {
        let ast = parse("fn main():\n    let x = 42\n");
        if let TopLevel::Fn { body, .. } = &ast[0] {
            assert!(matches!(&body[0], Stmt::Let { name, .. } if name == "x"));
        }
    }

    #[test]
    fn test_mut_stmt() {
        let ast = parse("fn main():\n    mut y = 10\n");
        if let TopLevel::Fn { body, .. } = &ast[0] {
            assert!(matches!(&body[0], Stmt::Mut { name, .. } if name == "y"));
        }
    }

    #[test]
    fn test_if_inline() {
        let ast = parse("fn main():\n    let s = if x > 0: \"pos\" else: \"neg\"\n");
        if let TopLevel::Fn { body, .. } = &ast[0] {
            if let Stmt::Let { value: Expr::If { .. }, .. } = &body[0] {
                // ok
            } else {
                panic!("expected if expression");
            }
        }
    }

    #[test]
    fn test_struct() {
        let ast = parse("struct Point:\n    x: float\n    y: float\n");
        assert!(matches!(&ast[0], TopLevel::Struct { name, .. } if name == "Point"));
    }

    #[test]
    fn test_impl() {
        let src = "impl Point:\n    fn new(x: float, y: float) -> Point:\n        return Point { x: x, y: y }\n";
        let ast = parse(src);
        assert!(matches!(&ast[0], TopLevel::Impl { name, .. } if name == "Point"));
    }

    #[test]
    fn test_import() {
        let ast = parse("import std.io\n");
        assert!(matches!(&ast[0], TopLevel::Import { path, .. } if path == &["std", "io"]));
    }

    #[test]
    fn test_from_import_std() {
        let ast = parse("from std import io\n");
        if let TopLevel::FromImport { source, path, names } = &ast[0] {
            assert_eq!(*source, ImportSource::Std);
            assert!(path.is_empty());
            assert_eq!(names[0].0, "io");
        } else { panic!("expected FromImport"); }
    }

    #[test]
    fn test_from_import_std_subpath() {
        let ast = parse("from std.io import print, read_line\n");
        if let TopLevel::FromImport { source, path, names } = &ast[0] {
            assert_eq!(*source, ImportSource::Std);
            assert_eq!(path, &["io"]);
            assert_eq!(names.len(), 2);
        } else { panic!("expected FromImport"); }
    }

    #[test]
    fn test_from_import_skp() {
        let ast = parse("from skp import http\n");
        if let TopLevel::FromImport { source, .. } = &ast[0] {
            assert_eq!(*source, ImportSource::Skp);
        } else { panic!("expected FromImport"); }
    }

    #[test]
    fn test_for_loop() {
        let ast = parse("fn main():\n    for i in items:\n        print(i)\n");
        if let TopLevel::Fn { body, .. } = &ast[0] {
            assert!(matches!(&body[0], Stmt::For { var, .. } if var == "i"));
        }
    }

    #[test]
    fn test_match() {
        let src = "fn main():\n    match x:\n        1 => print(\"one\")\n        _ => print(\"other\")\n";
        let ast = parse(src);
        if let TopLevel::Fn { body, .. } = &ast[0] {
            assert!(matches!(&body[0], Stmt::Expr(Expr::Match { .. })));
        }
    }

    #[test]
    fn test_lambda() {
        let ast = parse("fn main():\n    let f = (x: int) -> int => x + 1\n");
        if let TopLevel::Fn { body, .. } = &ast[0] {
            assert!(matches!(&body[0], Stmt::Let { value: Expr::Lambda { .. }, .. }));
        }
    }

    #[test]
    fn test_try_op() {
        let ast = parse("fn main():\n    let v = read_file(\"x\")?\n");
        if let TopLevel::Fn { body, .. } = &ast[0] {
            assert!(matches!(&body[0], Stmt::Let { value: Expr::Try(_), .. }));
        }
    }
}
