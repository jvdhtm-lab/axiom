//! Recursive-descent parser for the small Axiom 1.0 grammar.
//!
//! The parser never panics. It performs error recovery at declaration and
//! statement boundaries and records diagnostics with stable codes. Newline
//! tokens are lexed but discarded here; `;` (or a closing `}`) terminates.
//!
//! Constructs from earlier, larger language drafts (`interface`, `while`,
//! `throw`, `query`, `operation`, `team`, …) are recognized only so they can be
//! rejected with a clear diagnostic — they never produce an AST node.

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexer::{tokenize, Token, TokenKind};
use crate::span::Span;

/// Stable diagnostic codes emitted by the parser.
pub mod codes {
    /// A generic syntax error: an unexpected token or end of input.
    pub const SYNTAX: &str = "AX0001";
    /// The file does not begin with a `module` declaration.
    pub const MISSING_MODULE: &str = "AX0002";
    /// A declaration keyword that Axiom 1.0 does not support.
    pub const FORBIDDEN_DECL: &str = "AX0010";
    /// A statement keyword that Axiom 1.0 does not support.
    pub const FORBIDDEN_STMT: &str = "AX0011";
    /// General reassignment is not supported in 1.0.
    pub const NO_ASSIGN: &str = "AX0012";
    /// Lambdas are not supported in 1.0 (use named function references).
    pub const NO_LAMBDA: &str = "AX0013";
}

/// Declaration keywords that belonged to earlier drafts and are now rejected.
const FORBIDDEN_DECL_KEYWORDS: &[&str] = &[
    "interface",
    "const",
    "team",
    "goal",
    "task",
    "contract",
    "policy",
    "workflow",
    "operation",
    "query",
    "state",
    "store",
    "transition",
    "migration",
    "tool",
];

/// Statement keywords that belonged to earlier drafts and are now rejected.
const FORBIDDEN_STMT_KEYWORDS: &[&str] = &[
    "while",
    "throw",
    "try",
    "yield",
    "require",
    "parallel",
    "retry",
    "timeout",
    "on_error",
    "checkpoint",
    "emit",
    "delegate",
    "iterate",
    "break",
    "continue",
];

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub file: File,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse Axiom source into an AST plus diagnostics.
pub fn parse(source: &str) -> ParseResult {
    let (tokens, mut diagnostics) = tokenize(source);
    let mut parser = Parser {
        tokens,
        pos: 0,
        diagnostics: Vec::new(),
    };
    let file = parser.parse_file();
    diagnostics.append(&mut parser.diagnostics);
    diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    ParseResult { file, diagnostics }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    // -- token helpers ------------------------------------------------------

    fn skip_newlines(&mut self) {
        while matches!(self.tokens[self.pos].kind, TokenKind::Newline) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> &TokenKind {
        self.skip_newlines();
        &self.tokens[self.pos].kind
    }

    fn peek_nth(&mut self, n: usize) -> &TokenKind {
        self.skip_newlines();
        let mut idx = self.pos;
        let mut seen = 0;
        while seen < n {
            idx += 1;
            if idx >= self.tokens.len() {
                return &self.tokens[self.tokens.len() - 1].kind;
            }
            if matches!(self.tokens[idx].kind, TokenKind::Newline) {
                continue;
            }
            seen += 1;
        }
        &self.tokens[idx].kind
    }

    fn span_here(&mut self) -> Span {
        self.skip_newlines();
        self.tokens[self.pos].span
    }

    fn bump(&mut self) -> Token {
        self.skip_newlines();
        let token = self.tokens[self.pos].clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        token
    }

    fn is_eof(&mut self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn at_ident(&mut self, text: &str) -> bool {
        matches!(self.peek(), TokenKind::Ident(name) if name == text)
    }

    fn eat_ident(&mut self, text: &str) -> bool {
        if self.at_ident(text) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    fn at(&mut self, kind: &TokenKind) -> bool {
        self.peek() == kind
    }

    fn current_ident(&mut self) -> Option<String> {
        match self.peek() {
            TokenKind::Ident(name) => Some(name.clone()),
            _ => None,
        }
    }

    fn error(&mut self, span: Span, code: &'static str, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error_code(span, code, message));
    }

    /// Require a token of the given kind, or report `message` and return false.
    fn expect(&mut self, kind: &TokenKind, message: &str) -> bool {
        if self.peek() == kind {
            self.bump();
            true
        } else {
            let span = self.span_here();
            self.error(span, codes::SYNTAX, message);
            false
        }
    }

    fn expect_ident(&mut self, message: &str) -> Option<(String, Span)> {
        let span = self.span_here();
        match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.bump();
                Some((name, span))
            }
            _ => {
                self.error(span, codes::SYNTAX, message);
                None
            }
        }
    }

    fn expect_semicolon(&mut self) {
        if !self.eat(&TokenKind::Semicolon) {
            let span = self.span_here();
            self.error(span, codes::SYNTAX, "expected `;`");
        }
    }

    /// Consume an optional `;` or `,` separator.
    fn eat_separator(&mut self) {
        let _ = self.eat(&TokenKind::Semicolon) || self.eat(&TokenKind::Comma);
    }

    // -- file ---------------------------------------------------------------

    fn parse_file(&mut self) -> File {
        self.skip_newlines();
        let mut diagnostics_before_module = Vec::new();

        let module = if self.at_ident("module") {
            Some(self.parse_module())
        } else {
            diagnostics_before_module.push(Diagnostic::error_code(
                self.span_here(),
                codes::MISSING_MODULE,
                "expected a `module` declaration at the start of the file",
            ));
            None
        };

        let mut imports = Vec::new();
        self.skip_newlines();
        while self.at_ident("import") {
            imports.push(self.parse_import());
            self.skip_newlines();
        }

        let mut declarations = Vec::new();
        while !self.is_eof() {
            let before = self.pos;
            match self.parse_declaration() {
                Some(decl) => declarations.push(decl),
                None => self.recover_declaration(),
            }
            if self.pos == before {
                // Guarantee forward progress.
                self.bump();
            }
            self.skip_newlines();
        }

        self.diagnostics.append(&mut diagnostics_before_module);
        File {
            module,
            imports,
            declarations,
        }
    }

    fn parse_module(&mut self) -> ModuleDecl {
        let start = self.span_here();
        self.bump(); // `module`
        let path = self.parse_qualified_name_or_empty("expected a module path after `module`");
        let end = self.span_here();
        self.expect_semicolon();
        ModuleDecl {
            path,
            span: start.merge(end),
        }
    }

    fn parse_import(&mut self) -> ImportDecl {
        let start = self.span_here();
        self.bump(); // `import`
        let mut names = Vec::new();
        self.expect(&TokenKind::LBrace, "expected `{` after `import`");
        while !self.at(&TokenKind::RBrace) && !self.is_eof() {
            if let Some((name, _)) = self.expect_ident("expected an imported name") {
                names.push(name);
            } else {
                self.bump();
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace, "expected `}` to close the import list");
        if !self.eat_ident("from") {
            let span = self.span_here();
            self.error(span, codes::SYNTAX, "expected `from` in import");
        }
        let from = match self.peek().clone() {
            TokenKind::String(text) => {
                self.bump();
                text
            }
            _ => {
                let span = self.span_here();
                self.error(span, codes::SYNTAX, "expected a string path after `from`");
                String::new()
            }
        };
        let end = self.span_here();
        self.expect_semicolon();
        ImportDecl {
            names,
            from,
            span: start.merge(end),
        }
    }

    fn parse_qualified_name_or_empty(&mut self, message: &str) -> Vec<String> {
        let mut path = Vec::new();
        if let Some((first, _)) = self.expect_ident(message) {
            path.push(first);
            while self.at(&TokenKind::Dot) {
                self.bump();
                if let Some((seg, _)) = self.expect_ident("expected a name after `.`") {
                    path.push(seg);
                } else {
                    break;
                }
            }
        }
        path
    }

    // -- declarations -------------------------------------------------------

    fn parse_declaration(&mut self) -> Option<Declaration> {
        self.skip_newlines();
        let start = self.span_here();
        let exported = self.eat_ident("export");
        self.skip_newlines();

        let kind = if self.at_ident("type") {
            DeclKind::Type(self.parse_type_decl()?)
        } else if self.at_ident("enum") {
            DeclKind::Enum(self.parse_enum_decl()?)
        } else if self.at_ident("agent") {
            DeclKind::Agent(self.parse_agent_decl()?)
        } else if self.at_ident("workspace") {
            DeclKind::Workspace(self.parse_workspace_decl()?)
        } else if self.at_ident("api") || self.at_ident("async") || self.at_ident("fn") {
            DeclKind::Function(self.parse_function_decl()?)
        } else if let TokenKind::Ident(name) = self.peek().clone() {
            let span = self.span_here();
            if FORBIDDEN_DECL_KEYWORDS.contains(&name.as_str()) {
                self.error(
                    span,
                    codes::FORBIDDEN_DECL,
                    format!(
                        "`{name}` declarations are not part of Axiom 1.0; express this with a typed function, a type, or `axiom.yaml` configuration"
                    ),
                );
            } else {
                self.error(
                    span,
                    codes::SYNTAX,
                    format!("expected a declaration, found `{name}`"),
                );
            }
            return None;
        } else {
            let span = self.span_here();
            self.error(span, codes::SYNTAX, "expected a declaration");
            return None;
        };

        let end = self.span_here();
        Some(Declaration {
            exported,
            kind,
            span: start.merge(end),
        })
    }

    fn parse_type_params(&mut self) -> Vec<String> {
        let mut params = Vec::new();
        if self.eat(&TokenKind::Lt) {
            while !self.at(&TokenKind::Gt) && !self.is_eof() {
                if let Some((name, _)) = self.expect_ident("expected a type parameter name") {
                    params.push(name);
                } else {
                    self.bump();
                }
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::Gt, "expected `>` to close type parameters");
        }
        params
    }

    fn parse_type_decl(&mut self) -> Option<TypeDecl> {
        let start = self.span_here();
        self.bump(); // `type`
        let (name, _) = self.expect_ident("expected a type name")?;
        let generics = self.parse_type_params();
        if !self.expect(&TokenKind::Equals, "expected `=` in type declaration") {
            return None;
        }
        let ty = self.parse_type();
        let end = self.span_here();
        self.expect_semicolon();
        Some(TypeDecl {
            name,
            generics,
            ty,
            span: start.merge(end),
        })
    }

    fn parse_enum_decl(&mut self) -> Option<EnumDecl> {
        let start = self.span_here();
        self.bump(); // `enum`
        let (name, _) = self.expect_ident("expected an enum name")?;
        let mut variants = Vec::new();
        self.expect(&TokenKind::LBrace, "expected `{` after the enum name");
        while !self.at(&TokenKind::RBrace) && !self.is_eof() {
            // Stop at a declaration boundary so a missing `}` recovers cleanly.
            if let TokenKind::Ident(name) = self.peek().clone() {
                if is_decl_start(&name) {
                    break;
                }
            }
            let vspan = self.span_here();
            match self.expect_ident("expected an enum variant name") {
                Some((vname, _)) => variants.push(EnumVariant {
                    name: vname,
                    span: vspan,
                }),
                None => {
                    self.bump();
                    continue;
                }
            }
            if !(self.eat(&TokenKind::Semicolon) || self.eat(&TokenKind::Comma)) {
                break;
            }
        }
        let end = self.span_here();
        self.expect(&TokenKind::RBrace, "expected `}` to close the enum");
        Some(EnumDecl {
            name,
            variants,
            span: start.merge(end),
        })
    }

    fn parse_function_decl(&mut self) -> Option<FunctionDecl> {
        let start = self.span_here();
        let api = self.eat_ident("api");
        let is_async = self.eat_ident("async");
        if !self.eat_ident("fn") {
            let span = self.span_here();
            self.error(span, codes::SYNTAX, "expected `fn`");
            return None;
        }
        let (name, _) = self.expect_ident("expected a function name")?;
        let generics = self.parse_type_params();
        let params = self.parse_parameters();
        let ret = if self.eat(&TokenKind::Arrow) {
            Some(self.parse_type())
        } else {
            None
        };
        let effects = self.parse_effects();
        let body = self.parse_block();
        Some(FunctionDecl {
            api,
            is_async,
            name,
            generics,
            params,
            ret,
            effects,
            span: start.merge(body.span),
            body,
        })
    }

    fn parse_parameters(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        self.expect(&TokenKind::LParen, "expected `(`");
        while !self.at(&TokenKind::RParen) && !self.is_eof() {
            let pspan = self.span_here();
            let Some((pname, _)) = self.expect_ident("expected a parameter name") else {
                self.bump();
                continue;
            };
            if !self.expect(&TokenKind::Colon, "expected `:` after the parameter name") {
                break;
            }
            let ty = self.parse_type();
            params.push(Param {
                name: pname,
                ty,
                span: pspan,
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(
            &TokenKind::RParen,
            "expected `)` to close the parameter list",
        );
        params
    }

    fn parse_effects(&mut self) -> Option<Vec<EffectName>> {
        if !self.at_ident("effects") {
            return None;
        }
        self.bump(); // `effects`
        let mut effects = Vec::new();
        self.expect(&TokenKind::LBracket, "expected `[` after `effects`");
        while !self.at(&TokenKind::RBracket) && !self.is_eof() {
            let start = self.span_here();
            let path = self.parse_qualified_name_or_empty("expected an effect name");
            if !path.is_empty() {
                effects.push(EffectName { path, span: start });
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(
            &TokenKind::RBracket,
            "expected `]` to close the effects list",
        );
        Some(effects)
    }

    fn parse_agent_decl(&mut self) -> Option<AgentDecl> {
        let start = self.span_here();
        self.bump(); // `agent`
        let (name, _) = self.expect_ident("expected an agent name")?;
        let params = self.parse_parameters();
        if !self.expect(&TokenKind::Arrow, "expected `->` in the agent signature") {
            return None;
        }
        let ret = self.parse_type();
        let members = self.parse_member_block();
        let end = self.span_here();
        Some(AgentDecl {
            name,
            params,
            ret,
            members,
            span: start.merge(end),
        })
    }

    fn parse_workspace_decl(&mut self) -> Option<WorkspaceDecl> {
        let start = self.span_here();
        self.bump(); // `workspace`
        let (name, _) = self.expect_ident("expected a workspace name")?;
        let members = self.parse_member_block();
        let end = self.span_here();
        Some(WorkspaceDecl {
            name,
            members,
            span: start.merge(end),
        })
    }

    /// `{ name: value (;|,)? ... }` — agent/workspace metadata.
    fn parse_member_block(&mut self) -> Vec<Member> {
        let mut members = Vec::new();
        self.expect(&TokenKind::LBrace, "expected `{`");
        while !self.at(&TokenKind::RBrace) && !self.is_eof() {
            let mspan = self.span_here();
            let Some((name, _)) = self.expect_ident("expected a member name") else {
                self.bump();
                continue;
            };
            if !self.expect(&TokenKind::Colon, "expected `:` after the member name") {
                self.recover_member();
                continue;
            }
            let value = self.parse_expr();
            members.push(Member {
                name,
                value,
                span: mspan,
            });
            self.eat_separator();
        }
        self.expect(&TokenKind::RBrace, "expected `}` to close the member block");
        members
    }

    fn recover_member(&mut self) {
        while !self.is_eof() {
            match self.peek() {
                TokenKind::Semicolon | TokenKind::Comma => {
                    self.bump();
                    return;
                }
                TokenKind::RBrace => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Skip to a plausible declaration boundary after an error.
    fn recover_declaration(&mut self) {
        while !self.is_eof() {
            match self.peek() {
                TokenKind::Semicolon => {
                    self.bump();
                    return;
                }
                TokenKind::RBrace => {
                    self.bump();
                    return;
                }
                TokenKind::Ident(name) if is_decl_start(name) => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    // -- statements ---------------------------------------------------------

    fn parse_block(&mut self) -> Block {
        let start = self.span_here();
        self.expect(&TokenKind::LBrace, "expected `{` to open a block");
        let mut stmts = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.is_eof() {
            let before = self.pos;
            if let Some(stmt) = self.parse_statement() {
                stmts.push(stmt);
            }
            if self.pos == before {
                self.bump();
            }
            self.skip_newlines();
        }
        let end = self.span_here();
        self.expect(&TokenKind::RBrace, "expected `}` to close the block");
        Block {
            stmts,
            span: start.merge(end),
        }
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        self.skip_newlines();
        let start = self.span_here();

        if self.at_ident("let") {
            return self.parse_let();
        }
        if self.at_ident("for") {
            return self.parse_for();
        }
        if self.at_ident("return") {
            self.bump();
            let value = if self.at(&TokenKind::Semicolon) {
                None
            } else {
                Some(self.parse_expr())
            };
            let end = self.span_here();
            self.expect_semicolon();
            return Some(Stmt {
                kind: StmtKind::Return(value),
                span: start.merge(end),
            });
        }

        if let TokenKind::Ident(name) = self.peek().clone() {
            if name == "mut" {
                // handled by parse_let
            } else if FORBIDDEN_STMT_KEYWORDS.contains(&name.as_str()) {
                self.error(
                    start,
                    codes::FORBIDDEN_STMT,
                    format!(
                        "`{name}` is not part of Axiom 1.0; use a bounded standard-library API or `axiom.yaml` configuration"
                    ),
                );
                self.bump();
                self.recover_statement();
                return None;
            }
        }

        // General reassignment `x = ...`, `x += ...` is not supported.
        if self.is_assignment_statement() {
            self.error(
                start,
                codes::NO_ASSIGN,
                "Axiom 1.0 has no general reassignment; bind a new immutable value with `let`",
            );
            self.recover_statement();
            return None;
        }

        // `if`/`match` statements are terminated by their closing `}`.
        let blocky = self.at_ident("if") || self.at_ident("match");
        let value = self.parse_expr();
        let end = self.span_here();
        if blocky {
            let _ = self.eat(&TokenKind::Semicolon);
        } else {
            self.expect_semicolon();
        }
        Some(Stmt {
            kind: StmtKind::Expr(value),
            span: start.merge(end),
        })
    }

    fn is_assignment_statement(&mut self) -> bool {
        // A bare identifier/field path followed by `=` or a compound-assign
        // operator. Calls (`(`) and member reads are excluded by scanning only
        // an identifier/field chain.
        let save = self.pos;
        let mut ok = false;
        if matches!(self.peek(), TokenKind::Ident(_)) {
            ok = true;
            self.bump();
            while self.at(&TokenKind::Dot) {
                self.bump();
                if matches!(self.peek(), TokenKind::Ident(_)) {
                    self.bump();
                } else {
                    ok = false;
                    break;
                }
            }
        }
        let result = ok
            && matches!(
                self.peek(),
                TokenKind::Equals
                    | TokenKind::PlusEq
                    | TokenKind::MinusEq
                    | TokenKind::StarEq
                    | TokenKind::SlashEq
            );
        self.pos = save;
        result
    }

    fn recover_statement(&mut self) {
        while !self.is_eof() {
            match self.peek() {
                TokenKind::Semicolon => {
                    self.bump();
                    return;
                }
                TokenKind::RBrace => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn parse_let(&mut self) -> Option<Stmt> {
        let start = self.span_here();
        self.bump(); // `let`
        if self.at_ident("mut") {
            let span = self.span_here();
            self.error(
                span,
                codes::FORBIDDEN_STMT,
                "`let mut` is not supported; Axiom 1.0 bindings are immutable",
            );
            self.bump();
        }
        let (name, _) = self.expect_ident("expected a binding name after `let`")?;
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };
        if !self.expect(&TokenKind::Equals, "expected `=` in the binding") {
            self.recover_statement();
            return None;
        }
        let value = self.parse_expr();
        let end = self.span_here();
        self.expect_semicolon();
        Some(Stmt {
            kind: StmtKind::Let { name, ty, value },
            span: start.merge(end),
        })
    }

    fn parse_for(&mut self) -> Option<Stmt> {
        let start = self.span_here();
        self.bump(); // `for`
        let (var, _) = self.expect_ident("expected a loop variable after `for`")?;
        if !self.eat_ident("in") {
            let span = self.span_here();
            self.error(span, codes::SYNTAX, "expected `in` after the loop variable");
        }
        let iter = self.parse_expr();
        let body = self.parse_block();
        let span = start.merge(body.span);
        Some(Stmt {
            kind: StmtKind::For { var, iter, body },
            span,
        })
    }

    // -- types --------------------------------------------------------------

    fn parse_type(&mut self) -> TypeExpr {
        let start = self.span_here();
        let mut ty = self.parse_primary_type(start);
        while self.at(&TokenKind::LBracket) && self.peek_nth(1) == &TokenKind::RBracket {
            self.bump();
            let end = self.span_here();
            self.bump();
            ty = TypeExpr {
                kind: TypeKind::List(Box::new(ty)),
                span: start.merge(end),
            };
        }
        ty
    }

    fn parse_primary_type(&mut self, start: Span) -> TypeExpr {
        if self.at(&TokenKind::LBrace) {
            let fields = self.parse_record_type();
            let end = self.span_here();
            return TypeExpr {
                kind: TypeKind::Record(fields),
                span: start.merge(end),
            };
        }
        if self.eat(&TokenKind::LParen) {
            let inner = self.parse_type();
            let end = self.span_here();
            self.expect(&TokenKind::RParen, "expected `)`");
            return TypeExpr {
                kind: inner.kind,
                span: start.merge(end),
            };
        }
        let path = self.parse_qualified_name_or_empty("expected a type name");
        let args = if self.at(&TokenKind::Lt) {
            self.parse_type_arguments()
        } else {
            Vec::new()
        };
        let end = self.span_here();
        TypeExpr {
            kind: TypeKind::Named { path, args },
            span: start.merge(end),
        }
    }

    fn parse_record_type(&mut self) -> Vec<FieldType> {
        let mut fields = Vec::new();
        self.expect(&TokenKind::LBrace, "expected `{`");
        while !self.at(&TokenKind::RBrace) && !self.is_eof() {
            let fspan = self.span_here();
            let Some((fname, _)) = self.expect_ident("expected a field name") else {
                self.bump();
                continue;
            };
            if !self.expect(&TokenKind::Colon, "expected `:` after the field name") {
                self.recover_member();
                continue;
            }
            let ty = self.parse_type();
            fields.push(FieldType {
                name: fname,
                ty,
                span: fspan,
            });
            self.eat_separator();
        }
        self.expect(&TokenKind::RBrace, "expected `}` to close the record type");
        fields
    }

    fn parse_type_arguments(&mut self) -> Vec<TypeExpr> {
        let mut args = Vec::new();
        self.expect(&TokenKind::Lt, "expected `<`");
        while !self.at(&TokenKind::Gt) && !self.is_eof() {
            args.push(self.parse_type());
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::Gt, "expected `>` to close type arguments");
        args
    }

    // -- expressions --------------------------------------------------------

    fn parse_expr(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut lhs = self.parse_and();
        while self.at(&TokenKind::PipePipe) {
            self.bump();
            let rhs = self.parse_and();
            lhs = self.binary(BinaryOp::Or, lhs, rhs);
        }
        lhs
    }

    fn parse_and(&mut self) -> Expr {
        let mut lhs = self.parse_equality();
        while self.at(&TokenKind::AmpAmp) {
            self.bump();
            let rhs = self.parse_equality();
            lhs = self.binary(BinaryOp::And, lhs, rhs);
        }
        lhs
    }

    fn parse_equality(&mut self) -> Expr {
        let mut lhs = self.parse_comparison();
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinaryOp::Eq,
                TokenKind::BangEq => BinaryOp::Ne,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_comparison();
            lhs = self.binary(op, lhs, rhs);
        }
        lhs
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut lhs = self.parse_additive();
        loop {
            let op = match self.peek() {
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::LtEq => BinaryOp::Le,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::GtEq => BinaryOp::Ge,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_additive();
            lhs = self.binary(op, lhs, rhs);
        }
        lhs
    }

    fn parse_additive(&mut self) -> Expr {
        let mut lhs = self.parse_multiplicative();
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_multiplicative();
            lhs = self.binary(op, lhs, rhs);
        }
        lhs
    }

    fn parse_multiplicative(&mut self) -> Expr {
        let mut lhs = self.parse_unary();
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Rem,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_unary();
            lhs = self.binary(op, lhs, rhs);
        }
        lhs
    }

    fn parse_unary(&mut self) -> Expr {
        let start = self.span_here();
        if self.eat(&TokenKind::Bang) {
            let operand = self.parse_unary();
            let span = start.merge(operand.span);
            return Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                },
                span,
            };
        }
        if self.eat(&TokenKind::Minus) {
            let operand = self.parse_unary();
            let span = start.merge(operand.span);
            return Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                },
                span,
            };
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        loop {
            if self.at(&TokenKind::Dot) {
                self.bump();
                if let Some((name, _)) = self.expect_ident("expected a member name after `.`") {
                    let span = expr.span.merge(self.span_here());
                    expr = Expr {
                        kind: ExprKind::Field(Box::new(expr), name),
                        span,
                    };
                } else {
                    break;
                }
            } else if self.at(&TokenKind::LBracket) {
                self.bump();
                let index = self.parse_expr();
                self.expect(&TokenKind::RBracket, "expected `]`");
                let span = expr.span.merge(self.span_here());
                expr = Expr {
                    kind: ExprKind::Index(Box::new(expr), Box::new(index)),
                    span,
                };
            } else if self.at(&TokenKind::LParen) {
                let args = self.parse_arguments();
                let span = expr.span.merge(self.span_here());
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        type_args: Vec::new(),
                        args,
                    },
                    span,
                };
            } else if self.at(&TokenKind::Lt) {
                // Try to read `<T, ...>(` — a named generic call. If it does not
                // resolve, restore and treat `<` as a comparison operator.
                let save_pos = self.pos;
                let save_diags = self.diagnostics.len();
                let type_args = self.parse_type_arguments();
                if self.at(&TokenKind::LParen) && !type_args.is_empty() {
                    let args = self.parse_arguments();
                    let span = expr.span.merge(self.span_here());
                    expr = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(expr),
                            type_args,
                            args,
                        },
                        span,
                    };
                } else {
                    self.pos = save_pos;
                    self.diagnostics.truncate(save_diags);
                    break;
                }
            } else {
                break;
            }
        }
        expr
    }

    fn parse_primary(&mut self) -> Expr {
        let start = self.span_here();
        match self.peek().clone() {
            TokenKind::Integer(value) => {
                self.bump();
                Expr {
                    kind: ExprKind::Int(value),
                    span: start,
                }
            }
            TokenKind::Float(value) => {
                self.bump();
                Expr {
                    kind: ExprKind::Float(value),
                    span: start,
                }
            }
            TokenKind::String(text) => {
                self.bump();
                Expr {
                    kind: ExprKind::Str(text),
                    span: start,
                }
            }
            TokenKind::Duration { millis, unit } => {
                self.bump();
                Expr {
                    kind: ExprKind::Duration { millis, unit },
                    span: start,
                }
            }
            TokenKind::Size { bytes, unit } => {
                self.bump();
                Expr {
                    kind: ExprKind::Size { bytes, unit },
                    span: start,
                }
            }
            TokenKind::LBracket => {
                let args = self.parse_bracket_list();
                let span = start.merge(self.span_here());
                Expr {
                    kind: ExprKind::List(args),
                    span,
                }
            }
            TokenKind::LBrace => {
                let members = self.parse_object_literal();
                let span = start.merge(self.span_here());
                Expr {
                    kind: ExprKind::Record(members),
                    span,
                }
            }
            TokenKind::LParen => {
                self.bump();
                let inner = self.parse_expr();
                let span = start.merge(self.span_here());
                self.expect(&TokenKind::RParen, "expected `)`");
                Expr {
                    kind: inner.kind,
                    span,
                }
            }
            TokenKind::Ident(name) => self.parse_ident_expr(name),
            _ => {
                self.error(start, codes::SYNTAX, "expected an expression");
                self.bump();
                Expr {
                    kind: ExprKind::Ident(String::new()),
                    span: start,
                }
            }
        }
    }

    fn parse_ident_expr(&mut self, name: String) -> Expr {
        let start = self.span_here();
        match name.as_str() {
            "true" | "false" => {
                // Only a boolean literal when not a call/member target.
                if !matches!(self.peek_nth(1), TokenKind::LParen | TokenKind::Dot) {
                    self.bump();
                    return Expr {
                        kind: ExprKind::Bool(name == "true"),
                        span: start,
                    };
                }
                self.bump();
                Expr {
                    kind: ExprKind::Ident(name),
                    span: start,
                }
            }
            "await" => {
                self.bump();
                let operand = self.parse_unary();
                let span = start.merge(operand.span);
                Expr {
                    kind: ExprKind::Await(Box::new(operand)),
                    span,
                }
            }
            "if" => self.parse_if(),
            "match" => self.parse_match(),
            _ => {
                self.bump();
                Expr {
                    kind: ExprKind::Ident(name),
                    span: start,
                }
            }
        }
    }

    fn parse_if(&mut self) -> Expr {
        let start = self.span_here();
        self.bump(); // `if`
        let cond = self.parse_expr();
        let then = self.parse_block();
        let otherwise = if self.eat_ident("else") {
            if self.at_ident("if") {
                Some(Box::new(self.parse_if()))
            } else {
                let block = self.parse_block();
                Some(Box::new(Expr {
                    kind: ExprKind::Block(block.clone()),
                    span: block.span,
                }))
            }
        } else {
            None
        };
        let end = self.span_here();
        Expr {
            kind: ExprKind::If {
                cond: Box::new(cond),
                then,
                otherwise,
            },
            span: start.merge(end),
        }
    }

    fn parse_match(&mut self) -> Expr {
        let start = self.span_here();
        self.bump(); // `match`
        let scrutinee = self.parse_expr();
        self.expect(&TokenKind::LBrace, "expected `{` to open match arms");
        let mut arms = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.is_eof() {
            let before = self.pos;
            let arm_start = self.span_here();
            let pattern = self.parse_pattern();
            if !self.expect(&TokenKind::FatArrow, "expected `=>` in match arm") {
                self.recover_statement();
                continue;
            }
            let body = self.parse_arm_body();
            arms.push(MatchArm {
                pattern,
                span: arm_start.merge(body.span),
                body,
            });
            self.eat_separator();
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span_here();
        self.expect(&TokenKind::RBrace, "expected `}` to close match arms");
        Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            span: start.merge(end),
        }
    }

    fn parse_arm_body(&mut self) -> Block {
        self.skip_newlines();
        if self.at(&TokenKind::LBrace) {
            return self.parse_block();
        }
        let start = self.span_here();
        let mut stmts = Vec::new();
        if self.at_ident("return") {
            self.bump();
            let value = if self.at(&TokenKind::Semicolon) {
                None
            } else {
                Some(self.parse_expr())
            };
            let end = self.span_here();
            stmts.push(Stmt {
                kind: StmtKind::Return(value),
                span: start.merge(end),
            });
        } else if self.at_ident("let") {
            if let Some(stmt) = self.parse_let() {
                stmts.push(stmt);
            }
        } else {
            let value = self.parse_expr();
            let end = self.span_here();
            stmts.push(Stmt {
                kind: StmtKind::Expr(value),
                span: start.merge(end),
            });
        }
        Block {
            stmts,
            span: start.merge(self.span_here()),
        }
    }

    fn parse_pattern(&mut self) -> Pattern {
        let _start = self.span_here();
        match self.peek().clone() {
            TokenKind::Integer(value) => {
                self.bump();
                Pattern::Literal(PatternLiteral::Int(value))
            }
            TokenKind::String(text) => {
                self.bump();
                Pattern::Literal(PatternLiteral::Str(text))
            }
            TokenKind::LParen => {
                self.bump();
                let inner = self.parse_pattern();
                self.expect(&TokenKind::RParen, "expected `)`");
                inner
            }
            TokenKind::Ident(name) if name == "_" => {
                self.bump();
                Pattern::Wildcard
            }
            TokenKind::Ident(name) if name == "true" || name == "false" => {
                self.bump();
                Pattern::Literal(PatternLiteral::Bool(name == "true"))
            }
            TokenKind::Ident(name) => {
                self.bump();
                let mut path = vec![name];
                while self.at(&TokenKind::Dot) {
                    self.bump();
                    if let Some((seg, _)) = self.expect_ident("expected a name after `.`") {
                        path.push(seg);
                    } else {
                        break;
                    }
                }
                if self.at(&TokenKind::LParen) {
                    self.bump();
                    let mut inner = Vec::new();
                    while !self.at(&TokenKind::RParen) && !self.is_eof() {
                        inner.push(self.parse_pattern());
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen, "expected `)` to close the pattern");
                    Pattern::Constructor {
                        path,
                        inner: Some(inner),
                    }
                } else if path.len() == 1 {
                    Pattern::Binding(path.remove(0))
                } else {
                    Pattern::Constructor { path, inner: None }
                }
            }
            _ => {
                let span = self.span_here();
                self.error(span, codes::SYNTAX, "expected a pattern");
                self.bump();
                Pattern::Wildcard
            }
        }
    }

    fn parse_arguments(&mut self) -> Vec<Argument> {
        let mut args = Vec::new();
        self.expect(&TokenKind::LParen, "expected `(`");
        while !self.at(&TokenKind::RParen) && !self.is_eof() {
            let name = if matches!(self.peek(), TokenKind::Ident(_))
                && self.peek_nth(1) == &TokenKind::Colon
            {
                let name = self.current_ident();
                self.bump(); // ident
                self.bump(); // `:`
                name
            } else {
                None
            };
            let value = self.parse_expr();
            args.push(Argument { name, value });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "expected `)` to close the arguments");
        args
    }

    fn parse_bracket_list(&mut self) -> Vec<Argument> {
        let mut items = Vec::new();
        self.expect(&TokenKind::LBracket, "expected `[`");
        while !self.at(&TokenKind::RBracket) && !self.is_eof() {
            let value = self.parse_expr();
            items.push(Argument { name: None, value });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "expected `]` to close the list");
        items
    }

    fn parse_object_literal(&mut self) -> Vec<Member> {
        let mut members = Vec::new();
        self.expect(&TokenKind::LBrace, "expected `{`");
        while !self.at(&TokenKind::RBrace) && !self.is_eof() {
            let mspan = self.span_here();
            let Some((name, _)) = self.expect_ident("expected a field name") else {
                self.bump();
                continue;
            };
            if !self.expect(&TokenKind::Colon, "expected `:` after the field name") {
                self.recover_member();
                continue;
            }
            let value = self.parse_expr();
            members.push(Member {
                name,
                value,
                span: mspan,
            });
            self.eat_separator();
        }
        self.expect(&TokenKind::RBrace, "expected `}` to close the object");
        members
    }

    fn binary(&mut self, op: BinaryOp, lhs: Expr, rhs: Expr) -> Expr {
        let span = lhs.span.merge(rhs.span);
        Expr {
            kind: ExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        }
    }
}

fn is_decl_start(name: &str) -> bool {
    matches!(
        name,
        "module"
            | "import"
            | "export"
            | "type"
            | "enum"
            | "fn"
            | "agent"
            | "workspace"
            | "api"
            | "async"
    )
}
