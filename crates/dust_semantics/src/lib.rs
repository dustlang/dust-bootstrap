// File: lib.rs - This file is part of the DPL Toolchain
// Copyright (c) 2026 Dust LLC, and Contributors
// Description:
//   Semantic analysis for the Dust Programming Language.
//   This module handles:
//     - Regime validation (K, Q, Φ)
//     - Type checking and inference
//     - Effect tracking (emit, memory operations)
//     - Constraint solving
//     - Contract verification

use dust_dir::*;
use dust_frontend::ast::*;
use dust_frontend::{Lexer, Parser};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct CheckError {
    pub message: String,
    pub span: Span,
}

pub fn parse_and_check(source: &str) -> Result<FileAst, CheckError> {
    let toks = Lexer::new(source).lex_all().map_err(|e| CheckError {
        message: format!("lex error: {:?}", e.kind),
        span: e.span,
    })?;

    let mut p = Parser::new(toks);
    let file = p.parse_file().map_err(|e| CheckError {
        message: format!("parse error: {}", e.message),
        span: e.span,
    })?;

    // Minimal structural checks (enough for reference examples)
    check_file(&file)?;

    Ok(file)
}

type ProcKey = (Regime, String);
type BindEdge = (ProcKey, ProcKey);

fn check_file(file: &FileAst) -> Result<(), CheckError> {
    for f in &file.forges {
        if f.node.name.text.is_empty() {
            return Err(CheckError {
                message: "forge name empty".into(),
                span: f.span,
            });
        }

        let (proc_index_by_name, proc_index_keys) = build_proc_index(&f.node.items)?;
        let bind_edges = collect_bind_edges(&f.node.items, &proc_index_by_name, &proc_index_keys)?;

        for item in &f.node.items {
            if let Item::Proc(p) = &item.node {
                if matches!(p.sig.node.path.node.regime.node, Regime::Q) {
                    let has_linear = p
                        .sig
                        .node
                        .qualifiers
                        .iter()
                        .any(|q| matches!(q.node, ProcQualifier::Linear));
                    if !has_linear {
                        return Err(CheckError {
                            message: "Q-regime proc must be declared linear in v0.1".into(),
                            span: p.sig.span,
                        });
                    }
                }

                check_proc_semantics(p)?;
                check_phi_effect_witness_rule(p)?;
                check_cross_regime_bind_rules(
                    p,
                    &proc_index_by_name,
                    &proc_index_keys,
                    &bind_edges,
                )?;
            }
        }
    }
    Ok(())
}

fn build_proc_index(
    items: &[Spanned<Item>],
) -> Result<(HashMap<String, Vec<Regime>>, HashSet<ProcKey>), CheckError> {
    let mut by_name: HashMap<String, Vec<Regime>> = HashMap::new();
    let mut keys: HashSet<ProcKey> = HashSet::new();

    for item in items {
        let Item::Proc(proc) = &item.node else {
            continue;
        };
        let regime = proc.sig.node.path.node.regime.node;
        let name = proc.sig.node.path.node.name.text.clone();
        let key = (regime, name.clone());
        if !keys.insert(key.clone()) {
            return Err(CheckError {
                message: format!("duplicate proc declaration '{}'", format_proc_key(&key)),
                span: proc.sig.node.path.node.name.span,
            });
        }
        by_name.entry(name).or_default().push(regime);
    }

    Ok((by_name, keys))
}

fn collect_bind_edges(
    items: &[Spanned<Item>],
    proc_index_by_name: &HashMap<String, Vec<Regime>>,
    proc_index_keys: &HashSet<ProcKey>,
) -> Result<HashSet<BindEdge>, CheckError> {
    let mut edges: HashSet<BindEdge> = HashSet::new();

    for item in items {
        let Item::Bind(bind) = &item.node else {
            continue;
        };
        for clause in &bind.contract.node.clauses {
            if clause.node.key.text.is_empty() {
                return Err(CheckError {
                    message: "empty contract key".into(),
                    span: clause.span,
                });
            }
        }
        let source = resolve_bind_proc_ref(
            &bind.source.node,
            bind.source.span,
            proc_index_by_name,
            proc_index_keys,
        )?;
        let target = resolve_bind_proc_ref(
            &bind.target.node,
            bind.target.span,
            proc_index_by_name,
            proc_index_keys,
        )?;
        edges.insert((source, target));
    }

    Ok(edges)
}

fn resolve_bind_proc_ref(
    proc_ref: &ProcPathRef,
    span: Span,
    proc_index_by_name: &HashMap<String, Vec<Regime>>,
    proc_index_keys: &HashSet<ProcKey>,
) -> Result<ProcKey, CheckError> {
    match proc_ref {
        ProcPathRef::Qualified(path) => {
            let key = (path.regime.node, path.name.text.clone());
            if !proc_index_keys.contains(&key) {
                return Err(CheckError {
                    message: format!("bind references unknown proc '{}'", format_proc_key(&key)),
                    span,
                });
            }
            Ok(key)
        }
        ProcPathRef::Unqualified(id) => {
            let Some(regimes) = proc_index_by_name.get(&id.text) else {
                return Err(CheckError {
                    message: format!("bind references unknown proc '{}'", id.text),
                    span: id.span,
                });
            };
            if regimes.len() == 1 {
                return Ok((regimes[0], id.text.clone()));
            }
            Err(CheckError {
                message: format!(
                    "ambiguous unqualified proc reference '{}'; add regime qualification",
                    id.text
                ),
                span: id.span,
            })
        }
    }
}

fn check_phi_effect_witness_rule(proc: &ProcDecl) -> Result<(), CheckError> {
    if !matches!(proc.sig.node.path.node.regime.node, Regime::Phi) {
        return Ok(());
    }

    let mut has_effect = false;
    let mut has_prove = false;
    scan_block_features(&proc.body.node, &mut has_effect, &mut has_prove);
    if has_effect && !has_prove {
        return Err(CheckError {
            message: "Φ-regime process with effects must produce at least one witness via `prove`"
                .into(),
            span: proc.sig.span,
        });
    }

    Ok(())
}

fn scan_block_features(block: &Block, has_effect: &mut bool, has_prove: &mut bool) {
    for stmt in &block.stmts {
        scan_stmt_features(&stmt.node, has_effect, has_prove);
    }
}

fn scan_stmt_features(stmt: &Stmt, has_effect: &mut bool, has_prove: &mut bool) {
    match stmt {
        Stmt::Let(s) => scan_expr_features(&s.expr.node, has_effect, has_prove),
        Stmt::MutLet(s) => scan_expr_features(&s.expr.node, has_effect, has_prove),
        Stmt::Assign(s) => scan_expr_features(&s.expr.node, has_effect, has_prove),
        Stmt::Constrain(s) => scan_expr_features(&s.predicate.node, has_effect, has_prove),
        Stmt::Prove(s) => {
            *has_prove = true;
            scan_expr_features(&s.from.node, has_effect, has_prove);
        }
        Stmt::Effect(s) => {
            *has_effect = true;
            scan_expr_features(&s.payload.node, has_effect, has_prove);
        }
        Stmt::Return(s) => scan_expr_features(&s.expr.node, has_effect, has_prove),
        Stmt::Expr(s) => scan_expr_features(&s.expr.node, has_effect, has_prove),
        Stmt::If(s) => {
            scan_expr_features(&s.condition.node, has_effect, has_prove);
            scan_block_features(&s.then_block.node, has_effect, has_prove);
            if let Some(else_block) = &s.else_block {
                scan_block_features(&else_block.node, has_effect, has_prove);
            }
        }
        Stmt::For(s) => {
            scan_expr_features(&s.start.node, has_effect, has_prove);
            scan_expr_features(&s.end.node, has_effect, has_prove);
            scan_block_features(&s.body.node, has_effect, has_prove);
        }
        Stmt::While(s) => {
            scan_expr_features(&s.condition.node, has_effect, has_prove);
            scan_block_features(&s.body.node, has_effect, has_prove);
        }
        Stmt::Break(_) | Stmt::Continue(_) => {}
        // v0.2 statements - all have effects (Spanned wrapped)
        Stmt::Alloc(s) => scan_expr_features(&s.size.node, has_effect, has_prove),
        Stmt::Free(s) => scan_expr_features(&s.expr.node, has_effect, has_prove),
        Stmt::Spawn(s) => {
            scan_expr_features(&s.callee.node, has_effect, has_prove);
            if let Some(seed) = &s.seed {
                scan_expr_features(&seed.node, has_effect, has_prove);
            }
        }
        Stmt::Join(s) => scan_expr_features(&s.thread.node, has_effect, has_prove),
        Stmt::MutexNew(_) | Stmt::MutexLock(_) | Stmt::MutexUnlock(_) => *has_effect = true,
        Stmt::Open(s) => {
            scan_expr_features(&s.path.node, has_effect, has_prove);
            scan_expr_features(&s.mode.node, has_effect, has_prove);
        }
        Stmt::Read(s) => {
            scan_expr_features(&s.file.node, has_effect, has_prove);
            scan_expr_features(&s.buffer.node, has_effect, has_prove);
            scan_expr_features(&s.n.node, has_effect, has_prove);
        }
        Stmt::Write(s) => {
            scan_expr_features(&s.file.node, has_effect, has_prove);
            scan_expr_features(&s.buffer.node, has_effect, has_prove);
            scan_expr_features(&s.n.node, has_effect, has_prove);
        }
        Stmt::Close(s) => scan_expr_features(&s.file.node, has_effect, has_prove),
        // v0.2 Device I/O
        Stmt::IoRead(s) => scan_expr_features(&s.port.node, has_effect, has_prove),
        Stmt::IoWrite(s) => {
            scan_expr_features(&s.port.node, has_effect, has_prove);
            scan_expr_features(&s.value.node, has_effect, has_prove);
        }
        Stmt::MmioRead(s) => scan_expr_features(&s.addr.node, has_effect, has_prove),
        Stmt::MmioWrite(s) => {
            scan_expr_features(&s.addr.node, has_effect, has_prove);
            scan_expr_features(&s.value.node, has_effect, has_prove);
        }
        Stmt::Unsafe(s) => scan_block_features(&s.body, has_effect, has_prove),
    }
}

fn scan_expr_features(expr: &Expr, has_effect: &mut bool, has_prove: &mut bool) {
    match expr {
        Expr::Literal(_) | Expr::Ident(_) => {}
        Expr::Paren(inner) => scan_expr_features(&inner.node, has_effect, has_prove),
        Expr::Block(block) => scan_block_features(block, has_effect, has_prove),
        Expr::Binary(bin) => {
            scan_expr_features(&bin.node.lhs.node, has_effect, has_prove);
            scan_expr_features(&bin.node.rhs.node, has_effect, has_prove);
        }
        Expr::Unary(unary) => scan_expr_features(&unary.node.operand.node, has_effect, has_prove),
        Expr::Call(call) => {
            scan_expr_features(&call.node.callee.node, has_effect, has_prove);
            for arg in &call.node.args {
                scan_expr_features(&arg.node, has_effect, has_prove);
            }
        }
        Expr::Field(field) => scan_expr_features(&field.node.base.node, has_effect, has_prove),
        Expr::Index(index) => {
            scan_expr_features(&index.node.base.node, has_effect, has_prove);
            scan_expr_features(&index.node.index.node, has_effect, has_prove);
        }
        Expr::Array(elements) => {
            for e in elements {
                scan_expr_features(&e.node, has_effect, has_prove);
            }
        }
        Expr::StructLit(struct_lit) => {
            for init in &struct_lit.node.inits {
                scan_expr_features(&init.node.expr.node, has_effect, has_prove);
            }
        }
        Expr::Match(m) => {
            scan_expr_features(&m.node.expr.node, has_effect, has_prove);
            for arm in &m.node.arms {
                scan_expr_features(&arm.node.body.node, has_effect, has_prove);
            }
        }
        // v0.2 expressions - use ** to deref the box then access
        Expr::Alloc(a) => {
            let inner = &**a;
            scan_expr_features(&inner.size.node, has_effect, has_prove);
        }
        Expr::Spawn(s) => {
            let inner = &**s;
            scan_expr_features(&inner.callee.node, has_effect, has_prove);
            if let Some(seed) = &inner.seed {
                scan_expr_features(&seed.node, has_effect, has_prove);
            }
        }
        Expr::Join(j) => {
            let inner = &**j;
            scan_expr_features(&inner.thread.node, has_effect, has_prove);
        }
        Expr::MutexNew(_) => {}
    }
}

fn check_cross_regime_bind_rules(
    proc: &ProcDecl,
    proc_index_by_name: &HashMap<String, Vec<Regime>>,
    proc_index_keys: &HashSet<ProcKey>,
    bind_edges: &HashSet<BindEdge>,
) -> Result<(), CheckError> {
    let caller = (
        proc.sig.node.path.node.regime.node,
        proc.sig.node.path.node.name.text.clone(),
    );
    let mut calls = Vec::new();
    collect_calls_in_block(&proc.body.node, &mut calls);

    for (callee, span) in calls {
        let Some(target) =
            resolve_call_target(&callee, caller.0, proc_index_by_name, proc_index_keys)
        else {
            continue;
        };
        if target.0 == caller.0 {
            continue;
        }
        let direct = (caller.clone(), target.clone());
        let reverse = (target.clone(), caller.clone());
        if !bind_edges.contains(&direct) && !bind_edges.contains(&reverse) {
            return Err(CheckError {
                message: format!(
                    "cross-regime call '{}' -> '{}' requires explicit bind declaration",
                    format_proc_key(&caller),
                    format_proc_key(&target)
                ),
                span,
            });
        }
    }

    Ok(())
}

fn collect_calls_in_block(block: &Block, out: &mut Vec<(String, Span)>) {
    for stmt in &block.stmts {
        collect_calls_in_stmt(&stmt.node, out);
    }
}

fn collect_calls_in_stmt(stmt: &Stmt, out: &mut Vec<(String, Span)>) {
    match stmt {
        Stmt::Let(s) => collect_calls_in_expr(&s.expr.node, out),
        Stmt::MutLet(s) => collect_calls_in_expr(&s.expr.node, out),
        Stmt::Assign(s) => collect_calls_in_expr(&s.expr.node, out),
        Stmt::Constrain(s) => collect_calls_in_expr(&s.predicate.node, out),
        Stmt::Prove(s) => collect_calls_in_expr(&s.from.node, out),
        Stmt::Effect(s) => collect_calls_in_expr(&s.payload.node, out),
        Stmt::Return(s) => collect_calls_in_expr(&s.expr.node, out),
        Stmt::Expr(s) => collect_calls_in_expr(&s.expr.node, out),
        Stmt::If(s) => {
            collect_calls_in_expr(&s.condition.node, out);
            collect_calls_in_block(&s.then_block.node, out);
            if let Some(else_block) = &s.else_block {
                collect_calls_in_block(&else_block.node, out);
            }
        }
        Stmt::For(s) => {
            collect_calls_in_expr(&s.start.node, out);
            collect_calls_in_expr(&s.end.node, out);
            collect_calls_in_block(&s.body.node, out);
        }
        Stmt::While(s) => {
            collect_calls_in_block(&s.body.node, out);
        }
        Stmt::Break(_) | Stmt::Continue(_) => {}
        // v0.2 statements - collect calls from expressions
        Stmt::Alloc(s) => collect_calls_in_expr(&s.size.node, out),
        Stmt::Free(s) => collect_calls_in_expr(&s.expr.node, out),
        Stmt::Spawn(s) => {
            collect_calls_in_expr(&s.callee.node, out);
            if let Some(seed) = &s.seed {
                collect_calls_in_expr(&seed.node, out);
            }
        }
        Stmt::Join(s) => collect_calls_in_expr(&s.thread.node, out),
        Stmt::MutexNew(_) | Stmt::MutexLock(_) | Stmt::MutexUnlock(_) => {}
        Stmt::Open(s) => {
            collect_calls_in_expr(&s.path.node, out);
            collect_calls_in_expr(&s.mode.node, out);
        }
        Stmt::Read(s) => {
            collect_calls_in_expr(&s.file.node, out);
            collect_calls_in_expr(&s.buffer.node, out);
            collect_calls_in_expr(&s.n.node, out);
        }
        Stmt::Write(s) => {
            collect_calls_in_expr(&s.file.node, out);
            collect_calls_in_expr(&s.buffer.node, out);
            collect_calls_in_expr(&s.n.node, out);
        }
        Stmt::Close(s) => collect_calls_in_expr(&s.file.node, out),
        // v0.2 Device I/O
        Stmt::IoRead(s) => collect_calls_in_expr(&s.port.node, out),
        Stmt::IoWrite(s) => {
            collect_calls_in_expr(&s.port.node, out);
            collect_calls_in_expr(&s.value.node, out);
        }
        Stmt::MmioRead(s) => collect_calls_in_expr(&s.addr.node, out),
        Stmt::MmioWrite(s) => {
            collect_calls_in_expr(&s.addr.node, out);
            collect_calls_in_expr(&s.value.node, out);
        }
        Stmt::Unsafe(s) => collect_calls_in_block(&s.body, out),
    }
}

fn collect_calls_in_expr(expr: &Expr, out: &mut Vec<(String, Span)>) {
    match expr {
        Expr::Literal(_) | Expr::Ident(_) => {}
        Expr::Paren(inner) => collect_calls_in_expr(&inner.node, out),
        Expr::Block(block) => collect_calls_in_block(block, out),
        Expr::Binary(bin) => {
            collect_calls_in_expr(&bin.node.lhs.node, out);
            collect_calls_in_expr(&bin.node.rhs.node, out);
        }
        Expr::Unary(unary) => collect_calls_in_expr(&unary.node.operand.node, out),
        Expr::Call(call) => {
            if let Expr::Ident(id) = &call.node.callee.node {
                out.push((id.text.clone(), id.span));
            }
            collect_calls_in_expr(&call.node.callee.node, out);
            for arg in &call.node.args {
                collect_calls_in_expr(&arg.node, out);
            }
        }
        Expr::Field(field) => collect_calls_in_expr(&field.node.base.node, out),
        Expr::Index(index) => {
            collect_calls_in_expr(&index.node.base.node, out);
            collect_calls_in_expr(&index.node.index.node, out);
        }
        Expr::Array(elements) => {
            for e in elements {
                collect_calls_in_expr(&e.node, out);
            }
        }
        Expr::StructLit(struct_lit) => {
            for init in &struct_lit.node.inits {
                collect_calls_in_expr(&init.node.expr.node, out);
            }
        }
        Expr::Match(m) => {
            collect_calls_in_expr(&m.node.expr.node, out);
            for arm in &m.node.arms {
                collect_calls_in_expr(&arm.node.body.node, out);
            }
        }
        Expr::Alloc(a) => collect_calls_in_expr(&a.size.node, out),
        Expr::Spawn(s) => {
            collect_calls_in_expr(&s.callee.node, out);
            if let Some(seed) = &s.seed {
                collect_calls_in_expr(&seed.node, out);
            }
        }
        Expr::Join(j) => collect_calls_in_expr(&j.thread.node, out),
        Expr::MutexNew(_) => {}
    }
}

fn resolve_call_target(
    callee: &str,
    caller_regime: Regime,
    proc_index_by_name: &HashMap<String, Vec<Regime>>,
    proc_index_keys: &HashSet<ProcKey>,
) -> Option<ProcKey> {
    if let Some((prefix, name)) = callee.split_once("::") {
        let regime = parse_regime_segment(prefix)?;
        let key = (regime, name.to_string());
        if proc_index_keys.contains(&key) {
            return Some(key);
        }
        return None;
    }

    let regimes = proc_index_by_name.get(callee)?;
    if regimes.contains(&caller_regime) {
        return Some((caller_regime, callee.to_string()));
    }
    if regimes.len() == 1 {
        return Some((regimes[0], callee.to_string()));
    }
    None
}

fn parse_regime_segment(prefix: &str) -> Option<Regime> {
    match prefix {
        "K" => Some(Regime::K),
        "Q" => Some(Regime::Q),
        "Φ" | "Phi" => Some(Regime::Phi),
        _ => None,
    }
}

fn format_proc_key(key: &ProcKey) -> String {
    format!("{}::{}", regime_label(key.0), key.1)
}

fn regime_label(regime: Regime) -> &'static str {
    match regime {
        Regime::K => "K",
        Regime::Q => "Q",
        Regime::Phi => "Φ",
    }
}

fn check_proc_semantics(proc: &ProcDecl) -> Result<(), CheckError> {
    let mut scopes: Vec<HashMap<String, bool>> = vec![HashMap::new()];
    let in_phi_regime = matches!(proc.sig.node.path.node.regime.node, Regime::Phi);

    for param in &proc.sig.node.params {
        scopes[0].insert(param.node.name.text.clone(), false);
    }

    check_block_semantics(&proc.body.node, &mut scopes, false, in_phi_regime)
}

fn check_block_semantics(
    block: &Block,
    scopes: &mut Vec<HashMap<String, bool>>,
    in_loop: bool,
    in_phi_regime: bool,
) -> Result<(), CheckError> {
    scopes.push(HashMap::new());
    for stmt in &block.stmts {
        check_stmt_semantics(stmt, scopes, in_loop, in_phi_regime)?;
    }
    scopes.pop();
    Ok(())
}

fn check_stmt_semantics(
    stmt: &Spanned<Stmt>,
    scopes: &mut Vec<HashMap<String, bool>>,
    in_loop: bool,
    in_phi_regime: bool,
) -> Result<(), CheckError> {
    match &stmt.node {
        Stmt::Let(s) => {
            if scopes
                .last()
                .map(|scope| scope.contains_key(&s.name.text))
                .unwrap_or(false)
            {
                return Err(CheckError {
                    message: format!("duplicate local binding '{}'", s.name.text),
                    span: s.name.span,
                });
            }
            if let Some(scope) = scopes.last_mut() {
                scope.insert(s.name.text.clone(), false);
            }
        }
        Stmt::MutLet(s) => {
            if scopes
                .last()
                .map(|scope| scope.contains_key(&s.name.text))
                .unwrap_or(false)
            {
                return Err(CheckError {
                    message: format!("duplicate local binding '{}'", s.name.text),
                    span: s.name.span,
                });
            }
            if let Some(scope) = scopes.last_mut() {
                scope.insert(s.name.text.clone(), true);
            }
        }
        Stmt::Assign(s) => {
            let mut found: Option<bool> = None;
            for scope in scopes.iter().rev() {
                if let Some(is_mutable) = scope.get(&s.target.text) {
                    found = Some(*is_mutable);
                    break;
                }
            }
            match found {
                None => {
                    return Err(CheckError {
                        message: format!("assignment to unknown variable '{}'", s.target.text),
                        span: s.target.span,
                    });
                }
                Some(false) => {
                    return Err(CheckError {
                        message: format!(
                            "assignment to immutable binding '{}' (use `mut let`)",
                            s.target.text
                        ),
                        span: s.target.span,
                    });
                }
                Some(true) => {}
            }
        }
        Stmt::If(s) => {
            check_block_semantics(&s.then_block.node, scopes, in_loop, in_phi_regime)?;
            if let Some(else_block) = &s.else_block {
                check_block_semantics(&else_block.node, scopes, in_loop, in_phi_regime)?;
            }
        }
        Stmt::For(s) => {
            scopes.push(HashMap::new());
            if let Some(scope) = scopes.last_mut() {
                scope.insert(s.var.text.clone(), true);
            }
            check_block_semantics(&s.body.node, scopes, true, in_phi_regime)?;
            scopes.pop();
        }
        Stmt::While(s) => {
            check_block_semantics(&s.body.node, scopes, true, in_phi_regime)?;
        }
        Stmt::Break(_) | Stmt::Continue(_) => {
            if !in_loop {
                return Err(CheckError {
                    message: "loop control statement outside loop".into(),
                    span: stmt.span,
                });
            }
        }
        Stmt::Constrain(_) => {
            if !in_phi_regime {
                return Err(CheckError {
                    message: "`constrain` is only permitted in Φ-regime processes".into(),
                    span: stmt.span,
                });
            }
        }
        Stmt::Prove(s) => {
            if !in_phi_regime {
                return Err(CheckError {
                    message: "`prove` is only permitted in Φ-regime processes".into(),
                    span: stmt.span,
                });
            }
            if scopes
                .last()
                .map(|scope| scope.contains_key(&s.name.text))
                .unwrap_or(false)
            {
                return Err(CheckError {
                    message: format!("duplicate local binding '{}'", s.name.text),
                    span: s.name.span,
                });
            }
            if let Some(scope) = scopes.last_mut() {
                scope.insert(s.name.text.clone(), false);
            }
        }
        Stmt::Effect(_) | Stmt::Return(_) | Stmt::Expr(_) => {}
        // v0.2 statements - skip deep semantic checking for now
        Stmt::Alloc(_) | Stmt::Free(_) | Stmt::Spawn(_) | Stmt::Join(_) => {}
        Stmt::MutexNew(_) | Stmt::MutexLock(_) | Stmt::MutexUnlock(_) => {}
        Stmt::Open(_) | Stmt::Read(_) | Stmt::Write(_) | Stmt::Close(_) => {}
        Stmt::IoRead(_) | Stmt::IoWrite(_) | Stmt::MmioRead(_) | Stmt::MmioWrite(_) => {}
        Stmt::Unsafe(_) => {}
    }
    Ok(())
}

pub fn lower_to_dir(file: &FileAst) -> DirProgram {
    let mut forges = Vec::new();

    for f in &file.forges {
        let mut forge_consts: HashMap<String, String> = HashMap::new();
        for item in &f.node.items {
            if let Item::Const(c) = &item.node {
                forge_consts.insert(c.name.text.clone(), literal_to_string(&c.value.node));
            }
        }

        let mut shapes = Vec::new();
        let mut procs = Vec::new();
        let mut binds = Vec::new();

        for item in &f.node.items {
            match &item.node {
                Item::Shape(s) => {
                    shapes.push(DirShape {
                        name: s.name.text.clone(),
                        fields: s
                            .fields
                            .iter()
                            .map(|fld| DirField {
                                name: fld.node.name.text.clone(),
                                ty: type_to_string(&fld.node.ty.node),
                            })
                            .collect(),
                    });
                }
                Item::Proc(p) => {
                    procs.push(lower_proc(p, &forge_consts));
                }
                Item::Bind(b) => {
                    binds.push(DirBind {
                        source: proc_ref_to_string(&b.source.node),
                        target: proc_ref_to_string(&b.target.node),
                        contract: b
                            .contract
                            .node
                            .clauses
                            .iter()
                            .map(|cl| DirClause {
                                key: cl.node.key.text.clone(),
                                op: contract_op_to_string(&cl.node.op.node),
                                value: contract_value_to_string(&cl.node.value.node, &forge_consts),
                            })
                            .collect(),
                    });
                }
                Item::Const(_) => {
                    // Constants are inlined at compile time.
                }
            }
        }

        // stable ordering for deterministic output
        shapes.sort_by(|a, b| a.name.cmp(&b.name));
        procs.sort_by(|a, b| {
            (a.regime.clone(), a.name.clone()).cmp(&(b.regime.clone(), b.name.clone()))
        });
        binds.sort_by(|a, b| {
            (a.source.clone(), a.target.clone()).cmp(&(b.source.clone(), b.target.clone()))
        });

        forges.push(DirForge {
            name: f.node.name.text.clone(),
            shapes,
            procs,
            binds,
        });
    }

    forges.sort_by(|a, b| a.name.cmp(&b.name));
    DirProgram {
        forges,
        types: Vec::new(), // v0.2: type definitions
    }
}

fn lower_proc(p: &ProcDecl, consts: &HashMap<String, String>) -> DirProc {
    let regime = match p.sig.node.path.node.regime.node {
        Regime::K => "K",
        Regime::Q => "Q",
        Regime::Phi => "Φ",
    }
    .to_string();

    let name = p.sig.node.path.node.name.text.clone();

    let params = p
        .sig
        .node
        .params
        .iter()
        .map(|pr| DirParam {
            name: pr.node.name.text.clone(),
            ty: type_to_string(&pr.node.ty.node),
        })
        .collect();

    let uses = p
        .sig
        .node
        .uses
        .iter()
        .map(|u| DirUses {
            resource: u.node.resource.text.clone(),
            args: u
                .node
                .args
                .iter()
                .map(|a| {
                    let k = a.node.key.text.clone();
                    let v = match &a.node.value.node {
                        Literal::Int(n) => DirLit::Int(*n),
                        Literal::Float(f) => DirLit::Float(*f),
                        Literal::Bool(b) => DirLit::Bool(*b),
                        Literal::String(s) => DirLit::String(s.clone()),
                        Literal::Char(c) => DirLit::Char(*c),
                    };
                    (k, v)
                })
                .collect(),
        })
        .collect();

    let ret = p.sig.node.ret.as_ref().map(|t| type_to_string(&t.node));
    let qualifiers = p
        .sig
        .node
        .qualifiers
        .iter()
        .map(|q| match q.node {
            ProcQualifier::Linear => "linear".to_string(),
        })
        .collect();

    let body = p
        .body
        .node
        .stmts
        .iter()
        .map(|s| lower_stmt(&s.node, consts))
        .collect();

    DirProc {
        regime,
        name,
        params,
        uses,
        ret,
        qualifiers,
        body,
        locals: Vec::new(), // v0.2: local variables
    }
}

fn lower_stmt(s: &Stmt, consts: &HashMap<String, String>) -> DirStmt {
    match s {
        Stmt::Let(x) => {
            if let Some((target, args)) = expr_to_call(&x.expr.node, consts) {
                DirStmt::Call {
                    target,
                    args,
                    result: Some(x.name.text.clone()),
                }
            } else {
                DirStmt::Let {
                    name: x.name.text.clone(),
                    expr: expr_to_string(&x.expr.node, consts),
                }
            }
        }
        Stmt::MutLet(x) => {
            if let Some((target, args)) = expr_to_call(&x.expr.node, consts) {
                DirStmt::Call {
                    target,
                    args,
                    result: Some(x.name.text.clone()),
                }
            } else {
                DirStmt::Let {
                    name: x.name.text.clone(),
                    expr: expr_to_string(&x.expr.node, consts),
                }
            }
        }
        Stmt::Assign(x) => DirStmt::Assign {
            target: x.target.text.clone(),
            expr: expr_to_string(&x.expr.node, consts),
        },
        Stmt::Constrain(x) => DirStmt::Constrain {
            predicate: expr_to_string(&x.predicate.node, consts),
        },
        Stmt::Prove(x) => DirStmt::Prove {
            name: x.name.text.clone(),
            from: expr_to_string(&x.from.node, consts),
        },
        Stmt::Effect(x) => DirStmt::Effect {
            kind: match x.kind.node {
                EffectKind::Observe => "observe",
                EffectKind::Emit => "emit",
                EffectKind::Seal => "seal",
            }
            .into(),
            payload: expr_to_string(&x.payload.node, consts),
        },
        Stmt::Return(x) => DirStmt::Return {
            expr: Some(expr_to_string(&x.expr.node, consts)),
        },
        Stmt::If(x) => DirStmt::If {
            condition: expr_to_string(&x.condition.node, consts),
            then_body: x
                .then_block
                .node
                .stmts
                .iter()
                .map(|s| lower_stmt(&s.node, consts))
                .collect(),
            else_body: x.else_block.as_ref().map(|b| {
                b.node
                    .stmts
                    .iter()
                    .map(|s| lower_stmt(&s.node, consts))
                    .collect()
            }),
        },
        Stmt::For(x) => DirStmt::For {
            var: x.var.text.clone(),
            start: expr_to_string(&x.start.node, consts),
            end: expr_to_string(&x.end.node, consts),
            body: x
                .body
                .node
                .stmts
                .iter()
                .map(|s| lower_stmt(&s.node, consts))
                .collect(),
        },
        Stmt::While(x) => DirStmt::While {
            condition: expr_to_string(&x.condition.node, consts),
            body: x
                .body
                .node
                .stmts
                .iter()
                .map(|s| lower_stmt(&s.node, consts))
                .collect(),
        },
        Stmt::Break(_) => DirStmt::Break,
        Stmt::Continue(_) => DirStmt::Continue,
        // v0.2 statements
        Stmt::Alloc(s) => DirStmt::Effect {
            kind: "alloc".into(),
            payload: expr_to_string(&s.size.node, consts),
        },
        Stmt::Free(s) => DirStmt::Effect {
            kind: "free".into(),
            payload: expr_to_string(&s.expr.node, consts),
        },
        Stmt::Spawn(s) => DirStmt::Effect {
            kind: "spawn".into(),
            payload: expr_to_string(&s.callee.node, consts),
        },
        Stmt::Join(s) => DirStmt::Effect {
            kind: "join".into(),
            payload: expr_to_string(&s.thread.node, consts),
        },
        Stmt::MutexNew(_) => DirStmt::Effect {
            kind: "mutex_new".into(),
            payload: "".into(),
        },
        Stmt::MutexLock(s) => DirStmt::Effect {
            kind: "mutex_lock".into(),
            payload: expr_to_string(&s.mutex.node, consts),
        },
        Stmt::MutexUnlock(s) => DirStmt::Effect {
            kind: "mutex_unlock".into(),
            payload: expr_to_string(&s.mutex.node, consts),
        },
        Stmt::Open(s) => DirStmt::Effect {
            kind: "open".into(),
            payload: expr_to_string(&s.path.node, consts),
        },
        Stmt::Read(s) => DirStmt::Effect {
            kind: "read".into(),
            payload: expr_to_string(&s.file.node, consts),
        },
        Stmt::Write(s) => DirStmt::Effect {
            kind: "write".into(),
            payload: expr_to_string(&s.file.node, consts),
        },
        Stmt::Close(s) => DirStmt::Effect {
            kind: "close".into(),
            payload: expr_to_string(&s.file.node, consts),
        },
        // v0.2 Device I/O
        Stmt::IoRead(s) => DirStmt::Effect {
            kind: "io_read".into(),
            payload: expr_to_string(&s.port.node, consts),
        },
        Stmt::IoWrite(s) => DirStmt::Effect {
            kind: "io_write".into(),
            payload: format!(
                "{}:{}",
                expr_to_string(&s.port.node, consts),
                expr_to_string(&s.value.node, consts)
            ),
        },
        Stmt::MmioRead(s) => DirStmt::Effect {
            kind: "mmio_read".into(),
            payload: expr_to_string(&s.addr.node, consts),
        },
        Stmt::MmioWrite(s) => DirStmt::Effect {
            kind: "mmio_write".into(),
            payload: format!(
                "{}:{}",
                expr_to_string(&s.addr.node, consts),
                expr_to_string(&s.value.node, consts)
            ),
        },
        Stmt::Unsafe(s) => DirStmt::Effect {
            kind: "unsafe".into(),
            payload: format!(
                "{:?}",
                s.body
                    .stmts
                    .iter()
                    .map(|st| lower_stmt(&st.node, consts))
                    .collect::<Vec<_>>()
            ),
        },
        Stmt::Expr(x) => {
            if let Some((target, args)) = expr_to_call(&x.expr.node, consts) {
                DirStmt::Call {
                    target,
                    args,
                    result: None,
                }
            } else {
                DirStmt::Effect {
                    kind: "expr".into(),
                    payload: expr_to_string(&x.expr.node, consts),
                }
            }
        }
    }
}

fn expr_to_call(e: &Expr, consts: &HashMap<String, String>) -> Option<(String, Vec<String>)> {
    if let Expr::Call(c) = e {
        let target = expr_to_string(&c.node.callee.node, consts);
        let args = c
            .node
            .args
            .iter()
            .map(|a| expr_to_string(&a.node, consts))
            .collect::<Vec<_>>();
        Some((target, args))
    } else {
        None
    }
}

fn type_to_string(t: &TypeRef) -> String {
    match t {
        TypeRef::Primitive(p) => format!("{:?}", p),
        TypeRef::Named(id) => id.text.clone(),
        TypeRef::Generic { base, args } => {
            let inner = args
                .iter()
                .map(|a| type_to_string(&a.node))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", base.text, inner)
        }
        TypeRef::Array { elem, len, .. } => format!("[{}; {}]", type_to_string(&elem.node), len),
        TypeRef::Regimed { regime, inner } => {
            let regime_str = match regime {
                Regime::K => "K",
                Regime::Q => "Q",
                Regime::Phi => "Phi",
            };
            format!("{}[{}]", regime_str, type_to_string(&inner.node))
        }
    }
}

fn expr_to_string(e: &Expr, consts: &HashMap<String, String>) -> String {
    match e {
        Expr::Literal(Literal::Int(n)) => n.to_string(),
        Expr::Literal(Literal::Float(f)) => f.to_string(),
        Expr::Literal(Literal::Bool(b)) => b.to_string(),
        Expr::Literal(Literal::String(s)) => format!("{:?}", s),
        Expr::Literal(Literal::Char(c)) => format!("'{}'", c),
        Expr::Ident(id) => consts
            .get(&id.text)
            .cloned()
            .unwrap_or_else(|| id.text.clone()),
        Expr::Paren(inner) => format!("({})", expr_to_string(&inner.node, consts)),
        Expr::Block(b) => format!("{{ ... }}"),
        Expr::Binary(b) => format!(
            "({} {:?} {})",
            expr_to_string(&b.node.lhs.node, consts),
            b.node.op.node,
            expr_to_string(&b.node.rhs.node, consts)
        ),
        Expr::Unary(u) => format!(
            "{:?}({})",
            u.node.op.node,
            expr_to_string(&u.node.operand.node, consts)
        ),
        Expr::Call(c) => {
            let args = c
                .node
                .args
                .iter()
                .map(|a| expr_to_string(&a.node, consts))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", expr_to_string(&c.node.callee.node, consts), args)
        }
        Expr::Field(f) => format!(
            "{}.{}",
            expr_to_string(&f.node.base.node, consts),
            f.node.field.text
        ),
        Expr::Index(i) => format!(
            "{}[{}]",
            expr_to_string(&i.node.base.node, consts),
            expr_to_string(&i.node.index.node, consts)
        ),
        Expr::Array(arr) => {
            let elements = arr
                .iter()
                .map(|e| expr_to_string(&e.node, consts))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", elements)
        }
        Expr::StructLit(s) => {
            let inits = s
                .node
                .inits
                .iter()
                .map(|fi| {
                    format!(
                        "{}: {}",
                        fi.node.name.text,
                        expr_to_string(&fi.node.expr.node, consts)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}{{{}}}", s.node.ty_name.text, inits)
        }
        Expr::Match(m) => {
            let arms = m
                .node
                .arms
                .iter()
                .map(|arm| {
                    format!(
                        "{} => {}",
                        match_arm_pattern_to_string(&arm.node.pattern.node),
                        expr_to_string(&arm.node.body.node, consts)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "match {} {{ {} }}",
                expr_to_string(&m.node.expr.node, consts),
                arms
            )
        }
        Expr::Alloc(a) => {
            let inner = a.as_ref();
            let _ty_str = inner
                .ty
                .as_ref()
                .map(|t| type_to_string(&t.node))
                .unwrap_or_default();
            format!("alloc({})", expr_to_string(&inner.size.node, consts))
        }
        Expr::Spawn(s) => {
            let inner = s.as_ref();
            let seed_str = inner
                .seed
                .as_ref()
                .map(|e| format!(", {}", expr_to_string(&e.node, consts)))
                .unwrap_or_default();
            format!(
                "spawn({}{})",
                expr_to_string(&inner.callee.node, consts),
                seed_str
            )
        }
        Expr::Join(j) => {
            let inner = j.as_ref();
            format!("join({})", expr_to_string(&inner.thread.node, consts))
        }
        Expr::MutexNew(_) => "mutex_new()".to_string(),
    }
}

fn match_arm_pattern_to_string(p: &MatchPattern) -> String {
    match p {
        MatchPattern::Literal(lit) => literal_to_string(lit),
        MatchPattern::Ident(id) => id.text.clone(),
        MatchPattern::Wildcard => "_".to_string(),
        MatchPattern::Or(a, b) => format!(
            "{} | {}",
            match_arm_pattern_to_string(&a.node),
            match_arm_pattern_to_string(&b.node)
        ),
    }
}

fn proc_ref_to_string(r: &ProcPathRef) -> String {
    match r {
        ProcPathRef::Unqualified(id) => id.text.clone(),
        ProcPathRef::Qualified(p) => {
            let reg = match p.regime.node {
                Regime::K => "K",
                Regime::Q => "Q",
                Regime::Phi => "Φ",
            };
            format!("{}::{}", reg, p.name.text)
        }
    }
}

fn contract_op_to_string(op: &ContractOp) -> String {
    match op {
        ContractOp::EqEq => "==",
        ContractOp::Lt => "<",
        ContractOp::Lte => "<=",
        ContractOp::Gt => ">",
        ContractOp::Gte => ">=",
    }
    .into()
}

fn literal_to_string(lit: &Literal) -> String {
    match lit {
        Literal::Int(n) => n.to_string(),
        Literal::Float(f) => f.to_string(),
        Literal::Bool(b) => b.to_string(),
        Literal::String(s) => format!("{:?}", s),
        Literal::Char(c) => format!("'{}'", c),
    }
}

fn contract_value_to_string(v: &ContractValue, consts: &HashMap<String, String>) -> String {
    match v {
        ContractValue::Ident(id) => consts
            .get(&id.text)
            .cloned()
            .unwrap_or_else(|| id.text.clone()),
        ContractValue::Literal(Literal::Int(n)) => n.to_string(),
        ContractValue::Literal(Literal::Float(f)) => f.to_string(),
        ContractValue::Literal(Literal::Bool(b)) => b.to_string(),
        ContractValue::Literal(Literal::String(s)) => format!("{:?}", s),
        ContractValue::Literal(Literal::Char(c)) => format!("'{}'", c),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_and_check;

    #[test]
    fn allows_constrain_and_prove_in_phi_proc() {
        let src = r#"
forge f {
    shape Candidate { x: K[Int]; }
    proc Φ::admit(c: Candidate) -> Candidate {
        constrain c.x > 0;
        prove w from c;
        return w;
    }
}
"#;
        parse_and_check(src).expect("Φ constrain/prove should be semantically valid");
    }

    #[test]
    fn rejects_constrain_outside_phi_proc() {
        let src = r#"
forge f {
    proc K::main {
        constrain 1 == 1;
    }
}
"#;
        let err = parse_and_check(src).expect_err("constrain in K proc must fail");
        assert!(
            err.message
                .contains("`constrain` is only permitted in Φ-regime processes"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn rejects_prove_outside_phi_proc() {
        let src = r#"
forge f {
    shape Candidate { x: K[Int]; }
    proc K::main(c: Candidate) {
        prove w from c;
    }
}
"#;
        let err = parse_and_check(src).expect_err("prove in K proc must fail");
        assert!(
            err.message
                .contains("`prove` is only permitted in Φ-regime processes"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn prove_registers_immutable_witness_binding() {
        let src = r#"
forge f {
    shape Candidate { x: K[Int]; }
    proc Φ::main(c: Candidate) {
        prove w from c;
        w = c;
    }
}
"#;
        let err = parse_and_check(src).expect_err("witness reassignment must fail");
        assert!(
            err.message
                .contains("assignment to immutable binding 'w' (use `mut let`)"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn rejects_phi_effect_without_prove_witness() {
        let src = r#"
forge f {
    proc Φ::admit() {
        emit "x";
    }
}
"#;
        let err = parse_and_check(src).expect_err("Φ effects without witness must fail");
        assert!(
            err.message.contains(
                "Φ-regime process with effects must produce at least one witness via `prove`"
            ),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn allows_phi_effect_when_witness_is_produced() {
        let src = r#"
forge f {
    shape Candidate { x: K[Int]; }
    proc Φ::admit(c: Candidate) -> Candidate {
        prove w from c;
        seal w;
        return w;
    }
}
"#;
        parse_and_check(src).expect("Φ effect with witness should be valid");
    }

    #[test]
    fn rejects_cross_regime_call_without_bind() {
        let src = r#"
forge f {
    proc K::main() -> K[Int] {
        return Q::solve();
    }

    proc Q::solve() -> K[Int] linear {
        return 1;
    }
}
"#;
        let err = parse_and_check(src).expect_err("cross-regime call without bind must fail");
        assert!(
            err.message.contains(
                "cross-regime call 'K::main' -> 'Q::solve' requires explicit bind declaration"
            ),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn allows_cross_regime_call_with_bind_either_direction() {
        let src = r#"
forge f {
    proc K::main() -> K[Int] {
        return Q::solve();
    }

    proc Q::solve() -> K[Int] linear {
        return 1;
    }

    bind Q::solve -> K::main contract {
        allowed == true;
    }
}
"#;
        parse_and_check(src).expect("cross-regime call with bind should be valid");
    }
}
