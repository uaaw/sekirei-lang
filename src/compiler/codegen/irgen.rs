/// sekirei → LLVM IR テキスト生成
///
/// inkwell 不要。生成した .ll を clang/llc に渡して実行する。

use std::collections::HashMap;
use crate::parser::{BinOp, CatchHandler, Expr, MatchArm, Param, Pattern, Stmt, TopLevel, Type, UnOp};

// ============================================================
// LLVM型
// ============================================================

#[derive(Clone, Debug, PartialEq)]
pub enum LlvmTy {
    I1, I64, Double, Ptr, Void,
}

impl LlvmTy {
    pub fn ir(&self) -> &'static str {
        match self {
            LlvmTy::I1     => "i1",
            LlvmTy::I64    => "i64",
            LlvmTy::Double => "double",
            LlvmTy::Ptr    => "i8*",
            LlvmTy::Void   => "void",
        }
    }
    fn zero(&self) -> &'static str {
        match self {
            LlvmTy::I1     => "false",
            LlvmTy::I64    => "0",
            LlvmTy::Double => "0.0",
            LlvmTy::Ptr    => "null",
            LlvmTy::Void   => "undef",
        }
    }
}

pub fn sk_ty(ty: &Type) -> LlvmTy {
    match ty {
        Type::Bool                           => LlvmTy::I1,
        Type::Float | Type::F32 | Type::F64  => LlvmTy::Double,
        Type::String | Type::Str
        | Type::Char  | Type::Byte
        | Type::Nullable(_)                  => LlvmTy::Ptr,
        Type::Void                           => LlvmTy::Void,
        _ /* int 系・Named・etc */           => LlvmTy::I64,
    }
}

// ============================================================
// 生成値
// ============================================================

#[derive(Clone, Debug)]
struct Val {
    reg: String,
    ty:  LlvmTy,
}

impl Val {
    fn new(reg: impl Into<String>, ty: LlvmTy) -> Self {
        Self { reg: reg.into(), ty }
    }
    fn void() -> Self { Self::new("undef", LlvmTy::Void) }
    fn i64(n: i64) -> Self { Self::new(n.to_string(), LlvmTy::I64) }
}

// ============================================================
// IR エミッタ
// ============================================================

/// stdlib モジュールのメソッド → ランタイム関数へのマッピング
fn resolve_method(module: &str, method: &str) -> Option<(&'static str, LlvmTy)> {
    match (module, method) {
        ("io", "print")      => Some(("sk_print",    LlvmTy::Void)),
        ("io", "println")    => Some(("sk_println",  LlvmTy::Void)),
        ("io", "read_line")  => Some(("sk_read_line",LlvmTy::Ptr)),
        ("math", "sqrt")     => Some(("sk_sqrt",     LlvmTy::Double)),
        ("math", "pow")      => Some(("sk_pow",      LlvmTy::Double)),
        ("math", "abs")      => Some(("sk_abs",      LlvmTy::Double)),
        ("math", "floor")    => Some(("sk_floor",    LlvmTy::Double)),
        ("math", "ceil")     => Some(("sk_ceil",     LlvmTy::Double)),
        ("math", "sin")      => Some(("sk_sin",      LlvmTy::Double)),
        ("math", "cos")      => Some(("sk_cos",      LlvmTy::Double)),
        ("string", "len")    => Some(("sk_str_len",  LlvmTy::I64)),
        ("string", "concat") => Some(("sk_str_concat",LlvmTy::Ptr)),
        _ => None,
    }
}

const EXTERN_DECLS: &str = "\
declare void   @sk_runtime_init()
declare void   @sk_runtime_shutdown()
declare void   @sk_print(i8*)
declare void   @sk_println(i8*)
declare i8*    @sk_read_line()
declare i64    @sk_str_len(i8*)
declare i8*    @sk_str_concat(i8*, i8*)
declare i1     @sk_str_eq(i8*, i8*)
declare i8*    @sk_gc_alloc(i64)
declare double @sk_sqrt(double)
declare double @sk_pow(double, double)
declare double @sk_abs(double)
declare double @sk_floor(double)
declare double @sk_ceil(double)
declare double @sk_sin(double)
declare double @sk_cos(double)
";

pub struct IrGen {
    globals:   String,   // グローバル文字列定数
    fn_defs:   String,   // 生成済み関数定義
    fn_buf:    String,   // 現在生成中の関数バッファ
    tmp:       usize,    // SSA レジスタカウンタ
    str_cnt:   usize,    // 文字列定数カウンタ
    lbl:       usize,    // ラベルカウンタ
    vars:      HashMap<String, (String, LlvmTy)>, // 変数 → (alloca ptr, type)
    ret_ty:    LlvmTy,
    terminated: bool,    // 現在ブロックがターミネータで終わった
    // break/continue ターゲット (ループネスト)
    loop_stack: Vec<(String, String)>,  // (check_label, end_label)
}

impl IrGen {
    pub fn new() -> Self {
        Self {
            globals:    String::new(),
            fn_defs:    String::new(),
            fn_buf:     String::new(),
            tmp:        0,
            str_cnt:    0,
            lbl:        0,
            vars:       HashMap::new(),
            ret_ty:     LlvmTy::Void,
            terminated: false,
            loop_stack: Vec::new(),
        }
    }

    // --------------------------------------------------------
    // 公開 API
    // --------------------------------------------------------

    pub fn emit_program(&mut self, ast: &[TopLevel]) -> String {
        // sekirei main → sk_user_main として生成
        // C ランタイムの sk_main_entry が sk_user_main を呼ぶ
        for item in ast {
            self.emit_top(item);
        }

        format!(
            "; sekirei generated LLVM IR\n\
             target triple = \"aarch64-unknown-linux-musl\"\n\n\
             {}\n\
             {}\n\
             {}",
            self.globals,
            EXTERN_DECLS,
            self.fn_defs,
        )
    }

    // --------------------------------------------------------
    // トップレベル
    // --------------------------------------------------------

    fn emit_top(&mut self, item: &TopLevel) {
        match item {
            TopLevel::Fn { name, params, ret, body } => {
                self.emit_fn(name, params, ret.as_ref(), body);
            }
            TopLevel::Impl { methods, .. } => {
                for m in methods { self.emit_top(m); }
            }
            _ => {}
        }
    }

    fn emit_fn(&mut self, name: &str, params: &[Param], ret: Option<&Type>, body: &[Stmt]) {
        self.vars.clear();
        self.fn_buf.clear();
        self.tmp = 0;

        let ll_name = if name == "main" { "sk_user_main" } else { name };
        let ret_ty  = ret.map(sk_ty).unwrap_or(LlvmTy::Void);
        self.ret_ty = ret_ty.clone();

        // 関数シグネチャ
        let params_ir: Vec<String> = params.iter()
            .filter(|p| !p.is_self)
            .map(|p| format!("{} %{}", sk_ty(&p.ty).ir(), p.name))
            .collect();
        self.fn_buf.push_str(&format!(
            "define {} @{}({}) {{\nentry:\n",
            ret_ty.ir(), ll_name, params_ir.join(", ")
        ));

        // パラメータを alloca
        for p in params.iter().filter(|p| !p.is_self) {
            let ty  = sk_ty(&p.ty);
            let ptr = format!("%{}.p", p.name);
            self.i(&format!("{} = alloca {}", ptr, ty.ir()));
            self.i(&format!("store {} %{}, {}* {}", ty.ir(), p.name, ty.ir(), ptr));
            self.vars.insert(p.name.clone(), (ptr, ty));
        }

        self.terminated = false;
        for stmt in body {
            self.emit_stmt(stmt);
        }

        if !self.terminated {
            if matches!(self.ret_ty, LlvmTy::Void) {
                self.i("ret void");
            } else {
                let zero = self.ret_ty.clone().zero();
                let ty   = self.ret_ty.clone();
                self.i(&format!("ret {} {}", ty.ir(), zero));
            }
        }

        self.fn_buf.push_str("}\n\n");
        self.fn_defs.push_str(&self.fn_buf.clone());
    }

    // --------------------------------------------------------
    // 文
    // --------------------------------------------------------

    fn emit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, .. } | Stmt::Mut { name, value, .. } => {
                let v   = self.emit_expr(value);
                let ptr = format!("%{}.p", name);
                self.i(&format!("{} = alloca {}", ptr, v.ty.ir()));
                self.i(&format!("store {} {}, {}* {}", v.ty.ir(), v.reg, v.ty.ir(), ptr));
                self.vars.insert(name.clone(), (ptr, v.ty));
            }

            Stmt::Assign { target, value } => {
                let v = self.emit_expr(value);
                if let Expr::Ident(name) = target {
                    if let Some((ptr, ty)) = self.vars.get(name).cloned() {
                        let cv = self.coerce(v, &ty);
                        self.i(&format!("store {} {}, {}* {}", ty.ir(), cv, ty.ir(), ptr));
                    }
                } else if let Expr::Field { expr, name: field } = target {
                    // TODO: struct field assignment
                }
            }

            Stmt::Return(expr) => {
                let ret_ty = self.ret_ty.clone();
                if let Some(e) = expr {
                    let v  = self.emit_expr(e);
                    let cv = self.coerce(v, &ret_ty);
                    self.i(&format!("ret {} {}", ret_ty.ir(), cv));
                } else {
                    self.i("ret void");
                }
            }

            Stmt::Expr(e) => { self.emit_expr(e); }

            Stmt::For { var, iter, body } => self.emit_for(var, iter, body),

            Stmt::While { cond, body } => {
                let lcheck = self.label("wh_chk");
                let lbody  = self.label("wh_body");
                let lend   = self.label("wh_end");

                self.i(&format!("br label %{}", lcheck));
                self.block(&lcheck);
                let cv = self.emit_expr(cond);
                self.i(&format!("br i1 {}, label %{}, label %{}", cv.reg, lbody, lend));
                self.block(&lbody);

                self.loop_stack.push((lcheck.clone(), lend.clone()));
                for s in body { self.emit_stmt(s); }
                self.loop_stack.pop();

                self.i(&format!("br label %{}", lcheck));
                self.block(&lend);
            }

            Stmt::Loop(body) => {
                let lstart = self.label("loop");
                let lend   = self.label("loop_end");

                self.i(&format!("br label %{}", lstart));
                self.block(&lstart);

                self.loop_stack.push((lstart.clone(), lend.clone()));
                for s in body { self.emit_stmt(s); }
                self.loop_stack.pop();

                self.i(&format!("br label %{}", lstart));
                self.block(&lend);
            }

            Stmt::Break => {
                if let Some((_, lend)) = self.loop_stack.last().cloned() {
                    self.i(&format!("br label %{}", lend));
                    // unreachable ブロックを開く (後続命令のため)
                    let dead = self.label("dead");
                    self.block(&dead);
                }
            }

            Stmt::Continue => {
                if let Some((lcheck, _)) = self.loop_stack.last().cloned() {
                    self.i(&format!("br label %{}", lcheck));
                    let dead = self.label("dead");
                    self.block(&dead);
                }
            }
        }
    }

    fn emit_for(&mut self, var: &str, iter: &Expr, body: &[Stmt]) {
        // range: for i in start..end  (inclusive 未対応は後で)
        let (start_expr, end_expr, inclusive) = match iter {
            Expr::Range { start, end, inclusive } => {
                (start.as_ref(), end.as_ref(), *inclusive)
            }
            _ => {
                // 非range: TODO リストイテレーション
                for s in body { self.emit_stmt(s); }
                return;
            }
        };

        let sv = self.emit_expr(start_expr);
        let ev = self.emit_expr(end_expr);

        // カウンタ変数を alloca
        let ptr = format!("%{}.p", var);
        self.i(&format!("{} = alloca i64", ptr));
        let sv_c = self.coerce(sv, &LlvmTy::I64);
        self.i(&format!("store i64 {}, i64* {}", sv_c, ptr));
        self.vars.insert(var.to_string(), (ptr.clone(), LlvmTy::I64));

        let lcheck = self.label("for_chk");
        let lbody  = self.label("for_body");
        let lend   = self.label("for_end");
        let ev_c   = self.coerce(ev, &LlvmTy::I64);

        // ev の値をローカルに保存（式を2度評価しないため）
        let ev_ptr = format!("%for_ev_{}", self.lbl);
        self.i(&format!("{} = alloca i64", ev_ptr));
        self.i(&format!("store i64 {}, i64* {}", ev_c, ev_ptr));

        self.i(&format!("br label %{}", lcheck));
        self.block(&lcheck);

        let cur = self.tmp();
        self.i(&format!("{} = load i64, i64* {}", cur, ptr));
        let ev_cur = self.tmp();
        self.i(&format!("{} = load i64, i64* {}", ev_cur, ev_ptr));

        let cond = self.tmp();
        let cmp = if inclusive { "icmp sle" } else { "icmp slt" };
        self.i(&format!("{} = {} i64 {}, {}", cond, cmp, cur, ev_cur));
        self.i(&format!("br i1 {}, label %{}, label %{}", cond, lbody, lend));

        self.block(&lbody);
        self.loop_stack.push((lcheck.clone(), lend.clone()));
        for s in body { self.emit_stmt(s); }
        self.loop_stack.pop();

        // インクリメント
        let c2  = self.tmp();
        let inc = self.tmp();
        self.i(&format!("{} = load i64, i64* {}", c2, ptr));
        self.i(&format!("{} = add i64 {}, 1", inc, c2));
        self.i(&format!("store i64 {}, i64* {}", inc, ptr));
        self.i(&format!("br label %{}", lcheck));

        self.block(&lend);
    }

    // --------------------------------------------------------
    // 式
    // --------------------------------------------------------

    fn emit_expr(&mut self, expr: &Expr) -> Val {
        match expr {
            Expr::Int(n)   => Val::i64(*n),
            Expr::Float(f) => Val::new(format!("{:e}", f), LlvmTy::Double),
            Expr::Bool(b)  => Val::new(if *b { "true" } else { "false" }, LlvmTy::I1),
            Expr::Str(s)   => self.emit_str(s),
            Expr::Null | Expr::None => Val::new("null", LlvmTy::Ptr),

            Expr::Ident(name) => {
                if let Some((ptr, ty)) = self.vars.get(name).cloned() {
                    let r = self.tmp();
                    self.i(&format!("{} = load {}, {}* {}", r, ty.ir(), ty.ir(), ptr));
                    Val::new(r, ty)
                } else {
                    // グローバル関数などへの参照
                    Val::new(format!("@{}", name), LlvmTy::Ptr)
                }
            }

            Expr::BinOp { op, lhs, rhs } => self.emit_binop(op, lhs, rhs),

            Expr::UnOp { op, expr } => {
                let v = self.emit_expr(expr);
                let r = self.tmp();
                match op {
                    UnOp::Neg => {
                        if matches!(v.ty, LlvmTy::Double) {
                            self.i(&format!("{} = fneg double {}", r, v.reg));
                        } else {
                            self.i(&format!("{} = sub i64 0, {}", r, v.reg));
                        }
                        Val::new(r, v.ty)
                    }
                    UnOp::Not => {
                        self.i(&format!("{} = xor i1 {}, true", r, v.reg));
                        Val::new(r, LlvmTy::I1)
                    }
                }
            }

            Expr::Call { func, args } => self.emit_call(func, args),

            Expr::If { cond, then, elifs, else_ } => {
                self.emit_if(cond, then, elifs, else_.as_deref())
            }

            Expr::Block(stmts) => {
                for s in stmts {
                    self.emit_stmt(s);
                    if self.terminated { break; }
                }
                Val::void()
            }

            Expr::Field { expr, name } => {
                // TODO: struct GEP
                self.emit_expr(expr);
                Val::new("null", LlvmTy::Ptr)
            }

            Expr::Index { expr, idx } => {
                self.emit_expr(expr);
                self.emit_expr(idx);
                Val::new("null", LlvmTy::Ptr)
            }

            Expr::StructLit { name, fields } => {
                // TODO: GC alloc + GEP
                for (_, v) in fields { self.emit_expr(v); }
                Val::new("null", LlvmTy::Ptr)
            }

            Expr::Match { expr, arms } => self.emit_match(expr, arms),

            Expr::Try(inner) => {
                // ? 演算子: とりあえず中身をそのまま返す (エラー伝播は後で)
                self.emit_expr(inner)
            }

            Expr::Catch { expr, handler } => {
                let v = self.emit_expr(expr);
                match handler {
                    CatchHandler::Default(d)          => { self.emit_expr(d); v }
                    CatchHandler::WithErr(_, body)    => self.emit_expr(body),
                    CatchHandler::WithErrBlock(_, ss) => {
                        for s in ss { self.emit_stmt(s); }
                        Val::void()
                    }
                }
            }

            Expr::TryBlock(stmts) => {
                for s in stmts { self.emit_stmt(s); }
                Val::void()
            }

            Expr::Lambda { .. } => {
                // TODO: クロージャ
                Val::new("null", LlvmTy::Ptr)
            }

            Expr::Some(inner) | Expr::Ok(inner) => self.emit_expr(inner),
            Expr::Err(inner) => { self.emit_expr(inner); Val::new("null", LlvmTy::Ptr) }

            Expr::Range { start, end, .. } => {
                // Range は for ループ内でしか使わない → ここでは無視
                self.emit_expr(start);
                self.emit_expr(end);
                Val::void()
            }
        }
    }

    fn emit_str(&mut self, s: &str) -> Val {
        let escaped = s.chars().flat_map(|c| match c {
            '\n' => vec!['\\', '0', 'A'],
            '\t' => vec!['\\', '0', '9'],
            '"'  => vec!['\\', '2', '2'],
            '\\' => vec!['\\', '5', 'C'],
            c    => vec![c],
        }).collect::<String>();

        let len  = s.len() + 1;
        let name = format!("@.str{}", self.str_cnt);
        self.str_cnt += 1;
        self.globals.push_str(&format!(
            "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"\n",
            name, len, escaped
        ));
        let r = self.tmp();
        self.i(&format!(
            "{} = getelementptr inbounds [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            r, len, len, name
        ));
        Val::new(r, LlvmTy::Ptr)
    }

    fn emit_binop(&mut self, op: &BinOp, lhs: &Expr, rhs: &Expr) -> Val {
        let l = self.emit_expr(lhs);
        let r = self.emit_expr(rhs);
        let reg = self.tmp();
        let is_f = matches!(l.ty, LlvmTy::Double);
        let is_p = matches!(l.ty, LlvmTy::Ptr);

        match op {
            BinOp::Add => {
                if is_p {
                    self.i(&format!("{} = call i8* @sk_str_concat(i8* {}, i8* {})", reg, l.reg, r.reg));
                    Val::new(reg, LlvmTy::Ptr)
                } else if is_f {
                    self.i(&format!("{} = fadd double {}, {}", reg, l.reg, r.reg));
                    Val::new(reg, LlvmTy::Double)
                } else {
                    self.i(&format!("{} = add i64 {}, {}", reg, l.reg, r.reg));
                    Val::new(reg, LlvmTy::I64)
                }
            }
            BinOp::Sub => {
                if is_f { self.i(&format!("{} = fsub double {}, {}", reg, l.reg, r.reg)); Val::new(reg, LlvmTy::Double) }
                else     { self.i(&format!("{} = sub i64 {}, {}", reg, l.reg, r.reg));   Val::new(reg, LlvmTy::I64) }
            }
            BinOp::Mul => {
                if is_f { self.i(&format!("{} = fmul double {}, {}", reg, l.reg, r.reg)); Val::new(reg, LlvmTy::Double) }
                else     { self.i(&format!("{} = mul i64 {}, {}", reg, l.reg, r.reg));   Val::new(reg, LlvmTy::I64) }
            }
            BinOp::Div => {
                if is_f { self.i(&format!("{} = fdiv double {}, {}", reg, l.reg, r.reg)); Val::new(reg, LlvmTy::Double) }
                else     { self.i(&format!("{} = sdiv i64 {}, {}", reg, l.reg, r.reg));   Val::new(reg, LlvmTy::I64) }
            }
            BinOp::Mod => {
                self.i(&format!("{} = srem i64 {}, {}", reg, l.reg, r.reg));
                Val::new(reg, LlvmTy::I64)
            }
            BinOp::Eq => {
                if is_p {
                    self.i(&format!("{} = call i1 @sk_str_eq(i8* {}, i8* {})", reg, l.reg, r.reg));
                } else if is_f {
                    self.i(&format!("{} = fcmp oeq double {}, {}", reg, l.reg, r.reg));
                } else {
                    self.i(&format!("{} = icmp eq i64 {}, {}", reg, l.reg, r.reg));
                }
                Val::new(reg, LlvmTy::I1)
            }
            BinOp::NotEq => {
                if is_f { self.i(&format!("{} = fcmp one double {}, {}", reg, l.reg, r.reg)); }
                else     { self.i(&format!("{} = icmp ne i64 {}, {}", reg, l.reg, r.reg)); }
                Val::new(reg, LlvmTy::I1)
            }
            BinOp::Lt => {
                if is_f { self.i(&format!("{} = fcmp olt double {}, {}", reg, l.reg, r.reg)); }
                else     { self.i(&format!("{} = icmp slt i64 {}, {}", reg, l.reg, r.reg)); }
                Val::new(reg, LlvmTy::I1)
            }
            BinOp::LtEq => {
                if is_f { self.i(&format!("{} = fcmp ole double {}, {}", reg, l.reg, r.reg)); }
                else     { self.i(&format!("{} = icmp sle i64 {}, {}", reg, l.reg, r.reg)); }
                Val::new(reg, LlvmTy::I1)
            }
            BinOp::Gt => {
                if is_f { self.i(&format!("{} = fcmp ogt double {}, {}", reg, l.reg, r.reg)); }
                else     { self.i(&format!("{} = icmp sgt i64 {}, {}", reg, l.reg, r.reg)); }
                Val::new(reg, LlvmTy::I1)
            }
            BinOp::GtEq => {
                if is_f { self.i(&format!("{} = fcmp oge double {}, {}", reg, l.reg, r.reg)); }
                else     { self.i(&format!("{} = icmp sge i64 {}, {}", reg, l.reg, r.reg)); }
                Val::new(reg, LlvmTy::I1)
            }
            BinOp::And => {
                self.i(&format!("{} = and i1 {}, {}", reg, l.reg, r.reg));
                Val::new(reg, LlvmTy::I1)
            }
            BinOp::Or => {
                self.i(&format!("{} = or i1 {}, {}", reg, l.reg, r.reg));
                Val::new(reg, LlvmTy::I1)
            }
        }
    }

    fn emit_call(&mut self, func: &Expr, args: &[Expr]) -> Val {
        let arg_vals: Vec<Val> = args.iter().map(|a| self.emit_expr(a)).collect();

        match func {
            // 直接関数名: print("hello")
            Expr::Ident(name) => self.emit_named_call(name, &arg_vals),

            // メソッド呼び出し: io.println("hello")
            Expr::Field { expr, name: method } => {
                if let Expr::Ident(module) = expr.as_ref() {
                    if let Some((rt_fn, ret_ty)) = resolve_method(module, method) {
                        return self.emit_runtime_call(rt_fn, &arg_vals, ret_ty);
                    }
                }
                // 未解決: 受け手を評価して void を返す
                self.emit_expr(expr);
                Val::void()
            }

            _ => Val::void(),
        }
    }

    fn emit_named_call(&mut self, name: &str, args: &[Val]) -> Val {
        match name {
            "print"  => self.emit_runtime_call("sk_print",   args, LlvmTy::Void),
            "println"=> self.emit_runtime_call("sk_println", args, LlvmTy::Void),
            _ => {
                let arg_ir: Vec<String> = args.iter()
                    .map(|v| format!("{} {}", v.ty.ir(), v.reg))
                    .collect();
                let r = self.tmp();
                self.i(&format!("{} = call i64 @{}({})", r, name, arg_ir.join(", ")));
                Val::new(r, LlvmTy::I64)
            }
        }
    }

    fn emit_runtime_call(&mut self, fn_name: &str, args: &[Val], ret_ty: LlvmTy) -> Val {
        let arg_ir: Vec<String> = args.iter()
            .map(|v| format!("{} {}", v.ty.ir(), v.reg))
            .collect();
        if matches!(ret_ty, LlvmTy::Void) {
            self.i(&format!("call void @{}({})", fn_name, arg_ir.join(", ")));
            Val::void()
        } else {
            let r = self.tmp();
            self.i(&format!("{} = call {} @{}({})", r, ret_ty.ir(), fn_name, arg_ir.join(", ")));
            Val::new(r, ret_ty)
        }
    }

    fn emit_if(
        &mut self,
        cond: &Expr,
        then: &Expr,
        elifs: &[(Expr, Expr)],
        else_: Option<&Expr>,
    ) -> Val {
        let cv     = self.emit_expr(cond);
        let lthen  = self.label("if_then");
        let lend   = self.label("if_end");

        // elif/else がなければ lend に直接
        let lnext = if elifs.is_empty() && else_.is_none() {
            lend.clone()
        } else {
            self.label("if_else")
        };

        self.i(&format!("br i1 {}, label %{}, label %{}", cv.reg, lthen, lnext));
        self.block(&lthen);
        self.emit_expr(then);
        if !self.terminated { self.i(&format!("br label %{}", lend)); }

        // elif チェーン
        let mut cur_else = lnext.clone();
        let remaining: Vec<(&Expr, &Expr)> = elifs.iter().map(|(c, b)| (c, b)).collect();

        if !elifs.is_empty() || else_.is_some() {
            let mut i = 0;
            while i < remaining.len() {
                let (ec, eb) = remaining[i];
                self.block(&cur_else);
                let ecv   = self.emit_expr(ec);
                let ethen = self.label("elif_then");
                let enext = if i + 1 < remaining.len() || else_.is_some() {
                    self.label("elif_else")
                } else {
                    lend.clone()
                };
                self.i(&format!("br i1 {}, label %{}, label %{}", ecv.reg, ethen, enext));
                self.block(&ethen);
                self.emit_expr(eb);
                if !self.terminated { self.i(&format!("br label %{}", lend)); }
                cur_else = enext;
                i += 1;
            }

            if let Some(e) = else_ {
                self.block(&cur_else);
                self.emit_expr(e);
                if !self.terminated { self.i(&format!("br label %{}", lend)); }
            }
        }

        self.block(&lend);
        Val::void()
    }

    fn emit_match(&mut self, expr: &Expr, arms: &[MatchArm]) -> Val {
        let ev  = self.emit_expr(expr);
        let lend = self.label("match_end");

        for arm in arms {
            let lbody = self.label("match_arm");
            let lnext = self.label("match_next");

            match &arm.pattern {
                Pattern::Wildcard | Pattern::Ident(_) => {
                    // 必ずマッチ
                    self.i(&format!("br label %{}", lbody));
                }
                Pattern::Int(n) => {
                    let cmp = self.tmp();
                    let cv  = self.coerce(ev.clone(), &LlvmTy::I64);
                    self.i(&format!("{} = icmp eq i64 {}, {}", cmp, cv, n));
                    self.i(&format!("br i1 {}, label %{}, label %{}", cmp, lbody, lnext));
                }
                Pattern::Bool(b) => {
                    let cmp = self.tmp();
                    let cv  = self.coerce(ev.clone(), &LlvmTy::I1);
                    self.i(&format!("{} = icmp eq i1 {}, {}", cmp, cv, if *b { "true" } else { "false" }));
                    self.i(&format!("br i1 {}, label %{}, label %{}", cmp, lbody, lnext));
                }
                _ => {
                    self.i(&format!("br label %{}", lbody));
                }
            }

            self.block(&lbody);
            self.emit_expr(&arm.body);
            self.i(&format!("br label %{}", lend));

            self.block(&lnext);
        }

        // fallthrough → end
        self.i(&format!("br label %{}", lend));
        self.block(&lend);
        Val::void()
    }

    // --------------------------------------------------------
    // ユーティリティ
    // --------------------------------------------------------

    fn i(&mut self, ir: &str) {
        self.fn_buf.push_str(&format!("  {}\n", ir));
        let t = ir.trim_start();
        self.terminated = t.starts_with("ret ") || t.starts_with("br ") || t.starts_with("unreachable");
    }

    fn block(&mut self, name: &str) {
        self.fn_buf.push_str(&format!("{}:\n", name));
        self.terminated = false;
    }

    fn tmp(&mut self) -> String {
        self.tmp += 1;
        format!("%t{}", self.tmp)
    }

    fn label(&mut self, prefix: &str) -> String {
        self.lbl += 1;
        format!("{}_{}", prefix, self.lbl)
    }

    fn coerce(&mut self, val: Val, target: &LlvmTy) -> String {
        if &val.ty == target { return val.reg; }
        let r = self.tmp();
        match (&val.ty, target) {
            (LlvmTy::I64, LlvmTy::Double) => {
                self.i(&format!("{} = sitofp i64 {} to double", r, val.reg));
            }
            (LlvmTy::Double, LlvmTy::I64) => {
                self.i(&format!("{} = fptosi double {} to i64", r, val.reg));
            }
            (LlvmTy::I64, LlvmTy::I1) => {
                self.i(&format!("{} = icmp ne i64 {}, 0", r, val.reg));
            }
            (LlvmTy::I1, LlvmTy::I64) => {
                self.i(&format!("{} = zext i1 {} to i64", r, val.reg));
            }
            _ => return val.reg,
        }
        r
    }
}
