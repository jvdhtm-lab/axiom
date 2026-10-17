//! Recursive-descent parser: tokens -> AST, with error recovery.
//!
//! The parser never panics on malformed input. On error it records a diagnostic
//! and recovers (to the next declaration at the top level, or the next separator
//! inside a body), so one file can produce several diagnostics in a single pass.

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexer::{tokenize, Token, TokenKind};
use crate::span::Span;

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub file: SourceFile,
    pub diagnostics: Vec<Diagnostic>,
    pub tokens: Vec<Token>,
}

impl ParseResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Parse Axiom source text into an AST plus diagnostics.
pub fn parse(source: &str) -> ParseResult {
    let (tokens, diagnostics) = tokenize(source);
    let mut parser = Parser::new(&tokens, source.len() as u32, diagnostics);
    let file = parser.parse_source_file();
    ParseResult {
        file,
        diagnostics: parser.diagnostics,
        tokens,
    }
}

fn is_decl_keyword(text: &str) -> bool {
    matches!(
        text,
        "integration"
            | "model"
            | "agent"
            | "let"
            | "type"
            | "interface"
            | "contract"
            | "goal"
            | "tool"
            | "runtime"
            | "workflow"
    )
}

struct Parser<'t> {
    tokens: &'t [Token],
    pos: usize,
    source_len: u32,
    diagnostics: Vec<Diagnostic>,
}

impl<'t> Parser<'t> {
    fn new(tokens: &'t [Token], source_len: u32, diagnostics: Vec<Diagnostic>) -> Self {
        Parser {
            tokens,
            pos: 0,
            source_len,
            diagnostics,
        }
    }

    // --- token helpers ---------------------------------------------------

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().expect("token stream is never empty"))
    }

    fn peek_at(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .unwrap_or_else(|| self.tokens.last().expect("token stream is never empty"))
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn eat(&mut self, kind: &TokenKind) -> Option<Span> {
        if &self.peek().kind == kind {
            Some(self.advance().span)
        } else {
            None
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Option<Span> {
        if self.peek().kind == kind {
            Some(self.advance().span)
        } else {
            let found = self.peek().describe();
            self.error_here(format!("expected {what}, found {found}"));
            None
        }
    }

    fn ident_text(&self) -> Option<String> {
        if let TokenKind::Ident(text) = &self.peek().kind {
            Some(text.clone())
        } else {
            None
        }
    }

    fn expect_ident(&mut self, what: &str) -> Option<Ident> {
        if let TokenKind::Ident(text) = &self.peek().kind {
            let text = text.clone();
            let span = self.advance().span;
            Some(Ident { text, span })
        } else {
            let found = self.peek().describe();
            self.error_here(format!("expected {what}, found {found}"));
            None
        }
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(span, message));
    }

    fn error_here(&mut self, message: impl Into<String>) {
        let span = self.peek().span;
        self.error(span, message);
    }

    fn skip_insignificant(&mut self) {
        while matches!(self.peek().kind, TokenKind::Newline) {
            self.advance();
        }
    }

    fn skip_separators(&mut self) {
        while matches!(
            self.peek().kind,
            TokenKind::Newline | TokenKind::Comma | TokenKind::Semicolon
        ) {
            self.advance();
        }
    }

    fn starts_expr(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Ident(_)
                | TokenKind::String(_)
                | TokenKind::Integer(_)
                | TokenKind::Float(_)
                | TokenKind::Duration { .. }
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::LParen
        )
    }

    // --- error recovery --------------------------------------------------

    fn recover_to_separator(&mut self) {
        loop {
            match self.peek().kind {
                TokenKind::Eof | TokenKind::RBrace => break,
                TokenKind::Newline | TokenKind::Comma | TokenKind::Semicolon => {
                    self.advance();
                    break;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn recover_to_declaration(&mut self) {
        let mut depth: i32 = 0;
        loop {
            match &self.peek().kind {
                TokenKind::Eof => break,
                TokenKind::RBrace if depth == 0 => break,
                TokenKind::LBrace => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    self.advance();
                }
                TokenKind::Ident(text) if depth == 0 && is_decl_keyword(text) => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    // --- top level -------------------------------------------------------

    fn parse_source_file(&mut self) -> SourceFile {
        let mut decls = Vec::new();
        loop {
            self.skip_separators();
            if self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.parse_declaration() {
                Some(decl) => decls.push(decl),
                None => {
                    self.recover_to_declaration();
                    if self.pos == before {
                        self.advance();
                    }
                }
            }
        }
        SourceFile {
            decls,
            span: Span::new(0, self.source_len),
        }
    }

    fn parse_declaration(&mut self) -> Option<Decl> {
        let Some(keyword) = self.ident_text() else {
            let found = self.peek().describe();
            self.error_here(format!("expected a declaration, found {found}"));
            return None;
        };
        match keyword.as_str() {
            "integration" => {
                self.advance();
                let (name, entries, span) = self.parse_named_entries()?;
                Some(Decl::Integration(IntegrationDecl {
                    name,
                    entries,
                    span,
                }))
            }
            "model" => {
                self.advance();
                let (name, entries, span) = self.parse_named_entries()?;
                Some(Decl::Model(ModelDecl {
                    name,
                    entries,
                    span,
                }))
            }
            "goal" => {
                self.advance();
                let (name, entries, span) = self.parse_named_entries()?;
                Some(Decl::Goal(GoalDecl {
                    name,
                    entries,
                    span,
                }))
            }
            "contract" => {
                self.advance();
                let (name, entries, span) = self.parse_named_entries()?;
                Some(Decl::Contract(ContractDecl {
                    name,
                    entries,
                    span,
                }))
            }
            "agent" => self.parse_agent_decl(),
            "type" | "interface" => self.parse_type_decl(),
            "tool" => self.parse_tool_decl(),
            "runtime" => self.parse_runtime_decl(),
            "workflow" => self.parse_workflow_decl(),
            "let" => self.parse_instance_decl(),
            other => {
                let span = self.peek().span;
                self.error(span, format!("unknown declaration keyword `{other}`"));
                None
            }
        }
    }

    fn parse_named_entries(&mut self) -> Option<(Ident, Vec<Entry>, Span)> {
        let name = self.expect_ident("a name")?;
        let (entries, block) = self.parse_block_entries()?;
        let span = name.span.merge(block);
        Some((name, entries, span))
    }

    /// Parse `{ entry* }` where entries are separated by line breaks, commas, or
    /// semicolons. Returns the entries and the span of the braces.
    fn parse_block_entries(&mut self) -> Option<(Vec<Entry>, Span)> {
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut entries = Vec::new();
        loop {
            self.skip_separators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                return Some((entries, open.merge(close)));
            }
            if self.at_eof() {
                self.error_here("unclosed `{`; expected `}`");
                return Some((entries, open));
            }
            match self.parse_entry() {
                Some(entry) => entries.push(entry),
                None => self.recover_to_separator(),
            }
        }
    }

    fn parse_entry(&mut self) -> Option<Entry> {
        let key = self.expect_ident("an entry name")?;
        self.expect(TokenKind::Colon, "`:`")?;
        let value = self.parse_value()?;
        let span = key.span.merge(value.span());
        Some(Entry { key, value, span })
    }

    fn parse_agent_decl(&mut self) -> Option<Decl> {
        self.advance(); // agent
        let name = self.expect_ident("an agent name")?;
        self.skip_insignificant();
        let implements = if self.peek().kind.is_ident("implements") {
            self.advance();
            Some(self.parse_type_ref()?)
        } else {
            None
        };
        self.skip_insignificant();
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut items = Vec::new();
        loop {
            self.skip_separators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                let span = name.span.merge(open).merge(close);
                return Some(Decl::Agent(AgentDecl {
                    name,
                    implements,
                    items,
                    span,
                }));
            }
            if self.at_eof() {
                self.error_here("unclosed agent body; expected `}`");
                break;
            }
            match self.parse_agent_item() {
                Some(item) => items.push(item),
                None => self.recover_to_separator(),
            }
        }
        let span = name.span.merge(open);
        Some(Decl::Agent(AgentDecl {
            name,
            implements,
            items,
            span,
        }))
    }

    fn parse_agent_item(&mut self) -> Option<AgentItem> {
        for (keyword, make) in [("permissions", true), ("constraints", false)] {
            if self.peek().kind.is_ident(keyword) {
                let save = self.pos;
                self.advance();
                self.skip_insignificant();
                if matches!(self.peek().kind, TokenKind::LBrace) {
                    let (entries, span) = self.parse_block_entries()?;
                    return Some(if make {
                        AgentItem::Permissions(entries, span)
                    } else {
                        AgentItem::Constraints(entries, span)
                    });
                }
                self.pos = save;
            }
        }
        self.parse_entry().map(AgentItem::Entry)
    }

    fn parse_runtime_decl(&mut self) -> Option<Decl> {
        let keyword = self.advance(); // runtime
        let name = if matches!(self.peek().kind, TokenKind::Ident(_)) {
            Some(self.expect_ident("a runtime name")?)
        } else {
            None
        };
        let (entries, block) = self.parse_block_entries()?;
        Some(Decl::Runtime(RuntimeDecl {
            name,
            entries,
            span: keyword.span.merge(block),
        }))
    }

    fn parse_tool_decl(&mut self) -> Option<Decl> {
        let keyword = self.advance(); // tool
        let name = self.parse_dotted_name("a tool name")?;
        let params = self.parse_params()?;
        let (entries, block) = self.parse_block_entries()?;
        Some(Decl::Tool(ToolDecl {
            name,
            params,
            entries,
            span: keyword.span.merge(block),
        }))
    }

    /// A possibly-dotted name such as `repository.read`. Stored as a single
    /// `Ident` whose text is joined with `.`.
    fn parse_dotted_name(&mut self, what: &str) -> Option<Ident> {
        let first = self.expect_ident(what)?;
        let mut text = first.text.clone();
        let mut span = first.span;
        loop {
            if !matches!(self.peek().kind, TokenKind::Dot)
                || !matches!(self.peek_at(1).kind, TokenKind::Ident(_))
            {
                break;
            }
            self.advance(); // `.`
            let segment = self.expect_ident("a name after `.`")?;
            text.push('.');
            text.push_str(&segment.text);
            span = span.merge(segment.span);
        }
        Some(Ident { text, span })
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        self.skip_insignificant();
        let mut params = Vec::new();
        if self.eat(&TokenKind::LParen).is_none() {
            return Some(params);
        }
        loop {
            self.skip_insignificant();
            if self.eat(&TokenKind::RParen).is_some() {
                return Some(params);
            }
            if self.at_eof() {
                self.error_here("unclosed `(` in parameters; expected `)`");
                return None;
            }
            let name = self.expect_ident("a parameter name")?;
            self.skip_insignificant();
            let mut span = name.span;
            let ty = if self.eat(&TokenKind::Colon).is_some() {
                let ty = self.parse_type_ref()?;
                span = span.merge(ty.span);
                Some(ty)
            } else {
                None
            };
            params.push(Param { name, ty, span });
            self.skip_insignificant();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            if self.eat(&TokenKind::RParen).is_some() {
                return Some(params);
            }
            self.error_here(format!(
                "expected `,` or `)`, found {}",
                self.peek().describe()
            ));
            return None;
        }
    }

    fn parse_type_decl(&mut self) -> Option<Decl> {
        let keyword = self.advance(); // type | interface
        let kind = if keyword.kind.is_ident("interface") {
            TypeKind::Interface
        } else {
            TypeKind::Type
        };
        let name = self.expect_ident("a type name")?;
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        loop {
            self.skip_separators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                let span = name.span.merge(open).merge(close);
                return Some(Decl::Type(TypeDecl {
                    kind,
                    name,
                    fields,
                    span,
                }));
            }
            if self.at_eof() {
                self.error_here("unclosed type body; expected `}`");
                break;
            }
            match self.parse_field() {
                Some(field) => fields.push(field),
                None => self.recover_to_separator(),
            }
        }
        let span = name.span.merge(open);
        Some(Decl::Type(TypeDecl {
            kind,
            name,
            fields,
            span,
        }))
    }

    fn parse_field(&mut self) -> Option<Field> {
        let name = self.expect_ident("a field name")?;
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.parse_type_ref()?;
        let span = name.span.merge(ty.span);
        Some(Field { name, ty, span })
    }

    fn parse_type_ref(&mut self) -> Option<TypeRef> {
        let first = self.expect_ident("a type name")?;
        let first_span = first.span;
        let mut path = vec![first];
        loop {
            self.skip_insignificant();
            if self.eat(&TokenKind::Dot).is_none() {
                break;
            }
            self.skip_insignificant();
            path.push(self.expect_ident("a name after `.`")?);
        }
        self.skip_insignificant();
        let mut end = path.last().map(|i| i.span).unwrap_or(first_span);
        let mut args = Vec::new();
        if self.eat(&TokenKind::Lt).is_some() {
            loop {
                self.skip_insignificant();
                if let Some(close) = self.eat(&TokenKind::Gt) {
                    end = close;
                    break;
                }
                if self.at_eof() {
                    self.error_here("unclosed `<`; expected `>`");
                    break;
                }
                args.push(self.parse_type_ref()?);
                self.skip_insignificant();
                if self.eat(&TokenKind::Comma).is_some() {
                    continue;
                }
                if let Some(close) = self.eat(&TokenKind::Gt) {
                    end = close;
                    break;
                }
                self.error_here(format!(
                    "expected `,` or `>`, found {}",
                    self.peek().describe()
                ));
                break;
            }
        }
        Some(TypeRef {
            path,
            args,
            span: first_span.merge(end),
        })
    }

    fn parse_instance_decl(&mut self) -> Option<Decl> {
        let keyword = self.advance(); // let
        let name = self.expect_ident("a binding name")?;
        self.expect(TokenKind::Equals, "`=`")?;
        let value = self.parse_expr()?;
        let span = keyword.span.merge(value.span());
        Some(Decl::Instance(InstanceDecl { name, value, span }))
    }

    fn parse_workflow_decl(&mut self) -> Option<Decl> {
        let keyword = self.advance(); // workflow
        let name = self.expect_ident("a workflow name")?;
        let params = self.parse_params()?;
        self.skip_insignificant();
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let (items, close) = self.parse_workflow_items()?;
        Some(Decl::Workflow(WorkflowDecl {
            name,
            params,
            items,
            span: keyword.span.merge(open).merge(close),
        }))
    }

    /// Parse workflow items after `{`, consuming through the matching `}`.
    fn parse_workflow_items(&mut self) -> Option<(Vec<WorkflowItem>, Span)> {
        let mut items = Vec::new();
        loop {
            self.skip_separators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                return Some((items, close));
            }
            if self.at_eof() {
                self.error_here("unclosed workflow body; expected `}`");
                return None;
            }
            match self.parse_workflow_item() {
                Some(item) => items.push(item),
                None => self.recover_to_separator(),
            }
        }
    }

    fn parse_workflow_item(&mut self) -> Option<WorkflowItem> {
        self.skip_insignificant();
        if self.peek().kind.is_ident("let") {
            let keyword = self.advance();
            let name = self.expect_ident("a binding name")?;
            self.expect(TokenKind::Equals, "`=`")?;
            let value = self.parse_expr()?;
            let span = keyword.span.merge(value.span());
            return Some(WorkflowItem::Let { name, value, span });
        }
        if self.peek().kind.is_ident("return") {
            let keyword = self.advance();
            self.skip_insignificant();
            let value = if self.starts_expr() {
                Some(self.parse_expr()?)
            } else {
                None
            };
            let span = value
                .as_ref()
                .map(|v| keyword.span.merge(v.span()))
                .unwrap_or(keyword.span);
            return Some(WorkflowItem::Return { value, span });
        }
        if self.peek().kind.is_ident("require") {
            let keyword = self.advance();
            let condition = self.parse_expr()?;
            let mut span = keyword.span.merge(condition.span());
            self.skip_insignificant();
            let message = if self.eat(&TokenKind::Comma).is_some() {
                let token = self.peek().clone();
                if let TokenKind::String(text) = token.kind {
                    self.advance();
                    span = span.merge(token.span);
                    Some(StringLit {
                        value: text,
                        span: token.span,
                    })
                } else {
                    self.error_here("expected a message string after `,`");
                    None
                }
            } else {
                None
            };
            return Some(WorkflowItem::Require {
                condition,
                message,
                span,
            });
        }
        if self.peek().kind.is_ident("parallel") {
            let keyword = self.advance();
            self.skip_insignificant();
            self.expect(TokenKind::LBrace, "`{`")?;
            let (items, close) = self.parse_workflow_items()?;
            return Some(WorkflowItem::Parallel {
                items,
                span: keyword.span.merge(close),
            });
        }
        if self.peek().kind.is_ident("if") {
            return self.parse_if_item();
        }
        let expr = self.parse_expr()?;
        Some(WorkflowItem::Expr(expr))
    }

    fn parse_if_item(&mut self) -> Option<WorkflowItem> {
        let keyword = self.advance(); // if
        let condition = self.parse_expr()?;
        self.skip_insignificant();
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let (then_items, then_close) = self.parse_workflow_items()?;
        let mut span = keyword.span.merge(open).merge(then_close);
        let mut else_items = Vec::new();
        self.skip_separators();
        if self.peek().kind.is_ident("else") {
            let elseif = self.advance();
            self.skip_insignificant();
            if self.peek().kind.is_ident("if") {
                let nested = self.parse_if_item()?;
                span = span.merge(nested.span());
                else_items.push(nested);
            } else {
                self.expect(TokenKind::LBrace, "`{`")?;
                let (items, close) = self.parse_workflow_items()?;
                span = span.merge(elseif.span).merge(close);
                else_items = items;
            }
        }
        Some(WorkflowItem::If {
            condition,
            then_items,
            else_items,
            span,
        })
    }

    // --- expressions -----------------------------------------------------

    fn parse_expr(&mut self) -> Option<Expr> {
        self.skip_insignificant();
        let mut base = self.parse_primary()?;

        loop {
            self.skip_insignificant();
            if self.eat(&TokenKind::Dot).is_none() {
                break;
            }
            self.skip_insignificant();
            if self.peek().kind.is_ident("with") {
                self.advance();
                self.skip_insignificant();
                let (entries, object_span) = self.parse_object_entries()?;
                let span = base.span().merge(object_span);
                base = Expr::With {
                    base: Box::new(base),
                    entries,
                    span,
                };
                continue;
            }
            let field = self.expect_ident("a name after `.`")?;
            match &mut base {
                Expr::Path(path, path_span) => {
                    *path_span = path_span.merge(field.span);
                    path.push(field);
                }
                other => {
                    let span = other.span().merge(field.span);
                    self.error(span, "member access is only supported on a name");
                    break;
                }
            }
        }

        self.skip_insignificant();
        if let Expr::Path(path, path_span) = base {
            if self.eat(&TokenKind::LParen).is_some() {
                let args = self.parse_expr_args()?;
                let span = path_span.merge(self.tokens[self.pos.saturating_sub(1)].span);
                return Some(Expr::Call {
                    callee: path,
                    args,
                    span,
                });
            }
            return Some(Expr::Path(path, path_span));
        }
        Some(base)
    }

    /// Parse comma-separated expressions up to and including the closing `)`.
    fn parse_expr_args(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        loop {
            self.skip_insignificant();
            if self.eat(&TokenKind::RParen).is_some() {
                return Some(args);
            }
            if self.at_eof() {
                self.error_here("unclosed `(`; expected `)`");
                return None;
            }
            args.push(self.parse_expr()?);
            self.skip_insignificant();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            if self.eat(&TokenKind::RParen).is_some() {
                return Some(args);
            }
            self.error_here(format!(
                "expected `,` or `)`, found {}",
                self.peek().describe()
            ));
            return None;
        }
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        self.skip_insignificant();
        let token = self.peek().clone();
        match token.kind {
            TokenKind::String(text) => {
                self.advance();
                Some(Expr::String(StringLit {
                    value: text,
                    span: token.span,
                }))
            }
            TokenKind::Integer(value) => {
                self.advance();
                Some(Expr::Integer(value, token.span))
            }
            TokenKind::Float(value) => {
                self.advance();
                Some(Expr::Float(value, token.span))
            }
            TokenKind::Duration { millis, unit } => {
                self.advance();
                Some(Expr::Duration {
                    millis,
                    unit,
                    span: token.span,
                })
            }
            TokenKind::LBracket => self.parse_array_expr(),
            TokenKind::LBrace => {
                let (entries, span) = self.parse_object_entries()?;
                Some(Expr::Object(entries, span))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.skip_insignificant();
                self.expect(TokenKind::RParen, "`)`")?;
                Some(expr)
            }
            TokenKind::Ident(name) => {
                self.advance();
                let is_bool = name == "true" || name == "false";
                if is_bool {
                    self.skip_insignificant();
                    if !matches!(self.peek().kind, TokenKind::LParen | TokenKind::Dot) {
                        return Some(Expr::Bool(name == "true", token.span));
                    }
                }
                Some(Expr::Path(
                    vec![Ident {
                        text: name,
                        span: token.span,
                    }],
                    token.span,
                ))
            }
            _ => {
                self.error_here(format!(
                    "expected an expression, found {}",
                    token.describe()
                ));
                None
            }
        }
    }

    fn parse_array_expr(&mut self) -> Option<Expr> {
        let open = self.expect(TokenKind::LBracket, "`[`")?;
        let mut items = Vec::new();
        loop {
            self.skip_insignificant();
            if let Some(close) = self.eat(&TokenKind::RBracket) {
                return Some(Expr::Array(items, open.merge(close)));
            }
            if self.at_eof() {
                self.error_here("unclosed `[`; expected `]`");
                return None;
            }
            items.push(self.parse_expr()?);
            self.skip_insignificant();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            if let Some(close) = self.eat(&TokenKind::RBracket) {
                return Some(Expr::Array(items, open.merge(close)));
            }
            self.error_here(format!(
                "expected `,` or `]`, found {}",
                self.peek().describe()
            ));
            return None;
        }
    }

    // --- values ----------------------------------------------------------

    fn parse_value(&mut self) -> Option<Value> {
        self.skip_insignificant();
        let token = self.peek().clone();
        match token.kind {
            TokenKind::String(text) => {
                self.advance();
                Some(Value::String(StringLit {
                    value: text,
                    span: token.span,
                }))
            }
            TokenKind::Integer(value) => {
                self.advance();
                Some(Value::Integer(value, token.span))
            }
            TokenKind::Float(value) => {
                self.advance();
                Some(Value::Float(value, token.span))
            }
            TokenKind::Duration { millis, unit } => {
                self.advance();
                Some(Value::Duration {
                    millis,
                    unit,
                    span: token.span,
                })
            }
            TokenKind::LBracket => self.parse_array_value(),
            TokenKind::LBrace => {
                let (entries, span) = self.parse_object_entries()?;
                Some(Value::Object(entries, span))
            }
            TokenKind::Ident(_) => self.parse_value_ident(),
            _ => {
                let found = token.describe();
                self.error_here(format!("expected a value, found {found}"));
                None
            }
        }
    }

    fn parse_value_ident(&mut self) -> Option<Value> {
        let first = self.expect_ident("a value")?;
        let first_span = first.span;
        let mut path = vec![first];

        loop {
            self.skip_insignificant();
            if self.eat(&TokenKind::Dot).is_none() {
                break;
            }
            self.skip_insignificant();
            path.push(self.expect_ident("a name after `.`")?);
        }

        self.skip_insignificant();
        if self.eat(&TokenKind::LParen).is_some() {
            let (args, close) = self.parse_value_args()?;
            let callee = path.pop().expect("path is non-empty");
            return Some(Value::Call(CallValue {
                callee,
                args,
                span: first_span.merge(close),
            }));
        }

        if path.len() == 1 {
            if path[0].text == "true" {
                return Some(Value::Bool(true, path[0].span));
            }
            if path[0].text == "false" {
                return Some(Value::Bool(false, path[0].span));
            }
        }
        let last = path.last().map(|i| i.span).unwrap_or(first_span);
        Some(Value::Path(path, first_span.merge(last)))
    }

    /// Parse call arguments after the opening `(` through the closing `)`.
    fn parse_value_args(&mut self) -> Option<(Vec<ValueArg>, Span)> {
        let mut args = Vec::new();
        loop {
            self.skip_insignificant();
            if let Some(close) = self.eat(&TokenKind::RParen) {
                return Some((args, close));
            }
            if self.at_eof() {
                self.error_here("unclosed `(` in call; expected `)`");
                return None;
            }
            args.push(self.parse_value_arg()?);
            self.skip_insignificant();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            if let Some(close) = self.eat(&TokenKind::RParen) {
                return Some((args, close));
            }
            self.error_here(format!(
                "expected `,` or `)`, found {}",
                self.peek().describe()
            ));
            return None;
        }
    }

    fn parse_value_arg(&mut self) -> Option<ValueArg> {
        self.skip_insignificant();
        let named = matches!(self.peek().kind, TokenKind::Ident(_))
            && matches!(self.peek_at(1).kind, TokenKind::Colon);
        if named {
            let name = self.expect_ident("an argument name")?;
            self.advance(); // `:`
            let value = self.parse_value()?;
            let span = name.span.merge(value.span());
            return Some(ValueArg {
                name: Some(name),
                value,
                span,
            });
        }
        let value = self.parse_value()?;
        let span = value.span();
        Some(ValueArg {
            name: None,
            value,
            span,
        })
    }

    fn parse_array_value(&mut self) -> Option<Value> {
        let open = self.expect(TokenKind::LBracket, "`[`")?;
        let mut items = Vec::new();
        loop {
            self.skip_insignificant();
            if let Some(close) = self.eat(&TokenKind::RBracket) {
                return Some(Value::Array(items, open.merge(close)));
            }
            if self.at_eof() {
                self.error_here("unclosed `[`; expected `]`");
                return None;
            }
            items.push(self.parse_value()?);
            self.skip_insignificant();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            if let Some(close) = self.eat(&TokenKind::RBracket) {
                return Some(Value::Array(items, open.merge(close)));
            }
            self.error_here(format!(
                "expected `,` or `]`, found {}",
                self.peek().describe()
            ));
            return None;
        }
    }

    /// Parse `{ entry (, entry)* }` for object literals.
    fn parse_object_entries(&mut self) -> Option<(Vec<Entry>, Span)> {
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut entries = Vec::new();
        loop {
            self.skip_insignificant();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                return Some((entries, open.merge(close)));
            }
            if self.at_eof() {
                self.error_here("unclosed `{` in object; expected `}`");
                return None;
            }
            entries.push(self.parse_entry()?);
            self.skip_insignificant();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                return Some((entries, open.merge(close)));
            }
            self.error_here(format!(
                "expected `,` or `}}` in object, found {}",
                self.peek().describe()
            ));
            return None;
        }
    }
}
