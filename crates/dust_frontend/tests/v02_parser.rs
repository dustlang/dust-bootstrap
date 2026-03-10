use dust_frontend::{Lexer, Parser};

fn parse_ok(src: &str) {
    let toks = Lexer::new(src)
        .lex_all()
        .expect("lexer should succeed for v0.2 snippet");
    let mut parser = Parser::new(toks);
    parser
        .parse_file()
        .expect("parser should succeed for v0.2 snippet");
}

fn parse_file(src: &str) -> dust_frontend::ast::FileAst {
    let toks = Lexer::new(src)
        .lex_all()
        .expect("lexer should succeed for v0.2 snippet");
    let mut parser = Parser::new(toks);
    parser
        .parse_file()
        .expect("parser should succeed for v0.2 snippet")
}

#[test]
fn parses_type_first_params_and_tail_expr_return() {
    let src = r#"
K factorial(K[Int] n) -> K[Int] {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}
"#;
    parse_ok(src);
}

#[test]
fn parses_mut_let_and_assignment_stmt() {
    let src = r#"
K main {
    mut let counter: K[Int] = 0;
    counter = counter + 1;
    emit "counter {counter}";
}
"#;
    parse_ok(src);
}

#[test]
fn parses_else_if_and_for_range() {
    let src = r#"
K main {
    let x: K[Int] = 1;
    if x > 0 {
        emit "pos";
    } else if x < 0 {
        emit "neg";
    } else {
        emit "zero";
    }

    for i in 0..3 {
        emit "i={i}";
    }
}
"#;
    parse_ok(src);
}

#[test]
fn parses_not_equal_binary_operator() {
    let src = r#"
K main {
    let a: K[Int] = 1;
    let b: K[Int] = 2;
    if a != b {
        emit "ne";
    } else {
        emit "eq";
    }
}
"#;
    parse_ok(src);
}

#[test]
fn parses_struct_literals_and_match_or_patterns() {
    let src = r#"
K main {
    let p = Point { x: 1, y: 2 };
    let v = match 2 {
        0 | 1 => 10,
        2 => 20,
        _ => 30,
    };
    return v;
}
"#;
    parse_ok(src);
}

#[test]
fn enforces_operator_precedence() {
    use dust_frontend::ast::{BinOp, Expr, Item, Stmt};

    let src = r#"
K main {
    let x = 1 + 2 * 3;
    return x;
}
"#;

    let ast = parse_file(src);
    let forge = &ast.forges[0].node;
    let proc = forge
        .items
        .iter()
        .find_map(|item| match &item.node {
            Item::Proc(p) => Some(p),
            _ => None,
        })
        .expect("expected proc item");

    let first_stmt = &proc.body.node.stmts[0].node;
    let let_stmt = match first_stmt {
        Stmt::Let(s) => s,
        other => panic!("expected let statement, found {:?}", other),
    };
    let expr = &let_stmt.expr.node;

    let Expr::Binary(top) = expr else {
        panic!("expected binary expression");
    };
    assert_eq!(top.node.op.node, BinOp::Add);
    assert!(matches!(top.node.lhs.node, Expr::Literal(_)));
    let Expr::Binary(rhs) = &top.node.rhs.node else {
        panic!("expected multiplicative rhs");
    };
    assert_eq!(rhs.node.op.node, BinOp::Mul);
}

#[test]
fn parses_unsafe_block_expression() {
    let src = r#"
K main {
    unsafe {
        let x: K[Int] = 1;
    };
    return 0;
}
"#;
    parse_ok(src);
}

#[test]
fn parses_thread_generic_type_form() {
    let src = r#"
K worker() -> K[Int] {
    return 1;
}

K main {
    let t: Thread<K[Int]> = spawn(worker);
    return 0;
}
"#;
    parse_ok(src);
}

#[test]
fn parses_v02_system_effect_expression_forms() {
    let src = r#"
K worker() -> K[Int] {
    return 1;
}

K main {
    let m: Mutex = mutex_new();
    mutex_lock(m);
    mutex_unlock(m);

    let mem: Mem = alloc(64);
    free(mem);

    let th: Thread<K[Int]> = spawn(worker, 7);
    let joined: K[Int] = join(th);

    let file: File = open("path.bin", "rw");
    let nread: K[Int] = read(file, mem, 16);
    let nwrote: K[Int] = write(file, mem, nread);
    close(file);

    let port: Port = 3;
    let io_v: K[Int] = io_read(port);
    io_write(port, io_v);

    let dev: Device = 4096;
    let mmio_v: K[Int] = mmio_read(dev);
    mmio_write(dev, mmio_v);

    return joined + nwrote;
}
"#;
    parse_ok(src);
}

#[test]
fn parses_bind_contracts_and_proc_uses_clauses() {
    let src = r#"
forge net {
    proc K::tx() uses channel(mode = "lossless", retries = 3) -> K[Int] {
        observe true;
        emit "tx";
        seal "ok";
        return 0;
    }

    proc K::rx() -> K[Int] {
        return 1;
    }

    bind K::tx -> rx contract {
        latency <= 10;
        mode == "strict";
        enabled == true;
    }
}
"#;
    parse_ok(src);
}

#[test]
fn parses_shorthand_proc_with_uses_clause() {
    let src = r#"
K main() uses io(port = 1) -> K[Int] {
    observe true;
    return 0;
}
"#;
    parse_ok(src);
}

#[test]
fn parses_shorthand_proc_without_parens_with_uses_clause() {
    let src = r#"
K main uses io(port = 1) {
    emit "ok";
}
"#;
    parse_ok(src);
}

#[test]
fn parses_constraint_and_prove_statements() {
    use dust_frontend::ast::{Item, Stmt};

    let src = r#"
forge f {
    proc Φ::admit(c: Candidate) -> Witness {
        constrain c.x > 0;
        prove w from c;
        return w;
    }
}
"#;

    let ast = parse_file(src);
    let forge = &ast.forges[0].node;
    let proc = forge
        .items
        .iter()
        .find_map(|item| match &item.node {
            Item::Proc(p) => Some(p),
            _ => None,
        })
        .expect("expected proc item");

    assert!(matches!(proc.body.node.stmts[0].node, Stmt::Constrain(_)));
    assert!(matches!(proc.body.node.stmts[1].node, Stmt::Prove(_)));
}
