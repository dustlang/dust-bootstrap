// File: parser.rs - This file is part of the DPL Toolchain
// Copyright (c) 2026 Dust LLC, and Contributors
// Description:
//   DPL v0.1 parser (structure + expressions) following spec/03-grammar.md.
//   This module implements:
//     - Recursive descent parsing
//     - AST construction from tokens
//     - Expression parsing (arithmetic, logical, comparison)
//     - Statement parsing (variable declarations, control flow)
//     - Error reporting with source span information

use crate::ast::*;
use crate::lexer::{Keyword, Token};
use std::fmt;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

pub struct Parser {
    toks: Vec<Spanned<Token>>,
    i: usize,
}

impl Parser {
    pub fn new(toks: Vec<Spanned<Token>>) -> Self {
        Self { toks, i: 0 }
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Entry point
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    pub fn parse_file(&mut self) -> Result<FileAst, ParseError> {
        let mut forges: Vec<Spanned<ForgeDecl>> = Vec::new();

        // Implicit root forge for shorthand procs
        let mut root_items: Vec<Spanned<Item>> = Vec::new();
        let mut root_start: Option<u32> = None;
        let mut root_end: u32 = 0;

        while !self.is_eof() {
            match &self.peek().node {
                Token::Keyword(Keyword::Forge) => {
                    // Flush any accumulated shorthand procs
                    if !root_items.is_empty() {
                        let start = root_start.unwrap_or(0);
                        let name = Ident::new("__root__", Span::new(start, start));
                        forges.push(Spanned::new(
                            ForgeDecl {
                                name,
                                items: std::mem::take(&mut root_items),
                            },
                            Span::new(start, root_end),
                        ));
                        root_start = None;
                        root_end = 0;
                    }

                    forges.push(self.parse_forge()?);
                }

                // Top-level shorthand proc: K/Q/Φ main { ... }
                Token::Keyword(Keyword::K)
                | Token::Keyword(Keyword::Q)
                | Token::Keyword(Keyword::Phi) => {
                    let p = self.parse_proc_shorthand()?;
                    if root_start.is_none() {
                        root_start = Some(p.span.start);
                    }
                    root_end = p.span.end;
                    root_items.push(Spanned::new(Item::Proc(p.node), p.span));
                }

                _ => return Err(self.err_here("expected `forge` or top-level `K/Q/Φ` proc")),
            }
        }

        // Flush trailing root forge
        if !root_items.is_empty() {
            let start = root_start.unwrap_or(0);
            let name = Ident::new("__root__", Span::new(start, start));
            forges.push(Spanned::new(
                ForgeDecl {
                    name,
                    items: root_items,
                },
                Span::new(start, root_end),
            ));
        }

        Ok(FileAst { forges })
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Forge parsing
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_forge(&mut self) -> Result<Spanned<ForgeDecl>, ParseError> {
        let start = self.expect_kw(Keyword::Forge)?.span.start;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;

        let mut items = Vec::new();
        while !self.peek_is(&Token::RBrace) {
            items.push(self.parse_item()?);
        }

        let end = self.expect(Token::RBrace)?.span.end;
        Ok(Spanned::new(
            ForgeDecl { name, items },
            Span::new(start, end),
        ))
    }

    fn parse_item(&mut self) -> Result<Spanned<Item>, ParseError> {
        match &self.peek().node {
            Token::Keyword(Keyword::Proc) => {
                let p = self.parse_proc()?;
                Ok(Spanned::new(Item::Proc(p.node), p.span))
            }
            Token::Keyword(Keyword::Const) => {
                let c = self.parse_const()?;
                Ok(Spanned::new(Item::Const(c.node), c.span))
            }
            Token::Keyword(Keyword::Shape) => {
                let s = self.parse_shape()?;
                Ok(Spanned::new(Item::Shape(s.node), s.span))
            }
            Token::Keyword(Keyword::Bind) => {
                let b = self.parse_bind()?;
                Ok(Spanned::new(Item::Bind(b.node), b.span))
            }
            _ => Err(self.err_here("expected forge item (`proc`, `const`, `shape`, or `bind`)")),
        }
    }

    fn parse_const(&mut self) -> Result<Spanned<ConstDecl>, ParseError> {
        let start = self.expect_kw(Keyword::Const)?.span.start;
        let name = self.expect_ident()?;

        let ty = if self.peek_is(&Token::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(Token::Eq)?;
        let value = self.parse_literal()?;
        self.expect(Token::Semi)?;
        let end = value.span.end;

        Ok(Spanned::new(
            ConstDecl { name, ty, value },
            Span::new(start, end),
        ))
    }

    fn parse_shape(&mut self) -> Result<Spanned<ShapeDecl>, ParseError> {
        let start = self.expect_kw(Keyword::Shape)?.span.start;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;

        let mut fields = Vec::new();
        while !self.peek_is(&Token::RBrace) {
            let field_name = self.expect_ident()?;
            self.expect(Token::Colon)?;
            let field_type = self.parse_type()?;
            self.expect(Token::Semi)?;
            let field_span = field_type.span;
            fields.push(Spanned::new(
                FieldDecl {
                    name: field_name,
                    ty: field_type,
                },
                field_span,
            ));
        }

        let end = self.expect(Token::RBrace)?.span.end;

        Ok(Spanned::new(
            ShapeDecl { name, fields },
            Span::new(start, end),
        ))
    }

    fn parse_bind(&mut self) -> Result<Spanned<BindDecl>, ParseError> {
        let start = self.expect_kw(Keyword::Bind)?.span.start;
        let source = self.parse_proc_ref()?;
        self.expect(Token::Arrow)?;
        let target = self.parse_proc_ref()?;
        let contract = self.parse_contract_block()?;
        let end = contract.span.end;

        Ok(Spanned::new(
            BindDecl {
                source,
                target,
                contract,
            },
            Span::new(start, end),
        ))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Shorthand proc (expanded for v0.2)
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Shorthand proc (expanded for v0.2)
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_proc_shorthand(&mut self) -> Result<Spanned<ProcDecl>, ParseError> {
        // Syntax:  K main { ... }  or  K main(params) -> ReturnType { ... }
        let regime = self.parse_regime()?;
        let name = self.expect_ident()?;
        let start = regime.span.start;

        // Parse parameters if present.
        // Supports both:
        //   - name-first:  x: K[Int]
        //   - type-first:  K[Int] x
        let params = if self.peek_is(&Token::LParen) {
            self.bump();
            let mut params = Vec::new();
            while !self.peek_is(&Token::RParen) {
                params.push(self.parse_param_decl()?);
                if !self.peek_is(&Token::RParen) {
                    self.expect(Token::Comma)?;
                }
            }
            self.expect(Token::RParen)?;
            params
        } else {
            Vec::new()
        };

        let uses = self.parse_uses_clauses()?;

        // Parse return type if present
        let ret = if self.peek_is(&Token::Arrow) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };

        let qualifiers = self.parse_proc_qualifiers()?;
        let body = self.parse_block()?;
        let end = body.span.end;

        let path = Spanned::new(ProcPath { regime, name }, Span::new(start, end));

        let sig = Spanned::new(
            ProcSig {
                path,
                params,
                uses,
                ret,
                qualifiers,
            },
            Span::new(start, end),
        );

        Ok(Spanned::new(ProcDecl { sig, body }, Span::new(start, end)))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Normal proc parsing - enhanced for v0.2
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_proc(&mut self) -> Result<Spanned<ProcDecl>, ParseError> {
        let start = self.expect_kw(Keyword::Proc)?.span.start;
        let path = self.parse_proc_path()?;

        // Parse parameters if present.
        // Supports both:
        //   - name-first:  x: K[Int]
        //   - type-first:  K[Int] x
        let params = if self.peek_is(&Token::LParen) {
            self.bump();
            let mut params = Vec::new();
            while !self.peek_is(&Token::RParen) {
                params.push(self.parse_param_decl()?);
                if !self.peek_is(&Token::RParen) {
                    self.expect(Token::Comma)?;
                }
            }
            self.expect(Token::RParen)?;
            params
        } else {
            Vec::new()
        };

        let uses = self.parse_uses_clauses()?;

        // Parse return type if present
        let ret = if self.peek_is(&Token::Arrow) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };

        let qualifiers = self.parse_proc_qualifiers()?;
        let body = self.parse_block()?;
        let end = body.span.end;

        let sig = Spanned::new(
            ProcSig {
                path,
                params,
                uses,
                ret,
                qualifiers,
            },
            Span::new(start, end),
        );

        Ok(Spanned::new(ProcDecl { sig, body }, Span::new(start, end)))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Shared helpers
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_regime(&mut self) -> Result<Spanned<Regime>, ParseError> {
        let t = self.peek().clone();
        match t.node {
            Token::Keyword(Keyword::K) => Ok(Spanned::new(Regime::K, self.bump().span)),
            Token::Keyword(Keyword::Q) => Ok(Spanned::new(Regime::Q, self.bump().span)),
            Token::Keyword(Keyword::Phi) => Ok(Spanned::new(Regime::Phi, self.bump().span)),
            _ => Err(self.err_here("expected regime K/Q/Φ")),
        }
    }

    fn parse_proc_qualifiers(&mut self) -> Result<Vec<Spanned<ProcQualifier>>, ParseError> {
        let mut qualifiers = Vec::new();
        while matches!(self.peek().node, Token::Keyword(Keyword::Linear)) {
            let t = self.bump();
            qualifiers.push(Spanned::new(ProcQualifier::Linear, t.span));
        }
        Ok(qualifiers)
    }

    fn parse_uses_clauses(&mut self) -> Result<Vec<Spanned<UsesClause>>, ParseError> {
        let mut uses = Vec::new();
        while self.peek_is(&Token::Keyword(Keyword::Uses)) {
            uses.push(self.parse_uses_clause()?);
        }
        Ok(uses)
    }

    fn parse_uses_clause(&mut self) -> Result<Spanned<UsesClause>, ParseError> {
        let start = self.expect_kw(Keyword::Uses)?.span.start;
        let resource = self.expect_ident()?;
        self.expect(Token::LParen)?;

        let mut args = Vec::new();
        while !self.peek_is(&Token::RParen) {
            args.push(self.parse_named_arg()?);
            if !self.peek_is(&Token::RParen) {
                self.expect(Token::Comma)?;
            }
        }
        let end = self.expect(Token::RParen)?.span.end;

        Ok(Spanned::new(
            UsesClause { resource, args },
            Span::new(start, end),
        ))
    }

    fn parse_named_arg(&mut self) -> Result<Spanned<NamedArg>, ParseError> {
        let key = self.expect_ident()?;
        let start = key.span.start;
        self.expect(Token::Eq)?;
        let value = self.parse_literal()?;
        let end = value.span.end;
        Ok(Spanned::new(NamedArg { key, value }, Span::new(start, end)))
    }

    fn parse_proc_path(&mut self) -> Result<Spanned<ProcPath>, ParseError> {
        let reg = self.parse_regime()?;
        self.expect(Token::ColonColon)?;
        let name = self.expect_ident()?;
        let span = Span::new(reg.span.start, name.span.end);
        Ok(Spanned::new(ProcPath { regime: reg, name }, span))
    }

    fn parse_proc_ref(&mut self) -> Result<Spanned<ProcPathRef>, ParseError> {
        if matches!(
            self.peek().node,
            Token::Keyword(Keyword::K) | Token::Keyword(Keyword::Q) | Token::Keyword(Keyword::Phi)
        ) && matches!(self.peek_n(1).map(|t| &t.node), Some(Token::ColonColon))
        {
            let path = self.parse_proc_path()?;
            return Ok(Spanned::new(ProcPathRef::Qualified(path.node), path.span));
        }

        let ident = self.expect_ident()?;
        let span = ident.span;
        Ok(Spanned::new(ProcPathRef::Unqualified(ident), span))
    }

    fn parse_contract_block(&mut self) -> Result<Spanned<ContractBlock>, ParseError> {
        let start = self.expect_kw(Keyword::Contract)?.span.start;
        self.expect(Token::LBrace)?;
        let mut clauses = Vec::new();
        while !self.peek_is(&Token::RBrace) {
            clauses.push(self.parse_contract_clause()?);
        }
        let end = self.expect(Token::RBrace)?.span.end;
        Ok(Spanned::new(
            ContractBlock { clauses },
            Span::new(start, end),
        ))
    }

    fn parse_contract_clause(&mut self) -> Result<Spanned<ContractClause>, ParseError> {
        let key = self.expect_ident()?;
        let start = key.span.start;
        let op = self.parse_contract_op()?;
        let value = self.parse_contract_value()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            ContractClause { key, op, value },
            Span::new(start, end),
        ))
    }

    fn parse_contract_op(&mut self) -> Result<Spanned<ContractOp>, ParseError> {
        let t = self.peek().clone();
        match t.node {
            Token::EqEq => Ok(Spanned::new(ContractOp::EqEq, self.bump().span)),
            Token::Lt => Ok(Spanned::new(ContractOp::Lt, self.bump().span)),
            Token::Lte => Ok(Spanned::new(ContractOp::Lte, self.bump().span)),
            Token::Gt => Ok(Spanned::new(ContractOp::Gt, self.bump().span)),
            Token::Gte => Ok(Spanned::new(ContractOp::Gte, self.bump().span)),
            _ => Err(ParseError {
                message: "expected contract operator (`==`, `<`, `<=`, `>`, `>=`)".to_string(),
                span: t.span,
            }),
        }
    }

    fn parse_contract_value(&mut self) -> Result<Spanned<ContractValue>, ParseError> {
        if matches!(self.peek().node, Token::Ident(_)) {
            let id = self.expect_ident()?;
            let span = id.span;
            return Ok(Spanned::new(ContractValue::Ident(id), span));
        }
        let lit = self.parse_literal()?;
        let span = lit.span;
        Ok(Spanned::new(ContractValue::Literal(lit.node), span))
    }

    fn parse_param_decl(&mut self) -> Result<Spanned<ParamDecl>, ParseError> {
        // Name-first form: `name: Type`
        if matches!(self.peek().node, Token::Ident(_))
            && matches!(self.peek_n(1).map(|t| &t.node), Some(Token::Colon))
        {
            let name = self.expect_ident()?;
            self.expect(Token::Colon)?;
            let ty = self.parse_type()?;
            let span = Span::new(name.span.start, ty.span.end);
            return Ok(Spanned::new(ParamDecl { name, ty }, span));
        }

        // Type-first form: `Type name`
        let ty = self.parse_type()?;
        let name = self.expect_ident()?;
        let span = Span::new(ty.span.start, name.span.end);
        Ok(Spanned::new(ParamDecl { name, ty }, span))
    }

    fn parse_block(&mut self) -> Result<Spanned<Block>, ParseError> {
        let start = self.expect(Token::LBrace)?.span.start;
        let mut stmts = Vec::new();
        while !self.peek_is(&Token::RBrace) {
            stmts.push(self.parse_stmt()?);
        }
        let end = self.expect(Token::RBrace)?.span.end;
        Ok(Spanned::new(Block { stmts }, Span::new(start, end)))
    }

    fn parse_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        match &self.peek().node {
            Token::Keyword(Keyword::Let) => self.parse_let_stmt(false),
            Token::Keyword(Keyword::Mut) => self.parse_let_stmt(true),
            Token::Keyword(Keyword::Constrain) => self.parse_constrain_stmt(),
            Token::Keyword(Keyword::Prove) => self.parse_prove_stmt(),
            Token::Keyword(Keyword::Observe)
            | Token::Keyword(Keyword::Emit)
            | Token::Keyword(Keyword::Seal) => self.parse_effect_stmt(),
            Token::Keyword(Keyword::Return) => self.parse_return_stmt(),
            Token::Keyword(Keyword::If) => self.parse_if_stmt(),
            Token::Keyword(Keyword::For) => self.parse_for_stmt(),
            Token::Keyword(Keyword::While) => self.parse_while_stmt(),
            Token::Keyword(Keyword::Break) => self.parse_break_stmt(),
            Token::Keyword(Keyword::Continue) => self.parse_continue_stmt(),
            // v0.2 statements
            Token::Keyword(Keyword::Alloc) => self.parse_alloc_stmt(),
            Token::Keyword(Keyword::Free) => self.parse_free_stmt(),
            Token::Keyword(Keyword::Spawn) => self.parse_spawn_stmt(),
            Token::Keyword(Keyword::Join) => self.parse_join_stmt(),
            Token::Keyword(Keyword::MutexNew) => self.parse_mutex_new_stmt(),
            Token::Keyword(Keyword::MutexLock) => self.parse_mutex_lock_stmt(),
            Token::Keyword(Keyword::MutexUnlock) => self.parse_mutex_unlock_stmt(),
            Token::Keyword(Keyword::Open) => self.parse_open_stmt(),
            Token::Keyword(Keyword::Read) => self.parse_read_stmt(),
            Token::Keyword(Keyword::Write) => self.parse_write_stmt(),
            Token::Keyword(Keyword::Close) => self.parse_close_stmt(),
            Token::Keyword(Keyword::Unsafe) => self.parse_unsafe_stmt(),
            Token::Ident(_) if self.peek_is_assign_stmt() => self.parse_assign_stmt(),
            _ => self.parse_expr_stmt(),
        }
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Variable statements (let, mut let)
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_let_stmt(&mut self, mutable: bool) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.bump().span.start;
        if mutable && self.peek_is(&Token::Keyword(Keyword::Let)) {
            // v0.2 syntax: `mut let x = ...`
            self.bump();
        }

        let name = self.expect_ident()?;

        let ty = if self.peek_is(&Token::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(Token::Eq)?;
        let expr = self.parse_expr()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;

        let stmt = if mutable {
            Stmt::MutLet(MutLetStmt { name, ty, expr })
        } else {
            Stmt::Let(LetStmt { name, ty, expr })
        };

        Ok(Spanned::new(stmt, Span::new(start, end)))
    }

    fn parse_assign_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let target = self.expect_ident()?;
        let start = target.span.start;
        self.expect(Token::Eq)?;
        let expr = self.parse_expr()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Assign(AssignStmt { target, expr }),
            Span::new(start, end),
        ))
    }

    fn parse_constrain_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Constrain)?.span.start;
        let predicate = self.parse_expr()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Constrain(ConstrainStmt { predicate }),
            Span::new(start, end),
        ))
    }

    fn parse_prove_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Prove)?.span.start;
        let name = self.expect_ident()?;
        self.expect_kw(Keyword::From)?;
        let from = self.parse_expr()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Prove(ProveStmt { name, from }),
            Span::new(start, end),
        ))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Effect statement (observe/emit/seal)
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_effect_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let kw = self.bump();
        let start = kw.span.start;
        let kind = match kw.node {
            Token::Keyword(Keyword::Observe) => EffectKind::Observe,
            Token::Keyword(Keyword::Emit) => EffectKind::Emit,
            Token::Keyword(Keyword::Seal) => EffectKind::Seal,
            _ => {
                return Err(ParseError {
                    message: "expected effect keyword (`observe`, `emit`, `seal`)".to_string(),
                    span: kw.span,
                });
            }
        };
        let expr = self.parse_expr()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Effect(EffectStmt {
                kind: Spanned::new(kind, kw.span),
                payload: expr,
            }),
            Span::new(start, end),
        ))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Return statement
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_return_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.bump().span.start;
        let expr = self.parse_expr()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Return(ReturnStmt { expr }),
            Span::new(start, end),
        ))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // If statement
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_if_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::If)?.span.start;

        let condition = self.parse_expr()?;
        let then_block = self.parse_block()?;

        let else_block = if self.peek_is(&Token::Keyword(Keyword::Else)) {
            self.bump();
            if self.peek_is(&Token::Keyword(Keyword::If)) {
                // else-if is lowered as an else block containing a nested if statement.
                let nested_if = self.parse_if_stmt()?;
                let block_span = nested_if.span;
                Some(Spanned::new(
                    Block {
                        stmts: vec![nested_if],
                    },
                    block_span,
                ))
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        let end = else_block
            .as_ref()
            .map(|b| b.span.end)
            .unwrap_or(then_block.span.end);

        Ok(Spanned::new(
            Stmt::If(IfStmt {
                condition,
                then_block,
                else_block,
            }),
            Span::new(start, end),
        ))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // For statement
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_for_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::For)?.span.start;

        let var = self.expect_ident()?;
        self.expect(Token::Keyword(Keyword::In))?;

        let start_expr = self.parse_expr()?;
        self.expect(Token::DotDot)?;
        let end_expr = self.parse_expr()?;

        let body = self.parse_block()?;
        let end = body.span.end;

        Ok(Spanned::new(
            Stmt::For(ForStmt {
                var,
                start: start_expr,
                end: end_expr,
                body,
            }),
            Span::new(start, end),
        ))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // While statement
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_while_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::While)?.span.start;

        let condition = self.parse_expr()?;
        let body = self.parse_block()?;
        let end = body.span.end;

        Ok(Spanned::new(
            Stmt::While(WhileStmt { condition, body }),
            Span::new(start, end),
        ))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Break/Continue statements
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_break_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Break)?.span.start;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(Stmt::Break(BreakStmt), Span::new(start, end)))
    }

    fn parse_continue_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Continue)?.span.start;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Continue(ContinueStmt),
            Span::new(start, end),
        ))
    }

    // ========================================================================
    // v0.2 Memory Statements (alloc, free)
    // ========================================================================

    fn parse_alloc_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Alloc)?.span.start;
        self.expect(Token::LParen)?;
        let size = self.parse_expr()?;
        self.expect(Token::RParen)?;
        let name = self.expect_ident()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Alloc(AllocStmt {
                name,
                size,
                ty: None,
            }),
            Span::new(start, end),
        ))
    }

    fn parse_free_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Free)?.span.start;
        self.expect(Token::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(Token::RParen)?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Free(FreeStmt { expr }),
            Span::new(start, end),
        ))
    }

    // ========================================================================
    // v0.2 Concurrency Statements (spawn, join)
    // ========================================================================

    fn parse_spawn_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Spawn)?.span.start;
        self.expect(Token::LParen)?;
        let callee = self.parse_expr()?;
        let seed = if self.peek_is(&Token::Comma) {
            self.expect(Token::Comma)?;
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(Token::RParen)?;
        let name = self.expect_ident()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Spawn(SpawnStmt { name, callee, seed }),
            Span::new(start, end),
        ))
    }

    fn parse_join_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Join)?.span.start;
        self.expect(Token::LParen)?;
        let thread = self.parse_expr()?;
        self.expect(Token::RParen)?;
        let name = self.expect_ident()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Join(JoinStmt { name, thread }),
            Span::new(start, end),
        ))
    }

    // ========================================================================
    // v0.2 Mutex Statements
    // ========================================================================

    fn parse_mutex_new_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::MutexNew)?.span.start;
        self.expect(Token::LParen)?;
        self.expect(Token::RParen)?;
        let name = self.expect_ident()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::MutexNew(MutexNewStmt { name }),
            Span::new(start, end),
        ))
    }

    fn parse_mutex_lock_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::MutexLock)?.span.start;
        self.expect(Token::LParen)?;
        let mutex = self.parse_expr()?;
        self.expect(Token::RParen)?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::MutexLock(MutexLockStmt { mutex }),
            Span::new(start, end),
        ))
    }

    fn parse_mutex_unlock_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::MutexUnlock)?.span.start;
        self.expect(Token::LParen)?;
        let mutex = self.parse_expr()?;
        self.expect(Token::RParen)?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::MutexUnlock(MutexUnlockStmt { mutex }),
            Span::new(start, end),
        ))
    }

    // ========================================================================
    // v0.2 I/O Statements (open, read, write, close)
    // ========================================================================

    fn parse_open_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Open)?.span.start;
        self.expect(Token::LParen)?;
        let path = self.parse_expr()?;
        self.expect(Token::Comma)?;
        let mode = self.parse_expr()?;
        self.expect(Token::RParen)?;
        let name = self.expect_ident()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Open(OpenStmt { name, path, mode }),
            Span::new(start, end),
        ))
    }

    fn parse_read_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Read)?.span.start;
        self.expect(Token::LParen)?;
        let file = self.parse_expr()?;
        self.expect(Token::Comma)?;
        let buffer = self.parse_expr()?;
        self.expect(Token::Comma)?;
        let n = self.parse_expr()?;
        self.expect(Token::RParen)?;
        let name = self.expect_ident()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Read(ReadStmt {
                name,
                file,
                buffer,
                n,
            }),
            Span::new(start, end),
        ))
    }

    fn parse_write_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Write)?.span.start;
        self.expect(Token::LParen)?;
        let file = self.parse_expr()?;
        self.expect(Token::Comma)?;
        let buffer = self.parse_expr()?;
        self.expect(Token::Comma)?;
        let n = self.parse_expr()?;
        self.expect(Token::RParen)?;
        let name = self.expect_ident()?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Write(WriteStmt {
                name,
                file,
                buffer,
                n,
            }),
            Span::new(start, end),
        ))
    }

    fn parse_close_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Close)?.span.start;
        self.expect(Token::LParen)?;
        let file = self.parse_expr()?;
        self.expect(Token::RParen)?;
        self.expect(Token::Semi)?;
        let end = self.prev_span().end;
        Ok(Spanned::new(
            Stmt::Close(CloseStmt { file }),
            Span::new(start, end),
        ))
    }

    // ========================================================================
    // v0.2 Unsafe Block
    // ========================================================================

    fn parse_unsafe_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.expect_kw(Keyword::Unsafe)?.span.start;
        let body = self.parse_block()?;
        let end = body.span.end;
        Ok(Spanned::new(
            Stmt::Unsafe(UnsafeStmt { body: body.node }),
            Span::new(start, end),
        ))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Match expression (v0.2)
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_match_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let start = self.expect_kw(Keyword::Match)?.span.start;

        let expr = self.parse_expr()?;
        self.expect(Token::LBrace)?;

        let mut arms = Vec::new();
        while !self.peek_is(&Token::RBrace) {
            let pattern = self.parse_match_pattern()?;
            self.expect(Token::FatArrow)?;
            let body = self.parse_expr()?;

            arms.push(Spanned::new(MatchArm { pattern, body }, Span::default()));

            if !self.peek_is(&Token::RBrace) {
                self.expect(Token::Comma)?;
            }
        }

        self.expect(Token::RBrace)?;
        let end = self.prev_span().end;

        let match_expr = Spanned::new(MatchExpr { expr, arms }, Span::new(start, end));

        Ok(Spanned::new(
            Expr::Match(Box::new(match_expr)),
            Span::new(start, end),
        ))
    }

    fn parse_match_pattern(&mut self) -> Result<Spanned<MatchPattern>, ParseError> {
        let mut pattern = self.parse_match_pattern_atom()?;
        while self.peek_is(&Token::Pipe) {
            self.bump();
            let rhs = self.parse_match_pattern_atom()?;
            let span = Span::new(pattern.span.start, rhs.span.end);
            pattern = Spanned::new(MatchPattern::Or(Box::new(pattern), Box::new(rhs)), span);
        }
        Ok(pattern)
    }

    fn parse_match_pattern_atom(&mut self) -> Result<Spanned<MatchPattern>, ParseError> {
        let t = self.peek().clone();
        match &t.node {
            Token::Int(n) => {
                self.bump();
                let sp = self.prev_span();
                Ok(Spanned::new(MatchPattern::Literal(Literal::Int(*n)), sp))
            }
            Token::Bool(b) => {
                self.bump();
                let sp = self.prev_span();
                Ok(Spanned::new(MatchPattern::Literal(Literal::Bool(*b)), sp))
            }
            Token::Ident(s) => {
                self.bump();
                let sp = self.prev_span();
                let ident = Ident::new(s.clone(), sp);
                Ok(Spanned::new(MatchPattern::Ident(ident), sp))
            }
            Token::Underscore => {
                self.bump();
                let sp = self.prev_span();
                Ok(Spanned::new(MatchPattern::Wildcard, sp))
            }
            _ => Err(self.err_here("expected pattern")),
        }
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Expression statement
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_expr_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let expr = self.parse_expr()?;
        let start = expr.span.start;
        if self.peek_is(&Token::Semi) {
            self.bump();
            let end = self.prev_span().end;
            return Ok(Spanned::new(
                Stmt::Expr(ExprStmt { expr }),
                Span::new(start, end),
            ));
        }

        // v0.2 tail-expression form in a block:
        // last bare expression is treated as an implicit return value.
        if self.peek_is(&Token::RBrace) {
            let end = expr.span.end;
            return Ok(Spanned::new(
                Stmt::Return(ReturnStmt { expr }),
                Span::new(start, end),
            ));
        }

        Err(self.err_here("expected `;` after expression statement"))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Expression parsing
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        self.parse_logical_or_expr()
    }

    fn parse_logical_or_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_logical_and_expr()?;
        while self.peek_is(&Token::OrOr) {
            let op_span = self.bump().span;
            let rhs = self.parse_logical_and_expr()?;
            lhs = self.make_binary(lhs, BinOp::Or, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_logical_and_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_bit_or_expr()?;
        while self.peek_is(&Token::AndAnd) {
            let op_span = self.bump().span;
            let rhs = self.parse_bit_or_expr()?;
            lhs = self.make_binary(lhs, BinOp::And, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_bit_or_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_bit_xor_expr()?;
        while self.peek_is(&Token::Pipe) {
            let op_span = self.bump().span;
            let rhs = self.parse_bit_xor_expr()?;
            lhs = self.make_binary(lhs, BinOp::BitOr, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_bit_xor_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_bit_and_expr()?;
        while self.peek_is(&Token::Caret) {
            let op_span = self.bump().span;
            let rhs = self.parse_bit_and_expr()?;
            lhs = self.make_binary(lhs, BinOp::BitXor, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_bit_and_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_equality_expr()?;
        while self.peek_is(&Token::Amp) {
            let op_span = self.bump().span;
            let rhs = self.parse_equality_expr()?;
            lhs = self.make_binary(lhs, BinOp::BitAnd, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_equality_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_comparison_expr()?;
        loop {
            let op = if self.peek_is(&Token::EqEq) {
                Some(BinOp::Eq)
            } else if self.peek_is(&Token::BangEq) {
                Some(BinOp::Ne)
            } else {
                None
            };
            let Some(op) = op else { break };
            let op_span = self.bump().span;
            let rhs = self.parse_comparison_expr()?;
            lhs = self.make_binary(lhs, op, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_comparison_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_shift_expr()?;
        loop {
            let op = if self.peek_is(&Token::Lt) {
                Some(BinOp::Lt)
            } else if self.peek_is(&Token::Lte) {
                Some(BinOp::Le)
            } else if self.peek_is(&Token::Gt) {
                Some(BinOp::Gt)
            } else if self.peek_is(&Token::Gte) {
                Some(BinOp::Ge)
            } else {
                None
            };
            let Some(op) = op else { break };
            let op_span = self.bump().span;
            let rhs = self.parse_shift_expr()?;
            lhs = self.make_binary(lhs, op, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_shift_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_additive_expr()?;
        loop {
            let op = if self.peek_is(&Token::LtLt) {
                Some(BinOp::Shl)
            } else if self.peek_is(&Token::GtGt) {
                Some(BinOp::Shr)
            } else {
                None
            };
            let Some(op) = op else { break };
            let op_span = self.bump().span;
            let rhs = self.parse_additive_expr()?;
            lhs = self.make_binary(lhs, op, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_additive_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_multiplicative_expr()?;
        loop {
            let op = if self.peek_is(&Token::Plus) {
                Some(BinOp::Add)
            } else if self.peek_is(&Token::Minus) {
                Some(BinOp::Sub)
            } else {
                None
            };
            let Some(op) = op else { break };
            let op_span = self.bump().span;
            let rhs = self.parse_multiplicative_expr()?;
            lhs = self.make_binary(lhs, op, op_span, rhs);
        }
        Ok(lhs)
    }

    fn parse_multiplicative_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut lhs = self.parse_unary_expr()?;
        loop {
            let op = if self.peek_is(&Token::Star) {
                Some(BinOp::Mul)
            } else if self.peek_is(&Token::Slash) {
                Some(BinOp::Div)
            } else {
                None
            };
            let Some(op) = op else { break };
            let op_span = self.bump().span;
            let rhs = self.parse_unary_expr()?;
            lhs = self.make_binary(lhs, op, op_span, rhs);
        }
        Ok(lhs)
    }

    fn make_binary(
        &self,
        lhs: Spanned<Expr>,
        op: BinOp,
        op_span: Span,
        rhs: Spanned<Expr>,
    ) -> Spanned<Expr> {
        let span = Span::new(lhs.span.start, rhs.span.end);
        let bin_expr = Spanned::new(
            BinaryExpr {
                op: Spanned::new(op, op_span),
                lhs,
                rhs,
            },
            span,
        );
        Spanned::new(Expr::Binary(Box::new(bin_expr)), span)
    }

    fn parse_unary_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        if self.peek_is(&Token::Bang) {
            let start = self.bump().span.start;
            let operand = self.parse_unary_expr()?;
            let end = operand.span.end;
            let unary_expr = Spanned::new(
                UnaryExpr {
                    op: Spanned::new(UnOp::Not, Span::new(start, start)),
                    operand,
                },
                Span::new(start, end),
            );
            return Ok(Spanned::new(
                Expr::Unary(Box::new(unary_expr)),
                Span::new(start, end),
            ));
        }
        if self.peek_is(&Token::Minus) {
            let start = self.bump().span.start;
            let operand = self.parse_unary_expr()?;
            let end = operand.span.end;
            let unary_expr = Spanned::new(
                UnaryExpr {
                    op: Spanned::new(UnOp::Neg, Span::new(start, start)),
                    operand,
                },
                Span::new(start, end),
            );
            return Ok(Spanned::new(
                Expr::Unary(Box::new(unary_expr)),
                Span::new(start, end),
            ));
        }
        self.parse_postfix_expr()
    }

    fn parse_postfix_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            if self.peek_is(&Token::LParen) {
                expr = self.parse_call_expr(expr)?;
            } else if self.peek_is(&Token::LBrace) && self.can_start_struct_lit_body() {
                expr = self.parse_struct_lit_expr(expr)?;
            } else if self.peek_is(&Token::Dot) {
                expr = self.parse_field_expr(expr)?;
            } else if self.peek_is(&Token::LBracket) {
                expr = self.parse_index_expr(expr)?;
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn can_start_struct_lit_body(&self) -> bool {
        // Disambiguate `Type { ... }` from block starts such as `if cond { ... }`.
        // Struct literals must begin with either `}` (empty) or `ident :`.
        if !matches!(self.peek().node, Token::LBrace) {
            return false;
        }
        match self.peek_n(1).map(|t| &t.node) {
            Some(Token::RBrace) => true,
            Some(Token::Ident(_)) => matches!(self.peek_n(2).map(|t| &t.node), Some(Token::Colon)),
            _ => false,
        }
    }

    fn parse_struct_lit_expr(
        &mut self,
        ty_expr: Spanned<Expr>,
    ) -> Result<Spanned<Expr>, ParseError> {
        let ty_name = if let Expr::Ident(id) = &ty_expr.node {
            id.clone()
        } else {
            return Err(ParseError {
                message: "struct literal type must be an identifier".to_string(),
                span: ty_expr.span,
            });
        };

        self.expect(Token::LBrace)?;
        let mut inits = Vec::new();
        while !self.peek_is(&Token::RBrace) {
            let name = self.expect_ident()?;
            self.expect(Token::Colon)?;
            let expr = self.parse_expr()?;
            let span = Span::new(name.span.start, expr.span.end);
            inits.push(Spanned::new(FieldInit { name, expr }, span));
            if !self.peek_is(&Token::RBrace) {
                self.expect(Token::Comma)?;
            }
        }
        let end = self.expect(Token::RBrace)?.span.end;

        let span = Span::new(ty_expr.span.start, end);
        Ok(Spanned::new(
            Expr::StructLit(Box::new(Spanned::new(
                StructLitExpr { ty_name, inits },
                span,
            ))),
            span,
        ))
    }

    fn parse_call_expr(&mut self, callee: Spanned<Expr>) -> Result<Spanned<Expr>, ParseError> {
        let start = callee.span.start;
        self.bump(); // consume (

        let mut args = Vec::new();
        while !self.peek_is(&Token::RParen) {
            args.push(self.parse_expr()?);
            if !self.peek_is(&Token::RParen) {
                self.expect(Token::Comma)?;
            }
        }

        let end = self.expect(Token::RParen)?.span.end;

        let call_expr = Spanned::new(CallExpr { callee, args }, Span::new(start, end));

        Ok(Spanned::new(
            Expr::Call(Box::new(call_expr)),
            Span::new(start, end),
        ))
    }

    fn parse_field_expr(&mut self, base: Spanned<Expr>) -> Result<Spanned<Expr>, ParseError> {
        let start = base.span.start;
        self.bump(); // consume .
        let field = self.expect_ident()?;
        let end = field.span.end;

        let field_expr = Spanned::new(FieldExpr { base, field }, Span::new(start, end));

        Ok(Spanned::new(
            Expr::Field(Box::new(field_expr)),
            Span::new(start, end),
        ))
    }

    fn parse_index_expr(&mut self, base: Spanned<Expr>) -> Result<Spanned<Expr>, ParseError> {
        let start = base.span.start;
        self.bump(); // consume [
        let index = self.parse_expr()?;
        let end = self.expect(Token::RBracket)?.span.end;

        let index_expr = Spanned::new(IndexExpr { base, index }, Span::new(start, end));

        Ok(Spanned::new(
            Expr::Index(Box::new(index_expr)),
            Span::new(start, end),
        ))
    }

    fn parse_literal(&mut self) -> Result<Spanned<Literal>, ParseError> {
        let t = self.peek().clone();
        match &t.node {
            Token::Int(n) => {
                self.bump();
                Ok(Spanned::new(Literal::Int(*n), self.prev_span()))
            }
            Token::Float(f) => {
                self.bump();
                Ok(Spanned::new(Literal::Float(*f), self.prev_span()))
            }
            Token::Bool(b) => {
                self.bump();
                Ok(Spanned::new(Literal::Bool(*b), self.prev_span()))
            }
            Token::String(s) => {
                self.bump();
                Ok(Spanned::new(Literal::String(s.clone()), self.prev_span()))
            }
            Token::Char(c) => {
                self.bump();
                Ok(Spanned::new(Literal::Char(*c), self.prev_span()))
            }
            _ => Err(self.err_here("expected literal value")),
        }
    }

    fn parse_primary_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let t = self.peek().clone();
        match &t.node {
            Token::Int(n) => {
                self.bump();
                let sp = self.prev_span();
                Ok(Spanned::new(Expr::Literal(Literal::Int(*n)), sp))
            }
            Token::Float(f) => {
                self.bump();
                let sp = self.prev_span();
                Ok(Spanned::new(Expr::Literal(Literal::Float(*f)), sp))
            }
            Token::Char(c) => {
                self.bump();
                let sp = self.prev_span();
                Ok(Spanned::new(Expr::Literal(Literal::Char(*c)), sp))
            }
            Token::String(s) => {
                self.bump();
                let sp = self.prev_span();
                Ok(Spanned::new(Expr::Literal(Literal::String(s.clone())), sp))
            }
            Token::Bool(b) => {
                self.bump();
                let sp = self.prev_span();
                Ok(Spanned::new(Expr::Literal(Literal::Bool(*b)), sp))
            }
            Token::Keyword(Keyword::Unsafe) => self.parse_unsafe_block_expr(),
            Token::Ident(_) => self.parse_ident_path_expr(),
            Token::Keyword(k) if keyword_expr_ident_text(k).is_some() => {
                self.parse_ident_path_expr()
            }
            Token::LParen => {
                self.bump();
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                let span = Span::new(t.span.start, self.prev_span().end);
                Ok(Spanned::new(Expr::Paren(Box::new(expr)), span))
            }
            Token::LBracket => self.parse_array_or_slice(),
            Token::LBrace => self.parse_struct_or_block(),
            Token::Keyword(Keyword::Match) => self.parse_match_expr(),
            _ => Err(self.err_here("expected expression")),
        }
    }

    fn parse_ident_path_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let first = self.bump();
        let start = first.span.start;
        let mut end = first.span.end;
        let mut name = token_path_segment(&first.node).ok_or_else(|| ParseError {
            message: "expected identifier path segment".to_string(),
            span: first.span,
        })?;

        while self.peek_is(&Token::ColonColon) {
            self.bump();
            name.push_str("::");

            let seg = self.bump();
            if let Some(segment) = token_path_segment(&seg.node) {
                name.push_str(&segment);
                end = seg.span.end;
                continue;
            }
            match seg.node {
                Token::Ident(s) => {
                    name.push_str(&s);
                    end = seg.span.end;
                }
                Token::Keyword(Keyword::K) => {
                    name.push('K');
                    end = seg.span.end;
                }
                Token::Keyword(Keyword::Q) => {
                    name.push('Q');
                    end = seg.span.end;
                }
                Token::Keyword(Keyword::Phi) => {
                    name.push('Φ');
                    end = seg.span.end;
                }
                _ => {
                    return Err(ParseError {
                        message: "expected path segment after `::`".to_string(),
                        span: seg.span,
                    });
                }
            }
        }

        let span = Span::new(start, end);
        Ok(Spanned::new(Expr::Ident(Ident::new(name, span)), span))
    }

    fn parse_array_or_slice(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let start = self.bump().span.start;

        let mut elements = Vec::new();
        while !self.peek_is(&Token::RBracket) {
            elements.push(self.parse_expr()?);
            if !self.peek_is(&Token::RBracket) {
                self.expect(Token::Comma)?;
            }
        }

        let end = self.expect(Token::RBracket)?.span.end;

        Ok(Spanned::new(Expr::Array(elements), Span::new(start, end)))
    }

    fn parse_struct_or_block(&mut self) -> Result<Spanned<Expr>, ParseError> {
        // Could be a struct literal or a block - look ahead
        // For now, treat as block
        self.parse_block_expr()
    }

    fn parse_block_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let block = self.parse_block()?;
        Ok(Spanned::new(Expr::Block(block.node), block.span))
    }

    fn parse_unsafe_block_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let start = self.expect_kw(Keyword::Unsafe)?.span.start;
        let block = self.parse_block()?;
        let span = Span::new(start, block.span.end);
        Ok(Spanned::new(Expr::Block(block.node), span))
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Type parsing
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn parse_type(&mut self) -> Result<Spanned<TypeRef>, ParseError> {
        let t = self.peek().clone();
        match &t.node {
            Token::Keyword(Keyword::K) => {
                self.bump();
                if self.peek_is(&Token::LBracket) {
                    self.bump();
                    let inner = self.parse_type()?;
                    self.expect(Token::RBracket)?;
                    let span = Span::new(t.span.start, self.prev_span().end);
                    return Ok(Spanned::new(
                        TypeRef::Regimed {
                            regime: Regime::K,
                            inner: Box::new(inner),
                        },
                        span,
                    ));
                }
                // Just K as a type name
                Ok(Spanned::new(
                    TypeRef::Named(Ident::new("K", t.span)),
                    t.span,
                ))
            }
            Token::Keyword(Keyword::Q) => {
                self.bump();
                if self.peek_is(&Token::LBracket) {
                    self.bump();
                    let inner = self.parse_type()?;
                    self.expect(Token::RBracket)?;
                    let span = Span::new(t.span.start, self.prev_span().end);
                    return Ok(Spanned::new(
                        TypeRef::Regimed {
                            regime: Regime::Q,
                            inner: Box::new(inner),
                        },
                        span,
                    ));
                }
                Ok(Spanned::new(
                    TypeRef::Named(Ident::new("Q", t.span)),
                    t.span,
                ))
            }
            Token::Keyword(Keyword::Phi) => {
                self.bump();
                if self.peek_is(&Token::LBracket) {
                    self.bump();
                    let inner = self.parse_type()?;
                    self.expect(Token::RBracket)?;
                    let span = Span::new(t.span.start, self.prev_span().end);
                    return Ok(Spanned::new(
                        TypeRef::Regimed {
                            regime: Regime::Phi,
                            inner: Box::new(inner),
                        },
                        span,
                    ));
                }
                Ok(Spanned::new(
                    TypeRef::Named(Ident::new("Phi", t.span)),
                    t.span,
                ))
            }
            Token::Ident(s) => {
                self.bump();
                let ident_span = self.prev_span();
                let base = Ident::new(s.clone(), ident_span);

                if self.peek_is(&Token::Lt) {
                    let args = self.parse_generic_type_args()?;
                    let span = Span::new(ident_span.start, self.prev_span().end);
                    return Ok(Spanned::new(TypeRef::Generic { base, args }, span));
                }

                Ok(Spanned::new(TypeRef::Named(base), ident_span))
            }
            Token::LBracket => {
                let start = self.bump().span.start;
                let elem = self.parse_type()?;
                self.expect(Token::RBracket)?;
                let end = self.prev_span().end;
                Ok(Spanned::new(
                    TypeRef::Array {
                        elem: Box::new(elem),
                        len: 0,
                    },
                    Span::new(start, end),
                ))
            }
            _ => Err(self.err_here("expected type")),
        }
    }

    fn parse_generic_type_args(&mut self) -> Result<Vec<Spanned<TypeRef>>, ParseError> {
        self.expect(Token::Lt)?;
        let mut args = Vec::new();
        while !self.peek_is(&Token::Gt) {
            args.push(self.parse_type()?);
            if !self.peek_is(&Token::Gt) {
                self.expect(Token::Comma)?;
            }
        }
        self.expect(Token::Gt)?;
        Ok(args)
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Low-level helpers
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn peek(&self) -> &Spanned<Token> {
        &self.toks[self.i]
    }

    fn peek_n(&self, n: usize) -> Option<&Spanned<Token>> {
        self.toks.get(self.i + n)
    }

    fn peek_is_assign_stmt(&self) -> bool {
        matches!(self.peek().node, Token::Ident(_))
            && matches!(self.peek_n(1).map(|t| &t.node), Some(Token::Eq))
    }

    fn bump(&mut self) -> Spanned<Token> {
        let t = self.peek().clone();
        self.i += 1;
        t
    }

    fn prev_span(&self) -> Span {
        self.toks[self.i - 1].span
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek().node, Token::Eof)
    }

    fn peek_is(&self, want: &Token) -> bool {
        &self.peek().node == want
    }

    fn expect(&mut self, want: Token) -> Result<Spanned<Token>, ParseError> {
        let t = self.peek().clone();
        if t.node == want {
            Ok(self.bump())
        } else {
            Err(ParseError {
                message: format!("expected {:?}, found {:?}", want, t.node),
                span: t.span,
            })
        }
    }

    fn expect_kw(&mut self, want: Keyword) -> Result<Spanned<Token>, ParseError> {
        let t = self.peek().clone();
        match t.node {
            Token::Keyword(k) if k == want => Ok(self.bump()),
            _ => Err(ParseError {
                message: format!("expected keyword {:?}, found {:?}", want, t.node),
                span: t.span,
            }),
        }
    }

    fn expect_ident(&mut self) -> Result<Ident, ParseError> {
        let t = self.peek().clone();
        match t.node {
            Token::Ident(s) => {
                let sp = self.bump().span;
                Ok(Ident::new(s, sp))
            }
            _ => Err(ParseError {
                message: "expected identifier".into(),
                span: t.span,
            }),
        }
    }

    fn err_here(&self, msg: &str) -> ParseError {
        ParseError {
            message: msg.to_string(),
            span: self.peek().span,
        }
    }
}

fn keyword_expr_ident_text(keyword: &Keyword) -> Option<&'static str> {
    match keyword {
        Keyword::K => Some("K"),
        Keyword::Q => Some("Q"),
        Keyword::Phi => Some("Phi"),
        Keyword::Alloc => Some("alloc"),
        Keyword::Free => Some("free"),
        Keyword::Spawn => Some("spawn"),
        Keyword::Join => Some("join"),
        Keyword::MutexNew => Some("mutex_new"),
        Keyword::MutexLock => Some("mutex_lock"),
        Keyword::MutexUnlock => Some("mutex_unlock"),
        Keyword::Open => Some("open"),
        Keyword::Read => Some("read"),
        Keyword::Write => Some("write"),
        Keyword::Close => Some("close"),
        Keyword::IoRead => Some("io_read"),
        Keyword::IoWrite => Some("io_write"),
        Keyword::MmioRead => Some("mmio_read"),
        Keyword::MmioWrite => Some("mmio_write"),
        Keyword::Unsafe => Some("unsafe"),
        _ => None,
    }
}

fn token_path_segment(token: &Token) -> Option<String> {
    match token {
        Token::Ident(s) => Some(s.clone()),
        Token::Keyword(k) => keyword_expr_ident_text(k).map(|s| s.to_string()),
        _ => None,
    }
}
