/// sekirei Semantic Analyzer
/// 型検査・型推論 (2パス方式)

use std::collections::HashMap;
use crate::parser::{
    BinOp, CatchHandler, Expr, MatchArm, Param, Pattern, Stmt, TopLevel, Type, UnOp,
};

// ============================================================
// エラー
// ============================================================

#[derive(Debug)]
pub struct SemaError {
    pub message: String,
    pub line:    usize,
    pub col:     usize,
}

impl std::fmt::Display for SemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "[Sema Error] {} (line {}, col {})", self.message, self.line, self.col)
        } else {
            write!(f, "[Sema Error] {}", self.message)
        }
    }
}

// ============================================================
// 型環境（スコープスタック）
// ============================================================

struct TypeEnv {
    scopes: Vec<HashMap<String, Type>>,
}

impl TypeEnv {
    fn new() -> Self {
        Self { scopes: vec![HashMap::new()] }
    }

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: &str, ty: Type) {
        self.scopes.last_mut().unwrap().insert(name.to_string(), ty);
    }

    fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }
}

// ============================================================
// 関数シグネチャ
// ============================================================

#[derive(Clone, Debug)]
struct FnSig {
    params: Vec<(String, Type)>,
    ret:    Type,
}

// ============================================================
// セマンティック解析器
// ============================================================

pub struct SemanticAnalyzer {
    env:         TypeEnv,
    fns:         HashMap<String, FnSig>,
    structs:     HashMap<String, Vec<(String, Type)>>,
    current_ret: Option<Type>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut a = Self {
            env:         TypeEnv::new(),
            fns:         HashMap::new(),
            structs:     HashMap::new(),
            current_ret: None,
        };
        a.register_builtins();
        a
    }

    // 組み込み関数を登録
    fn register_builtins(&mut self) {
        let builtins: &[(&str, &[Type], Type)] = &[
            ("print",     &[Type::String],            Type::Void),
            ("println",   &[Type::String],            Type::Void),
            ("read_line", &[],                        Type::String),
            ("len",       &[Type::Named("?".into())], Type::Int),
            ("str",       &[Type::Named("?".into())], Type::String),
            ("int",       &[Type::Named("?".into())], Type::Int),
            ("float",     &[Type::Named("?".into())], Type::Float),
        ];
        for (name, params, ret) in builtins {
            let p = params.iter().enumerate()
                .map(|(i, t)| (format!("_{}", i), t.clone()))
                .collect();
            self.fns.insert(name.to_string(), FnSig { params: p, ret: ret.clone() });
        }
    }

    pub fn analyze(&mut self, ast: &[TopLevel]) -> Result<(), SemaError> {
        // パス1: 宣言収集
        for item in ast {
            self.collect_decl(item)?;
        }
        // パス2: 型検査
        for item in ast {
            self.check_top_level(item)?;
        }
        Ok(())
    }

    // ============================================================
    // パス1: 宣言収集
    // ============================================================

    fn collect_decl(&mut self, item: &TopLevel) -> Result<(), SemaError> {
        match item {
            TopLevel::Fn { name, params, ret, .. } => {
                let sig = FnSig {
                    params: params.iter()
                        .filter(|p| !p.is_self)
                        .map(|p| (p.name.clone(), p.ty.clone()))
                        .collect(),
                    ret: ret.clone().unwrap_or(Type::Void),
                };
                self.fns.insert(name.clone(), sig);
            }
            TopLevel::Struct { name, fields } => {
                self.structs.insert(name.clone(), fields.clone());
            }
            TopLevel::Impl { name, methods } => {
                for method in methods {
                    if let TopLevel::Fn { name: mname, params, ret, .. } = method {
                        let full = format!("{}::{}", name, mname);
                        let sig = FnSig {
                            params: params.iter()
                                .filter(|p| !p.is_self)
                                .map(|p| (p.name.clone(), p.ty.clone()))
                                .collect(),
                            ret: ret.clone().unwrap_or(Type::Void),
                        };
                        self.fns.insert(full, sig);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ============================================================
    // パス2: 型検査
    // ============================================================

    fn check_top_level(&mut self, item: &TopLevel) -> Result<(), SemaError> {
        match item {
            TopLevel::Fn { name, params, ret, body } => {
                self.check_fn(name, params, ret, body)?;
            }
            TopLevel::Impl { methods, .. } => {
                for m in methods { self.check_top_level(m)?; }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_fn(
        &mut self,
        _name: &str,
        params: &[Param],
        ret: &Option<Type>,
        body: &[Stmt],
    ) -> Result<(), SemaError> {
        self.env.push();
        self.current_ret = Some(ret.clone().unwrap_or(Type::Void));

        for p in params {
            if !p.is_self {
                self.env.define(&p.name, p.ty.clone());
            }
        }

        for stmt in body {
            self.check_stmt(stmt)?;
        }

        self.env.pop();
        self.current_ret = None;
        Ok(())
    }

    // ============================================================
    // 文の型検査
    // ============================================================

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), SemaError> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let inferred = self.infer_expr(value)?;
                let actual = if let Some(declared) = ty {
                    self.unify(declared, &inferred, &format!("let '{}'", name))?;
                    declared.clone()
                } else {
                    inferred
                };
                self.env.define(name, actual);
            }

            Stmt::Mut { name, ty, value } => {
                let inferred = self.infer_expr(value)?;
                let actual = if let Some(declared) = ty {
                    self.unify(declared, &inferred, &format!("mut '{}'", name))?;
                    declared.clone()
                } else {
                    inferred
                };
                self.env.define(name, actual);
            }

            Stmt::Assign { target, value } => {
                let t_ty = self.infer_expr(target)?;
                let v_ty = self.infer_expr(value)?;
                self.unify(&t_ty, &v_ty, "assignment")?;
            }

            Stmt::Return(expr) => {
                let expected = self.current_ret.clone().unwrap_or(Type::Void);
                let actual = expr.as_ref()
                    .map(|e| self.infer_expr(e))
                    .transpose()?
                    .unwrap_or(Type::Void);
                self.unify(&expected, &actual, "return")?;
            }

            Stmt::Expr(e) => { self.infer_expr(e)?; }

            Stmt::For { var, iter, body } => {
                let iter_ty = self.infer_expr(iter)?;
                let elem_ty = self.element_type_of(&iter_ty)?;
                self.env.push();
                self.env.define(var, elem_ty);
                for s in body { self.check_stmt(s)?; }
                self.env.pop();
            }

            Stmt::While { cond, body } => {
                let c = self.infer_expr(cond)?;
                self.unify(&Type::Bool, &c, "while condition")?;
                self.env.push();
                for s in body { self.check_stmt(s)?; }
                self.env.pop();
            }

            Stmt::Loop(body) => {
                self.env.push();
                for s in body { self.check_stmt(s)?; }
                self.env.pop();
            }

            Stmt::Break | Stmt::Continue => {}
        }
        Ok(())
    }

    // ============================================================
    // 式の型推論
    // ============================================================

    fn infer_expr(&mut self, expr: &Expr) -> Result<Type, SemaError> {
        match expr {
            Expr::Int(_)   => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Str(_)   => Ok(Type::String),
            Expr::Bool(_)  => Ok(Type::Bool),
            Expr::Null     => Ok(Type::Nullable(Box::new(Type::Named("?".into())))),
            Expr::None     => Ok(Type::Option(Box::new(Type::Named("?".into())))),

            Expr::Ident(name) => {
                if let Some(t) = self.env.lookup(name).cloned() {
                    return Ok(t);
                }
                if let Some(sig) = self.fns.get(name).cloned() {
                    let param_tys: Vec<Type> = sig.params.iter().map(|(_, t)| t.clone()).collect();
                    return Ok(Type::Fn(param_tys, Box::new(sig.ret)));
                }
                Err(SemaError { message: format!("undefined variable '{}'", name), line: 0, col: 0 })
            }

            Expr::BinOp { op, lhs, rhs } => {
                let l = self.infer_expr(lhs)?;
                let r = self.infer_expr(rhs)?;
                self.infer_binop(op, &l, &r)
            }

            Expr::UnOp { op, expr } => {
                let ty = self.infer_expr(expr)?;
                match op {
                    UnOp::Neg => {
                        if self.is_numeric(&ty) {
                            Ok(ty)
                        } else {
                            Err(SemaError {
                                message: "'-' requires numeric type".into(), line: 0, col: 0,
                            })
                        }
                    }
                    UnOp::Not => {
                        self.unify(&Type::Bool, &ty, "'!'")?;
                        Ok(Type::Bool)
                    }
                }
            }

            Expr::Call { func, args } => self.infer_call(func, args),

            Expr::Lambda { params, ret, body } => {
                self.env.push();
                let saved = self.current_ret.clone();
                let ret_ty = ret.clone().unwrap_or(Type::Named("?".into()));
                self.current_ret = Some(ret_ty);

                for p in params { self.env.define(&p.name, p.ty.clone()); }

                let body_ty = self.infer_expr(body)?;
                self.env.pop();
                self.current_ret = saved;

                let param_tys = params.iter().map(|p| p.ty.clone()).collect();
                Ok(Type::Fn(param_tys, Box::new(body_ty)))
            }

            Expr::If { cond, then, elifs, else_ } => {
                let c = self.infer_expr(cond)?;
                self.unify(&Type::Bool, &c, "if condition")?;

                let then_ty = self.infer_expr(then)?;

                for (ec, eb) in elifs {
                    let ect = self.infer_expr(ec)?;
                    self.unify(&Type::Bool, &ect, "elif condition")?;
                    let ebt = self.infer_expr(eb)?;
                    self.unify(&then_ty, &ebt, "elif branch")?;
                }

                if let Some(e) = else_ {
                    let et = self.infer_expr(e)?;
                    self.unify(&then_ty, &et, "else branch")?;
                    Ok(then_ty)
                } else {
                    Ok(Type::Void)
                }
            }

            Expr::Match { expr, arms } => {
                self.infer_expr(expr)?;
                if arms.is_empty() { return Ok(Type::Void); }
                let first = self.infer_arm(&arms[0])?;
                for arm in &arms[1..] {
                    let t = self.infer_arm(arm)?;
                    self.unify(&first, &t, "match arm")?;
                }
                Ok(first)
            }

            Expr::Try(inner) => {
                let ty = self.infer_expr(inner)?;
                match ty {
                    Type::Result(ok, _) => Ok(*ok),
                    Type::Option(inner) => Ok(*inner),
                    t => Err(SemaError {
                        message: format!("'?' requires Result<T,E> or Option<T>, got {:?}", t),
                        line: 0, col: 0,
                    }),
                }
            }

            Expr::Catch { expr, handler } => {
                let ty = self.infer_expr(expr)?;
                match handler {
                    CatchHandler::Default(default) => {
                        let d = self.infer_expr(default)?;
                        match ty {
                            Type::Result(ok, _) => { self.unify(&ok, &d, "catch")?; Ok(*ok) }
                            Type::Option(i)     => { self.unify(&i, &d, "catch")?; Ok(*i) }
                            _                   => Ok(d),
                        }
                    }
                    CatchHandler::WithErr(name, body) => {
                        self.env.push();
                        self.env.define(name, Type::Named("Error".into()));
                        let r = self.infer_expr(body)?;
                        self.env.pop();
                        Ok(r)
                    }
                    CatchHandler::WithErrBlock(name, stmts) => {
                        self.env.push();
                        self.env.define(name, Type::Named("Error".into()));
                        for s in stmts { self.check_stmt(s)?; }
                        self.env.pop();
                        Ok(Type::Void)
                    }
                }
            }

            Expr::TryBlock(stmts) => {
                self.env.push();
                let mut last = Type::Void;
                for s in stmts {
                    if let Stmt::Expr(e) = s { last = self.infer_expr(e)?; }
                    else { self.check_stmt(s)?; last = Type::Void; }
                }
                self.env.pop();
                Ok(Type::Result(Box::new(last), Box::new(Type::Named("Error".into()))))
            }

            Expr::Block(stmts) => {
                self.env.push();
                let mut last = Type::Void;
                for s in stmts {
                    match s {
                        Stmt::Expr(e)          => { last = self.infer_expr(e)?; }
                        Stmt::Return(Some(e))  => { last = self.infer_expr(e)?; }
                        _                      => { self.check_stmt(s)?; last = Type::Void; }
                    }
                }
                self.env.pop();
                Ok(last)
            }

            Expr::Field { expr, name } => {
                let ty = self.infer_expr(expr)?;
                self.infer_field(&ty, name)
            }

            Expr::Index { expr, idx } => {
                let ty = self.infer_expr(expr)?;
                self.infer_expr(idx)?;
                self.infer_index(&ty)
            }

            Expr::StructLit { name, fields } => {
                let def = self.structs.get(name).cloned().ok_or_else(|| SemaError {
                    message: format!("undefined struct '{}'", name), line: 0, col: 0,
                })?;
                for (fname, fval) in fields {
                    let expected = def.iter().find(|(n, _)| n == fname)
                        .map(|(_, t)| t.clone())
                        .ok_or_else(|| SemaError {
                            message: format!("struct '{}' has no field '{}'", name, fname),
                            line: 0, col: 0,
                        })?;
                    let actual = self.infer_expr(fval)?;
                    self.unify(&expected, &actual, fname)?;
                }
                Ok(Type::Named(name.clone()))
            }

            Expr::Range { start, end, .. } => {
                self.infer_expr(start)?;
                self.infer_expr(end)?;
                Ok(Type::Named("Range".into()))
            }

            Expr::Some(inner) => {
                let t = self.infer_expr(inner)?;
                Ok(Type::Option(Box::new(t)))
            }
            Expr::Ok(inner) => {
                let t = self.infer_expr(inner)?;
                Ok(Type::Result(Box::new(t), Box::new(Type::Named("Error".into()))))
            }
            Expr::Err(inner) => {
                let t = self.infer_expr(inner)?;
                Ok(Type::Result(Box::new(Type::Named("?".into())), Box::new(t)))
            }
        }
    }

    // ============================================================
    // ヘルパー
    // ============================================================

    fn infer_binop(&self, op: &BinOp, l: &Type, r: &Type) -> Result<Type, SemaError> {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                // 文字列連結
                if matches!(op, BinOp::Add) && matches!(l, Type::String) { return Ok(Type::String); }
                if self.types_compat(l, r) && self.is_numeric(l) {
                    Ok(l.clone())
                } else {
                    Err(SemaError {
                        message: format!("type mismatch in arithmetic: {:?} op {:?}", l, r),
                        line: 0, col: 0,
                    })
                }
            }
            BinOp::Eq | BinOp::NotEq => {
                if self.types_compat(l, r) { Ok(Type::Bool) }
                else { Err(SemaError { message: format!("cannot compare {:?} and {:?}", l, r), line: 0, col: 0 }) }
            }
            BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                if self.types_compat(l, r) && (self.is_numeric(l) || matches!(l, Type::String)) {
                    Ok(Type::Bool)
                } else {
                    Err(SemaError { message: format!("cannot order-compare {:?} and {:?}", l, r), line: 0, col: 0 })
                }
            }
            BinOp::And | BinOp::Or => {
                if matches!(l, Type::Bool) && matches!(r, Type::Bool) { Ok(Type::Bool) }
                else { Err(SemaError { message: "'&&'/'||' require bool".into(), line: 0, col: 0 }) }
            }
        }
    }

    fn infer_call(&mut self, func: &Expr, args: &[Expr]) -> Result<Type, SemaError> {
        // ラムダ変数の場合
        let func_ty = self.infer_expr(func)?;
        if let Type::Fn(param_tys, ret) = func_ty {
            if args.len() != param_tys.len() {
                return Err(SemaError {
                    message: format!("expected {} args, got {}", param_tys.len(), args.len()),
                    line: 0, col: 0,
                });
            }
            for (a, e) in args.iter().zip(&param_tys) {
                let at = self.infer_expr(a)?;
                self.unify(e, &at, "argument")?;
            }
            return Ok(*ret);
        }

        // 名前引きでシグネチャを取得
        if let Expr::Ident(name) = func {
            if let Some(sig) = self.fns.get(name).cloned() {
                for a in args { self.infer_expr(a)?; }
                return Ok(sig.ret);
            }
        }

        // メソッド呼び出し (foo.method()) は後でSemaを拡張
        for a in args { self.infer_expr(a)?; }
        Ok(Type::Named("?".into()))
    }

    fn infer_arm(&mut self, arm: &MatchArm) -> Result<Type, SemaError> {
        self.env.push();
        match &arm.pattern {
            Pattern::Some(n)  => self.env.define(n, Type::Named("?".into())),
            Pattern::Ok(n)    => self.env.define(n, Type::Named("?".into())),
            Pattern::Err(n)   => self.env.define(n, Type::Named("Error".into())),
            Pattern::Ident(n) => self.env.define(n, Type::Named("?".into())),
            _ => {}
        }
        let ty = self.infer_expr(&arm.body)?;
        self.env.pop();
        Ok(ty)
    }

    fn infer_field(&self, ty: &Type, name: &str) -> Result<Type, SemaError> {
        if let Type::Named(sname) = ty {
            if let Some(fields) = self.structs.get(sname) {
                return fields.iter().find(|(n, _)| n == name)
                    .map(|(_, t)| t.clone())
                    .ok_or_else(|| SemaError {
                        message: format!("struct '{}' has no field '{}'", sname, name),
                        line: 0, col: 0,
                    });
            }
        }
        Ok(Type::Named("?".into()))
    }

    fn infer_index(&self, ty: &Type) -> Result<Type, SemaError> {
        match ty {
            Type::List(e) | Type::Set(e) => Ok(*e.clone()),
            Type::Dict(_, v)             => Ok(*v.clone()),
            Type::Named(_)               => Ok(Type::Named("?".into())),
            _ => Err(SemaError { message: format!("type {:?} is not indexable", ty), line: 0, col: 0 }),
        }
    }

    fn element_type_of(&self, ty: &Type) -> Result<Type, SemaError> {
        match ty {
            Type::List(e) | Type::Set(e) => Ok(*e.clone()),
            Type::Named(n) if n == "Range" => Ok(Type::Int),
            Type::Named(_) => Ok(Type::Named("?".into())),
            _ => Err(SemaError { message: format!("type {:?} is not iterable", ty), line: 0, col: 0 }),
        }
    }

    /// `?` 型はワイルドカードとして扱い、それ以外はdiscriminantで比較
    fn types_compat(&self, a: &Type, b: &Type) -> bool {
        let is_unknown = |t: &Type| matches!(t, Type::Named(n) if n == "?");
        if is_unknown(a) || is_unknown(b) { return true; }
        std::mem::discriminant(a) == std::mem::discriminant(b)
    }

    fn is_numeric(&self, ty: &Type) -> bool {
        matches!(ty,
            Type::Int | Type::Float
            | Type::I8  | Type::I16 | Type::I32 | Type::I64
            | Type::Uint | Type::U8 | Type::U16 | Type::U32 | Type::U64
            | Type::F32 | Type::F64
        )
    }

    fn unify(&self, expected: &Type, actual: &Type, ctx: &str) -> Result<(), SemaError> {
        if self.types_compat(expected, actual) {
            Ok(())
        } else {
            Err(SemaError {
                message: format!(
                    "type mismatch in {}: expected {:?}, got {:?}", ctx, expected, actual
                ),
                line: 0, col: 0,
            })
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
    use crate::parser::Parser;

    fn analyze(src: &str) -> Result<(), SemaError> {
        let tokens = Lexer::new(src).tokenize().expect("lex failed");
        let ast    = Parser::new(tokens).parse().expect("parse failed");
        SemanticAnalyzer::new().analyze(&ast)
    }

    #[test]
    fn test_simple_fn() {
        assert!(analyze("fn add(x: int, y: int) -> int:\n    return x + y\n").is_ok());
    }

    #[test]
    fn test_type_inference() {
        assert!(analyze("fn main():\n    let x = 42\n    let y = x + 1\n").is_ok());
    }

    #[test]
    fn test_undefined_var() {
        assert!(analyze("fn main():\n    let x = y + 1\n").is_err());
    }

    #[test]
    fn test_type_mismatch_return() {
        let src = "fn get() -> int:\n    return \"hello\"\n";
        assert!(analyze(src).is_err());
    }

    #[test]
    fn test_bool_condition() {
        let src = "fn main():\n    let x = 1\n    if x > 0:\n        let y = 1\n";
        assert!(analyze(src).is_ok());
    }

    #[test]
    fn test_struct_field_access() {
        let src = "struct Point:\n    x: float\n    y: float\nfn main():\n    let p = Point { x: 1.0, y: 2.0 }\n    let v = p.x\n";
        assert!(analyze(src).is_ok());
    }

    #[test]
    fn test_struct_unknown_field() {
        let src = "struct Point:\n    x: float\n    y: float\nfn main():\n    let p = Point { x: 1.0, y: 2.0 }\n    let v = p.z\n";
        assert!(analyze(src).is_err());
    }

    #[test]
    fn test_lambda_type() {
        let src = "fn main():\n    let f = (x: int) -> int => x + 1\n";
        assert!(analyze(src).is_ok());
    }

    #[test]
    fn test_option_some() {
        let src = "fn main():\n    let v = Some(42)\n";
        assert!(analyze(src).is_ok());
    }

    #[test]
    fn test_arithmetic_type_mismatch() {
        let src = "fn main():\n    let x = 1 + \"hello\"\n";
        assert!(analyze(src).is_err());
    }
}
