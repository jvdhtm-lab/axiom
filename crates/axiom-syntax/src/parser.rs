//! Recursive-descent parser: tokens -> AST, with error recovery.
//!
//! Implements the Axiom 1.0 grammar (`docs/grammar.ebnf`). The parser never
//! panics on malformed input. On error it records a diagnostic and recovers (to
//! the next declaration at the top level, or the next separator inside a body),
//! so one file can produce several diagnostics in a single pass.

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
///
/// Per `agents.md` v4, newlines are insignificant whitespace: the lexer still
/// emits `Newline` tokens (the retained token stream is returned for tooling),
/// but the parser works on a filtered stream where statements are separated by
/// `;` only.
pub fn parse(source: &str) -> ParseResult {
    let (tokens, diagnostics) = tokenize(source);
    let significant: Vec<Token> = tokens
        .iter()
        .filter(|t| !matches!(t.kind, TokenKind::Newline))
        .cloned()
        .collect();
    let mut parser = Parser::new(&significant, source.len() as u32, diagnostics);
    let file = parser.parse_program();
    ParseResult {
        file,
        diagnostics: parser.diagnostics,
        tokens,
    }
}

const DECL_KEYWORDS: &[&str] = &[
    "type",
    "interface",
    "enum",
    "const",
    "let",
    "fn",
    "pure",
    "agent",
    "team",
    "tool",
    "contract",
    "goal",
    "policy",
    "workspace",
    "workflow",
    "export",
];

fn is_decl_keyword(text: &str) -> bool {
    DECL_KEYWORDS.contains(&text)
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

    fn check(&self, kind: &TokenKind) -> bool {
        &self.peek().kind == kind
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn eat(&mut self, kind: &TokenKind) -> Option<Span> {
        if self.check(kind) {
            Some(self.advance().span)
        } else {
            None
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Option<Span> {
        if self.check(&kind) {
            Some(self.advance().span)
        } else {
            let found = self.peek().describe();
            self.error_here(format!("expected {what}, found {found}"));
            None
        }
    }

    fn peek_ident(&self) -> Option<&str> {
        match &self.peek().kind {
            TokenKind::Ident(text) => Some(text),
            _ => None,
        }
    }

    fn check_keyword(&self, keyword: &str) -> bool {
        self.peek().kind.is_ident(keyword)
    }

    fn ident_text(&self) -> Option<String> {
        self.peek_ident().map(str::to_string)
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

    fn expect_keyword(&mut self, keyword: &str) -> Option<Span> {
        if self.check_keyword(keyword) {
            Some(self.advance().span)
        } else {
            let found = self.peek().describe();
            self.error_here(format!("expected `{keyword}`, found {found}"));
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

    fn skip_newlines(&mut self) {
        while matches!(self.peek().kind, TokenKind::Newline) {
            self.advance();
        }
    }

    fn skip_terminators(&mut self) {
        while matches!(self.peek().kind, TokenKind::Semicolon) {
            self.advance();
        }
    }

    /// Consume a statement/declaration terminator: `;`, or (at the end of a
    /// block) `}`/EOF. Newlines are insignificant in Axiom 1.0.
    fn end_statement(&mut self) {
        if self.check(&TokenKind::Semicolon) {
            self.advance();
            return;
        }
        if self.check(&TokenKind::RBrace) || self.at_eof() {
            return;
        }
        self.error_here("expected `;`");
        self.recover_to_terminator();
    }

    // --- error recovery --------------------------------------------------

    fn recover_to_terminator(&mut self) {
        loop {
            match self.peek().kind {
                TokenKind::Eof | TokenKind::RBrace => break,
                TokenKind::Semicolon => {
                    self.advance();
                    break;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn recover_to_item(&mut self) {
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

    // --- program ---------------------------------------------------------

    fn parse_program(&mut self) -> SourceFile {
        self.skip_terminators();
        let module = if self.check_keyword("module") {
            self.parse_module()
        } else {
            None
        };

        self.skip_terminators();
        let mut imports = Vec::new();
        while self.check_keyword("import") {
            match self.parse_import() {
                Some(import) => imports.push(import),
                None => self.recover_to_terminator(),
            }
            self.skip_terminators();
        }

        let mut items = Vec::new();
        loop {
            self.skip_terminators();
            if self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.parse_item() {
                Some(item) => items.push(item),
                None => {
                    self.recover_to_item();
                    if self.pos == before {
                        self.advance();
                    }
                }
            }
        }

        SourceFile {
            module,
            imports,
            items,
            span: Span::new(0, self.source_len),
        }
    }

    fn parse_module(&mut self) -> Option<ModuleDecl> {
        let keyword = self.advance(); // module
        let path = self.parse_qualified_name("a module path")?;
        let end = path.last().map(|i| i.span).unwrap_or(keyword.span);
        self.end_statement();
        Some(ModuleDecl {
            path,
            span: keyword.span.merge(end),
        })
    }

    fn parse_import(&mut self) -> Option<ImportDecl> {
        let keyword = self.advance(); // import
        self.skip_newlines();
        self.expect(TokenKind::LBrace, "`{`")?;
        let names = self.parse_identifier_list()?;
        self.expect(TokenKind::RBrace, "`}`")?;
        self.skip_newlines();
        self.expect_keyword("from")?;
        self.skip_newlines();
        let from = self.parse_string_lit("an import path string")?;
        let span = keyword.span.merge(from.span);
        self.end_statement();
        Some(ImportDecl { names, from, span })
    }

    // --- items -----------------------------------------------------------

    fn parse_item(&mut self) -> Option<Item> {
        let start = self.peek().span;
        let exported = self.check_keyword("export");
        if exported {
            self.advance();
            self.skip_newlines();
        }

        let keyword = self.ident_text()?;
        let kind = match keyword.as_str() {
            "type" => ItemKind::Type(self.parse_type_decl()?),
            "interface" => ItemKind::Interface(self.parse_interface_decl()?),
            "enum" => ItemKind::Enum(self.parse_enum_decl()?),
            "const" | "let" => ItemKind::Const(self.parse_const_decl()?),
            "fn" => ItemKind::Function(self.parse_function_decl(false)?),
            "pure" => {
                self.advance();
                self.skip_newlines();
                ItemKind::Function(self.parse_function_decl(true)?)
            }
            "agent" => ItemKind::Agent(self.parse_agent_decl()?),
            "team" => ItemKind::MemberBlock(self.parse_member_block_decl("team")?),
            "tool" => ItemKind::Tool(self.parse_tool_decl()?),
            "contract" => ItemKind::MemberBlock(self.parse_member_block_decl("contract")?),
            "goal" => ItemKind::Goal(self.parse_goal_decl()?),
            "policy" => ItemKind::MemberBlock(self.parse_member_block_decl("policy")?),
            "workspace" => ItemKind::MemberBlock(self.parse_member_block_decl("workspace")?),
            "workflow" => ItemKind::Workflow(self.parse_workflow_decl()?),
            other => {
                let span = self.peek().span;
                self.error(span, format!("unknown declaration keyword `{other}`"));
                return None;
            }
        };
        let end = kind.name().map(|n| n.span).unwrap_or(start);
        Some(Item {
            exported,
            kind,
            span: start.merge(end),
        })
    }

    fn parse_type_decl(&mut self) -> Option<TypeDecl> {
        let keyword = self.advance(); // type
        let name = self.expect_ident("a type name")?;
        let type_params = self.parse_optional_type_params();
        self.expect(TokenKind::Equals, "`=`")?;
        let value = self.parse_type_expr()?;
        self.end_statement();
        Some(TypeDecl {
            span: keyword.span.merge(value.span),
            name,
            type_params,
            value,
        })
    }

    fn parse_interface_decl(&mut self) -> Option<InterfaceDecl> {
        let keyword = self.advance(); // interface
        let name = self.expect_ident("an interface name")?;
        let type_params = self.parse_optional_type_params();
        self.skip_newlines();
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut properties = Vec::new();
        let close = loop {
            self.skip_terminators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                break close;
            }
            if self.at_eof() {
                self.error_here("unclosed interface body; expected `}`");
                break open;
            }
            match self.parse_property_decl() {
                Some(property) => properties.push(property),
                None => self.recover_to_terminator(),
            }
        };
        Some(InterfaceDecl {
            span: keyword.span.merge(close),
            name,
            type_params,
            properties,
        })
    }

    fn parse_enum_decl(&mut self) -> Option<EnumDecl> {
        let keyword = self.advance(); // enum
        let name = self.expect_ident("an enum name")?;
        self.skip_newlines();
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut members = Vec::new();
        let close = loop {
            self.skip_terminators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                break close;
            }
            if self.at_eof() {
                self.error_here("unclosed enum body; expected `}`");
                break open;
            }
            let member_name = match self.expect_ident("an enum member") {
                Some(name) => name,
                None => {
                    self.recover_to_terminator();
                    continue;
                }
            };
            let value = if self.eat(&TokenKind::Equals).is_some() {
                self.parse_expression()
            } else {
                None
            };
            let end = value.as_ref().map(|v| v.span).unwrap_or(member_name.span);
            members.push(EnumMember {
                span: member_name.span.merge(end),
                name: member_name,
                value,
            });
            self.skip_newlines();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            self.skip_terminators();
        };
        Some(EnumDecl {
            span: keyword.span.merge(close),
            name,
            members,
        })
    }

    fn parse_const_decl(&mut self) -> Option<ConstDecl> {
        let keyword_text = self.ident_text()?;
        let keyword = self.advance(); // const | let
        let mutable = if keyword_text == "let" && self.check_keyword("mut") {
            self.advance();
            true
        } else {
            false
        };
        let name = self.expect_ident("a binding name")?;
        let ty = if self.eat(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Equals, "`=`")?;
        let value = self.parse_expression()?;
        self.end_statement();
        Some(ConstDecl {
            span: keyword.span.merge(value.span),
            name,
            mutable,
            ty,
            value,
        })
    }

    fn parse_function_decl(&mut self, pure: bool) -> Option<FunctionDecl> {
        let keyword = self.expect_keyword("fn")?;
        let name = self.expect_ident("a function name")?;
        let type_params = self.parse_optional_type_params();
        let params = self.parse_parameters()?;
        let ret = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let effects = self.parse_optional_effects();
        let (body, close) = self.parse_braced_block()?;
        Some(FunctionDecl {
            span: keyword.merge(close),
            pure,
            name,
            type_params,
            params,
            ret,
            effects,
            body,
        })
    }

    fn parse_tool_decl(&mut self) -> Option<ToolDecl> {
        let keyword = self.advance(); // tool
        let name = self.expect_ident("a tool name")?;
        let params = self.parse_parameters()?;
        self.expect(TokenKind::Arrow, "`->`")?;
        let ret = self.parse_type_expr()?;
        let effects = self.parse_optional_effects().unwrap_or_default();
        let (members, close) = self.parse_member_block()?;
        Some(ToolDecl {
            span: keyword.span.merge(close),
            name,
            params,
            ret,
            effects,
            members,
        })
    }

    fn parse_goal_decl(&mut self) -> Option<GoalDecl> {
        let keyword = self.advance(); // goal
        let name = self.expect_ident("a goal name")?;
        let params = self.parse_parameters()?;
        self.expect(TokenKind::Arrow, "`->`")?;
        let ret = self.parse_type_expr()?;
        let (members, close) = self.parse_member_block()?;
        Some(GoalDecl {
            span: keyword.span.merge(close),
            name,
            params,
            ret,
            members,
        })
    }

    /// Parse a member block declaration: `keyword Name { ... }`.
    /// Used by `team`, `contract`, `policy`, and `workspace`.
    fn parse_member_block_decl(&mut self, kind: &'static str) -> Option<MemberBlockDecl> {
        let start = self.expect_keyword(kind)?;
        let name = self.expect_ident("a name")?;
        let (members, close) = self.parse_member_block()?;
        Some(MemberBlockDecl {
            span: start.merge(close),
            kind,
            name,
            members,
        })
    }

    /// Parse `{ member* }` and return the members plus the closing-brace span.
    fn parse_member_block(&mut self) -> Option<(Vec<Member>, Span)> {
        self.skip_terminators();
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut members = Vec::new();
        let close = loop {
            self.skip_terminators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                break close;
            }
            if self.at_eof() {
                self.error_here("unclosed member block; expected `}`");
                break open;
            }
            match self.parse_member() {
                Some(member) => members.push(member),
                None => self.recover_to_terminator(),
            }
        };
        Some((members, close))
    }

    /// Parse one member entry: `name: expr;`, `name label: expr;`,
    /// `name { ... }`, or `Type -> expr;` (routing).
    fn parse_member(&mut self) -> Option<Member> {
        let name = self.expect_ident("a member name")?;
        let label = if matches!(self.peek().kind, TokenKind::Ident(_))
            && matches!(self.peek_at(1).kind, TokenKind::Colon)
        {
            Some(MemberLabel::Ident(self.expect_ident("a member label")?))
        } else if matches!(self.peek().kind, TokenKind::String(_))
            && matches!(self.peek_at(1).kind, TokenKind::Colon)
        {
            Some(MemberLabel::String(
                self.parse_string_lit("a member label")?,
            ))
        } else {
            None
        };

        if self.check(&TokenKind::LBrace) {
            let (block, close) = self.parse_member_block()?;
            let span = name.span.merge(close);
            let _ = self.eat(&TokenKind::Semicolon);
            return Some(Member {
                name,
                label,
                value: MemberValue::Block(block),
                span,
            });
        }

        if self.eat(&TokenKind::Arrow).is_some() {
            let value = self.parse_expression()?;
            let span = name.span.merge(value.span);
            self.end_statement();
            return Some(Member {
                name,
                label,
                value: MemberValue::Arrow(value),
                span,
            });
        }

        self.expect(TokenKind::Colon, "`:`")?;
        let value = self.parse_expression()?;
        let span = name.span.merge(value.span);
        self.end_statement();
        Some(Member {
            name,
            label,
            value: MemberValue::Expr(value),
            span,
        })
    }

    fn parse_workflow_decl(&mut self) -> Option<WorkflowDecl> {
        let keyword = self.advance(); // workflow
        let name = self.expect_ident("a workflow name")?;
        let params = self.parse_parameters()?;
        let ret = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let (body, close) = self.parse_braced_block()?;
        Some(WorkflowDecl {
            span: keyword.span.merge(close),
            name,
            params,
            ret,
            body,
        })
    }

    fn parse_agent_decl(&mut self) -> Option<AgentDecl> {
        let keyword = self.advance(); // agent
        let name = self.expect_ident("an agent name")?;
        self.expect_keyword("implements")?;
        let implements = self.parse_type_expr()?;
        let (members, close) = self.parse_member_block()?;
        Some(AgentDecl {
            span: keyword.span.merge(close),
            name,
            implements,
            members,
        })
    }

    // --- blocks & parameters --------------------------------------------

    /// Parse `{ statement|declaration* }` and return the statements and the span
    /// of the closing brace.
    fn parse_braced_block(&mut self) -> Option<(Vec<Stmt>, Span)> {
        self.skip_newlines();
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut body = Vec::new();
        loop {
            self.skip_terminators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                return Some((body, close));
            }
            if self.at_eof() {
                self.error_here("unclosed `{`; expected `}`");
                return Some((body, open));
            }
            match self.parse_statement_or_item() {
                Some(stmt) => body.push(stmt),
                None => self.recover_to_terminator(),
            }
        }
    }

    fn parse_parameters(&mut self) -> Option<Vec<Param>> {
        self.skip_newlines();
        let mut params = Vec::new();
        if self.eat(&TokenKind::LParen).is_none() {
            return Some(params);
        }
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RParen).is_some() {
                return Some(params);
            }
            if self.at_eof() {
                self.error_here("unclosed `(` in parameters; expected `)`");
                return None;
            }
            let param = self.parse_parameter()?;
            params.push(param);
            self.skip_newlines();
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

    fn parse_parameter(&mut self) -> Option<Param> {
        let name = self.expect_ident("a parameter name")?;
        let optional = self.eat(&TokenKind::Question).is_some();
        let mut span = name.span;
        let ty = if self.eat(&TokenKind::Colon).is_some() {
            let ty = self.parse_type_expr()?;
            span = span.merge(ty.span);
            Some(ty)
        } else {
            None
        };
        let default = if self.eat(&TokenKind::Equals).is_some() {
            let value = self.parse_expression()?;
            span = span.merge(value.span);
            Some(value)
        } else {
            None
        };
        Some(Param {
            name,
            optional,
            ty,
            default,
            span,
        })
    }

    fn parse_optional_effects(&mut self) -> Option<Vec<Ident>> {
        self.skip_newlines();
        if !self.check_keyword("effects") {
            return None;
        }
        self.advance();
        self.skip_newlines();
        self.expect(TokenKind::LBracket, "`[`")?;
        let mut effects = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RBracket).is_some() {
                break;
            }
            if self.at_eof() {
                self.error_here("unclosed effects list; expected `]`");
                break;
            }
            effects.push(self.parse_effect_name()?);
            self.skip_newlines();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            self.expect(TokenKind::RBracket, "`]`")?;
            break;
        }
        Some(effects)
    }

    /// Parse a dotted effect name (`fs.read`) as one identifier.
    fn parse_effect_name(&mut self) -> Option<Ident> {
        let first = self.expect_ident("an effect name")?;
        let mut text = first.text;
        let mut span = first.span;
        while self.check(&TokenKind::Dot) && matches!(self.peek_at(1).kind, TokenKind::Ident(_)) {
            self.advance(); // `.`
            let segment = self.expect_ident("an effect name segment")?;
            text.push('.');
            text.push_str(&segment.text);
            span = span.merge(segment.span);
        }
        Some(Ident { text, span })
    }

    fn parse_optional_type_params(&mut self) -> Vec<Ident> {
        self.skip_newlines();
        if self.eat(&TokenKind::Lt).is_none() {
            return Vec::new();
        }
        let mut names = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::Gt).is_some() {
                break;
            }
            if self.at_eof() {
                self.error_here("unclosed `<`; expected `>`");
                break;
            }
            match self.expect_ident("a type parameter") {
                Some(name) => names.push(name),
                None => break,
            }
            self.skip_newlines();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            if self.eat(&TokenKind::Gt).is_some() {
                break;
            }
            self.error_here(format!(
                "expected `,` or `>`, found {}",
                self.peek().describe()
            ));
            break;
        }
        names
    }

    fn parse_identifier_list(&mut self) -> Option<Vec<Ident>> {
        let mut names = Vec::new();
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RBrace)
                || self.check(&TokenKind::Gt)
                || self.check(&TokenKind::RBracket)
                || self.check(&TokenKind::RParen)
            {
                return Some(names);
            }
            if self.at_eof() {
                self.error_here("unexpected end of file in a name list");
                return Some(names);
            }
            names.push(self.expect_ident("an identifier")?);
            self.skip_newlines();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            return Some(names);
        }
    }

    fn parse_qualified_name(&mut self, what: &str) -> Option<Vec<Ident>> {
        let first = self.expect_ident(what)?;
        let mut path = vec![first];
        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::Dot) || !matches!(self.peek_at(1).kind, TokenKind::Ident(_))
            {
                break;
            }
            self.advance(); // `.`
            self.skip_newlines();
            path.push(self.expect_ident("a name after `.`")?);
        }
        Some(path)
    }

    // --- types -----------------------------------------------------------

    fn parse_type_expr(&mut self) -> Option<TypeExpr> {
        self.skip_newlines();
        let first = self.parse_generic_type()?;
        let mut parts = vec![first];
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::Pipe).is_none() {
                break;
            }
            parts.push(self.parse_generic_type()?);
        }
        if parts.len() == 1 {
            parts.pop()
        } else {
            let span = parts
                .first()
                .map(|p| p.span)
                .unwrap_or(Span::empty(0))
                .merge(parts.last().map(|p| p.span).unwrap_or(Span::empty(0)));
            Some(TypeExpr {
                kind: TypeExprKind::Union(parts),
                span,
            })
        }
    }

    fn parse_generic_type(&mut self) -> Option<TypeExpr> {
        self.skip_newlines();
        let mut base = if self.check(&TokenKind::LBracket) {
            let open = self.advance().span;
            let inner = self.parse_type_expr()?;
            let close = self.expect(TokenKind::RBracket, "`]`")?;
            TypeExpr {
                kind: TypeExprKind::Array(Box::new(inner)),
                span: open.merge(close),
            }
        } else if self.check(&TokenKind::LBrace) {
            let open = self.advance().span;
            let mut properties = Vec::new();
            let close = loop {
                self.skip_newlines();
                if let Some(close) = self.eat(&TokenKind::RBrace) {
                    break close;
                }
                self.skip_terminators();
                if let Some(close) = self.eat(&TokenKind::RBrace) {
                    break close;
                }
                if self.at_eof() {
                    self.error_here("unclosed record type; expected `}`");
                    break open;
                }
                properties.push(self.parse_property_type()?);
                self.skip_newlines();
                if self.eat(&TokenKind::Comma).is_some()
                    || self.eat(&TokenKind::Semicolon).is_some()
                {
                    continue;
                }
            };
            TypeExpr {
                kind: TypeExprKind::Record(properties),
                span: open.merge(close),
            }
        } else if matches!(self.peek().kind, TokenKind::String(_)) {
            let span = self.peek().span;
            let value = self.parse_string_lit("a string literal type")?;
            TypeExpr {
                kind: TypeExprKind::Str(value),
                span,
            }
        } else {
            let path = self.parse_qualified_name("a type name")?;
            let start = path.first().map(|i| i.span).unwrap_or(Span::empty(0));
            let mut end = path.last().map(|i| i.span).unwrap_or(start);
            let mut args = Vec::new();
            self.skip_newlines();
            if self.eat(&TokenKind::Lt).is_some() {
                loop {
                    self.skip_newlines();
                    if let Some(close) = self.eat(&TokenKind::Gt) {
                        end = close;
                        break;
                    }
                    if self.at_eof() {
                        self.error_here("unclosed `<`; expected `>`");
                        break;
                    }
                    args.push(self.parse_type_expr()?);
                    self.skip_newlines();
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
            TypeExpr {
                kind: TypeExprKind::Ref { path, args },
                span: start.merge(end),
            }
        };

        // Postfix array suffix: `T[]` (optionally repeated).
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::LBracket)
                && matches!(self.peek_at(1).kind, TokenKind::RBracket)
            {
                let open = self.advance().span;
                let close = self.advance().span;
                let span = base.span.merge(open).merge(close);
                base = TypeExpr {
                    kind: TypeExprKind::Array(Box::new(base)),
                    span,
                };
                continue;
            }
            break;
        }
        Some(base)
    }

    fn parse_property_type(&mut self) -> Option<PropertyType> {
        let name = self.expect_ident("a property name")?;
        let optional = self.eat(&TokenKind::Question).is_some();
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.parse_type_expr()?;
        Some(PropertyType {
            span: name.span.merge(ty.span),
            name,
            optional,
            ty,
        })
    }

    fn parse_property_decl(&mut self) -> Option<PropertyType> {
        let property = self.parse_property_type()?;
        self.end_statement();
        Some(property)
    }

    // --- statements ------------------------------------------------------

    fn parse_statement_or_item(&mut self) -> Option<Stmt> {
        self.skip_newlines();
        if let Some(keyword) = self.peek_ident() {
            if keyword == "export" || is_decl_keyword(keyword) {
                // `const`/`let` are bindings (handled as statements); other
                // keywords are declarations.
                if keyword != "const" && keyword != "let" {
                    let item = self.parse_item()?;
                    return Some(Stmt {
                        span: item.span,
                        kind: StmtKind::Item(Box::new(item)),
                    });
                }
            }
        }
        self.parse_statement()
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        self.skip_newlines();
        let start = self.peek().span;

        if self.check_keyword("const") || self.check_keyword("let") {
            let decl = self.parse_const_decl()?;
            return Some(Stmt {
                span: decl.span,
                kind: StmtKind::Const(decl),
            });
        }
        if self.check_keyword("if") {
            return self.parse_if_statement();
        }
        if self.check_keyword("match") {
            return self.parse_match_statement();
        }
        if self.check_keyword("for") {
            return self.parse_for_statement();
        }
        if self.check_keyword("while") {
            return self.parse_while_statement();
        }
        if self.check_keyword("return") {
            self.advance();
            self.skip_newlines();
            let value = if self.starts_expression() {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let span = value.as_ref().map(|v| start.merge(v.span)).unwrap_or(start);
            self.end_statement();
            return Some(Stmt {
                span,
                kind: StmtKind::Return(value),
            });
        }
        if self.check_keyword("yield") {
            self.advance();
            self.skip_newlines();
            let value = if self.starts_expression() {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let span = value.as_ref().map(|v| start.merge(v.span)).unwrap_or(start);
            self.end_statement();
            return Some(Stmt {
                span,
                kind: StmtKind::Yield(value),
            });
        }
        if self.check_keyword("break") {
            self.advance();
            self.end_statement();
            return Some(Stmt {
                span: start,
                kind: StmtKind::Break,
            });
        }
        if self.check_keyword("continue") {
            self.advance();
            self.end_statement();
            return Some(Stmt {
                span: start,
                kind: StmtKind::Continue,
            });
        }
        if self.check_keyword("throw") {
            self.advance();
            let value = self.parse_expression()?;
            let span = start.merge(value.span);
            self.end_statement();
            return Some(Stmt {
                span,
                kind: StmtKind::Throw(value),
            });
        }
        if self.check_keyword("try") {
            return self.parse_try_statement();
        }
        if self.check_keyword("require") {
            return self.parse_require_statement();
        }
        if self.check_keyword("parallel") && self.peek_at(1).kind.is_ident("for") {
            return self.parse_parallel_statement();
        }
        if self.check_keyword("retry") {
            return self.parse_retry_statement();
        }
        if self.check_keyword("timeout") {
            let keyword = self.advance(); // timeout
            let duration = self.parse_expression()?;
            let (body, close) = self.parse_braced_block()?;
            return Some(Stmt {
                span: keyword.span.merge(close),
                kind: StmtKind::Timeout { duration, body },
            });
        }
        if self.check_keyword("on_error") {
            let keyword = self.advance(); // on_error
            self.expect(TokenKind::LParen, "`(`")?;
            let param = self.parse_parameter()?;
            self.expect(TokenKind::RParen, "`)`")?;
            let (body, close) = self.parse_braced_block()?;
            return Some(Stmt {
                span: keyword.span.merge(close),
                kind: StmtKind::OnError { param, body },
            });
        }
        if self.check_keyword("checkpoint") {
            self.advance();
            self.skip_newlines();
            let value = if self.starts_expression() {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let span = value.as_ref().map(|v| start.merge(v.span)).unwrap_or(start);
            self.end_statement();
            return Some(Stmt {
                span,
                kind: StmtKind::Checkpoint(value),
            });
        }
        if self.check_keyword("emit") {
            self.advance();
            let value = self.parse_expression()?;
            let span = start.merge(value.span);
            self.end_statement();
            return Some(Stmt {
                span,
                kind: StmtKind::Emit(value),
            });
        }

        // Expression or assignment statement.
        let expr = self.parse_expression()?;
        if let Some(op) = self.peek_assign_op() {
            self.advance();
            self.skip_newlines();
            let value = self.parse_expression()?;
            let span = start.merge(value.span);
            self.end_statement();
            return Some(Stmt {
                span,
                kind: StmtKind::Assignment {
                    target: expr,
                    op,
                    value,
                },
            });
        }
        let span = expr.span;
        self.end_statement();
        Some(Stmt {
            span,
            kind: StmtKind::Expr(expr),
        })
    }

    fn peek_assign_op(&self) -> Option<AssignOp> {
        match self.peek().kind {
            TokenKind::Equals => Some(AssignOp::Assign),
            TokenKind::PlusEq => Some(AssignOp::Add),
            TokenKind::MinusEq => Some(AssignOp::Sub),
            TokenKind::StarEq => Some(AssignOp::Mul),
            TokenKind::SlashEq => Some(AssignOp::Div),
            _ => None,
        }
    }

    fn parse_if_statement(&mut self) -> Option<Stmt> {
        let keyword = self.advance(); // if
        let cond = self.parse_expression()?;
        let (then_body, then_close) = self.parse_braced_block()?;
        let mut span = keyword.span.merge(then_close);
        let mut else_body = Vec::new();
        self.skip_newlines();
        if self.check_keyword("else") {
            let else_kw = self.advance();
            self.skip_newlines();
            if self.check_keyword("if") {
                let nested = self.parse_if_statement()?;
                span = span.merge(nested.span);
                else_body.push(nested);
            } else {
                let (body, close) = self.parse_braced_block()?;
                span = span.merge(else_kw.span).merge(close);
                else_body = body;
            }
        }
        Some(Stmt {
            span,
            kind: StmtKind::If {
                cond,
                then_body,
                else_body,
            },
        })
    }

    fn parse_match_statement(&mut self) -> Option<Stmt> {
        let keyword = self.advance(); // match
        let scrutinee = self.parse_expression()?;
        self.skip_newlines();
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut arms = Vec::new();
        let close = loop {
            self.skip_terminators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                break close;
            }
            if self.at_eof() {
                self.error_here("unclosed match body; expected `}`");
                break open;
            }
            match self.parse_match_arm() {
                Some(arm) => arms.push(arm),
                None => self.recover_to_terminator(),
            }
        };
        Some(Stmt {
            span: keyword.span.merge(close),
            kind: StmtKind::Match { scrutinee, arms },
        })
    }

    fn parse_match_arm(&mut self) -> Option<MatchArm> {
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::FatArrow, "`=>`")?;
        self.skip_newlines();
        let (body, end) = if self.check(&TokenKind::LBrace) {
            let (stmts, close) = self.parse_braced_block()?;
            (MatchBody::Block(stmts), close)
        } else {
            let expr = self.parse_expression()?;
            let span = expr.span;
            (MatchBody::Expr(expr), span)
        };
        let span = pattern.span().merge(end);
        self.skip_newlines();
        let _ = self.eat(&TokenKind::Comma);
        Some(MatchArm {
            pattern,
            span,
            body,
        })
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        self.skip_newlines();
        match &self.peek().kind {
            TokenKind::Ident(text) if text == "_" => {
                let span = self.advance().span;
                Some(Pattern::Wildcard(span))
            }
            TokenKind::Ident(_) => Some(Pattern::Binding(self.expect_ident("a pattern")?)),
            TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::String(_)
            | TokenKind::Duration { .. }
            | TokenKind::Size { .. } => {
                let span = self.peek().span;
                let literal = self.parse_literal()?;
                Some(Pattern::Literal(literal, span))
            }
            TokenKind::LParen => {
                let open = self.advance().span;
                let mut items = Vec::new();
                loop {
                    self.skip_newlines();
                    if let Some(close) = self.eat(&TokenKind::RParen) {
                        return Some(Pattern::Tuple(items, open.merge(close)));
                    }
                    items.push(self.parse_pattern()?);
                    self.skip_newlines();
                    if self.eat(&TokenKind::Comma).is_some() {
                        continue;
                    }
                    let close = self.expect(TokenKind::RParen, "`)`")?;
                    return Some(Pattern::Tuple(items, open.merge(close)));
                }
            }
            _ => {
                self.error_here(format!(
                    "expected a pattern, found {}",
                    self.peek().describe()
                ));
                None
            }
        }
    }

    fn parse_for_statement(&mut self) -> Option<Stmt> {
        let keyword = self.advance(); // for
        let var = self.expect_ident("a loop variable")?;
        self.expect_keyword("in")?;
        let iter = self.parse_expression()?;
        let (body, close) = self.parse_braced_block()?;
        Some(Stmt {
            span: keyword.span.merge(close),
            kind: StmtKind::For { var, iter, body },
        })
    }

    fn parse_while_statement(&mut self) -> Option<Stmt> {
        let keyword = self.advance(); // while
        let cond = self.parse_expression()?;
        let max = if self.check_keyword("max") {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        let (body, close) = self.parse_braced_block()?;
        Some(Stmt {
            span: keyword.span.merge(close),
            kind: StmtKind::While { cond, max, body },
        })
    }

    fn parse_parallel_statement(&mut self) -> Option<Stmt> {
        let keyword = self.advance(); // parallel
        self.expect_keyword("for")?;
        let var = self.expect_ident("a loop variable")?;
        self.expect_keyword("in")?;
        let iter = self.parse_expression()?;
        let max = if self.check_keyword("max") {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        let (body, close) = self.parse_braced_block()?;
        Some(Stmt {
            span: keyword.span.merge(close),
            kind: StmtKind::Parallel {
                var,
                iter,
                max,
                body,
            },
        })
    }

    fn parse_retry_statement(&mut self) -> Option<Stmt> {
        let keyword = self.advance(); // retry
        let attempts = self.parse_expression()?;
        let (body, close) = self.parse_braced_block()?;
        Some(Stmt {
            span: keyword.span.merge(close),
            kind: StmtKind::Retry { attempts, body },
        })
    }

    fn parse_try_statement(&mut self) -> Option<Stmt> {
        let keyword = self.advance(); // try
        let (body, mut span) = self.parse_braced_block()?;
        let mut catches = Vec::new();
        loop {
            self.skip_newlines();
            if !self.check_keyword("catch") {
                break;
            }
            let catch_kw = self.advance();
            self.skip_newlines();
            let binding = if self.eat(&TokenKind::LParen).is_some() {
                let name = self.expect_ident("a catch binding")?;
                self.expect(TokenKind::RParen, "`)`")?;
                self.skip_newlines();
                Some(name)
            } else {
                None
            };
            let (catch_body, catch_close) = self.parse_braced_block()?;
            span = span.merge(catch_kw.span).merge(catch_close);
            catches.push(CatchClause {
                binding,
                body: catch_body,
                span: catch_kw.span.merge(catch_close),
            });
        }
        let mut finally = Vec::new();
        self.skip_newlines();
        if self.check_keyword("finally") {
            let finally_kw = self.advance();
            let (finally_body, finally_close) = self.parse_braced_block()?;
            span = span.merge(finally_kw.span).merge(finally_close);
            finally = finally_body;
        }
        Some(Stmt {
            span: keyword.span.merge(span),
            kind: StmtKind::Try {
                body,
                catches,
                finally,
            },
        })
    }

    fn parse_require_statement(&mut self) -> Option<Stmt> {
        let keyword = self.advance(); // require
        let cond = self.parse_expression()?;
        let mut span = keyword.span.merge(cond.span);
        let mut else_expr = None;
        let mut escalate = None;
        self.skip_newlines();
        if self.check_keyword("else") {
            self.advance();
            self.skip_newlines();
            if self.check_keyword("escalate") {
                self.advance();
                let value = self.parse_expression()?;
                span = span.merge(value.span);
                escalate = Some(value);
            } else {
                let value = self.parse_expression()?;
                span = span.merge(value.span);
                else_expr = Some(value);
            }
        }
        self.end_statement();
        Some(Stmt {
            span,
            kind: StmtKind::Require {
                cond,
                else_expr,
                escalate,
            },
        })
    }

    // --- expressions -----------------------------------------------------

    fn starts_expression(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Ident(_)
                | TokenKind::String(_)
                | TokenKind::Integer(_)
                | TokenKind::Float(_)
                | TokenKind::Duration { .. }
                | TokenKind::Size { .. }
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::LParen
                | TokenKind::Bang
                | TokenKind::Minus
                | TokenKind::Plus
        )
    }

    fn parse_expression(&mut self) -> Option<Expr> {
        self.skip_newlines();
        self.parse_conditional()
    }

    fn parse_conditional(&mut self) -> Option<Expr> {
        let cond = self.parse_binary(0)?;
        self.skip_newlines();
        if self.eat(&TokenKind::Question).is_none() {
            return Some(cond);
        }
        self.skip_newlines();
        let then_expr = self.parse_expression()?;
        self.skip_newlines();
        self.expect(TokenKind::Colon, "`:`")?;
        self.skip_newlines();
        let else_expr = self.parse_expression()?;
        let span = cond.span.merge(else_expr.span);
        Some(Expr {
            span,
            kind: ExprKind::Conditional {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            },
        })
    }

    fn binary_op_precedence(kind: &TokenKind) -> Option<(BinaryOp, u8)> {
        Some(match kind {
            TokenKind::PipePipe => (BinaryOp::Or, 1),
            TokenKind::AmpAmp => (BinaryOp::And, 2),
            TokenKind::EqEq => (BinaryOp::Eq, 3),
            TokenKind::BangEq => (BinaryOp::Ne, 3),
            TokenKind::Lt => (BinaryOp::Lt, 4),
            TokenKind::LtEq => (BinaryOp::LtEq, 4),
            TokenKind::Gt => (BinaryOp::Gt, 4),
            TokenKind::GtEq => (BinaryOp::GtEq, 4),
            TokenKind::Plus => (BinaryOp::Add, 5),
            TokenKind::Minus => (BinaryOp::Sub, 5),
            TokenKind::Star => (BinaryOp::Mul, 6),
            TokenKind::Slash => (BinaryOp::Div, 6),
            TokenKind::Percent => (BinaryOp::Mod, 6),
            _ => return None,
        })
    }

    fn parse_binary(&mut self, min_precedence: u8) -> Option<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            self.skip_newlines();
            let Some((op, precedence)) = Self::binary_op_precedence(&self.peek().kind) else {
                break;
            };
            if precedence < min_precedence {
                break;
            }
            self.advance();
            self.skip_newlines();
            let right = self.parse_binary(precedence + 1)?;
            let span = left.span.merge(right.span);
            left = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            };
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        self.skip_newlines();
        let op = match self.peek().kind {
            TokenKind::Bang => Some(UnaryOp::Not),
            TokenKind::Minus => Some(UnaryOp::Neg),
            TokenKind::Plus => Some(UnaryOp::Plus),
            _ => None,
        };
        if let Some(op) = op {
            let start = self.advance().span;
            let operand = self.parse_unary()?;
            let span = start.merge(operand.span);
            return Some(Expr {
                span,
                kind: ExprKind::Unary {
                    op,
                    operand: Box::new(operand),
                },
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::Dot).is_some() {
                self.skip_newlines();
                let name = self.expect_ident("a name after `.`")?;
                let is_with = name.text == "with";
                let span = expr.span.merge(name.span);
                let member = Expr {
                    span,
                    kind: ExprKind::Member {
                        base: Box::new(expr),
                        name,
                    },
                };
                // `.with { ... }` sugar: an object-literal argument without
                // parentheses (see the canonical agent-instance example).
                if is_with && self.check(&TokenKind::LBrace) {
                    let obj = self.parse_object_literal()?;
                    let call_span = member.span.merge(obj.span);
                    let arg_span = obj.span;
                    expr = Expr {
                        span: call_span,
                        kind: ExprKind::Call {
                            callee: Box::new(member),
                            args: vec![Argument {
                                name: None,
                                value: obj,
                                span: arg_span,
                            }],
                        },
                    };
                } else {
                    expr = member;
                }
                continue;
            }
            if self.check(&TokenKind::LBracket) {
                let open = self.advance().span;
                let index = self.parse_expression()?;
                let close = self.expect(TokenKind::RBracket, "`]`")?;
                let span = expr.span.merge(open).merge(close);
                expr = Expr {
                    span,
                    kind: ExprKind::Index {
                        base: Box::new(expr),
                        index: Box::new(index),
                    },
                };
                continue;
            }
            if self.check(&TokenKind::LParen) {
                let (args, close) = self.parse_arguments()?;
                let span = expr.span.merge(close);
                expr = Expr {
                    span,
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                };
                continue;
            }
            break;
        }
        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        self.skip_newlines();
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::String(_)
            | TokenKind::Duration { .. }
            | TokenKind::Size { .. } => {
                let literal = self.parse_literal()?;
                Some(Expr {
                    span: token.span,
                    kind: ExprKind::Literal(literal),
                })
            }
            TokenKind::LBracket => self.parse_list_literal(),
            TokenKind::LBrace => self.parse_object_literal(),
            TokenKind::LParen => {
                if let Some(lambda) = self.try_parse_lambda() {
                    return Some(lambda);
                }
                self.advance();
                let expr = self.parse_expression()?;
                self.skip_newlines();
                self.expect(TokenKind::RParen, "`)`")?;
                Some(expr)
            }
            TokenKind::Ident(name) => {
                if matches!(self.peek_at(1).kind, TokenKind::FatArrow) {
                    return self.parse_simple_lambda();
                }
                self.parse_ident_expression(name, token.span)
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

    fn parse_ident_expression(&mut self, name: String, span: Span) -> Option<Expr> {
        match name.as_str() {
            "true" | "false" if !self.is_call_or_member() => {
                self.advance();
                return Some(Expr {
                    span,
                    kind: ExprKind::Literal(Literal::Bool(name == "true")),
                });
            }
            "null" if !self.is_call_or_member() => {
                self.advance();
                return Some(Expr {
                    span,
                    kind: ExprKind::Literal(Literal::Null),
                });
            }
            "await" => {
                self.advance();
                let operand = self.parse_unary()?;
                let span = span.merge(operand.span);
                return Some(Expr {
                    span,
                    kind: ExprKind::Await(Box::new(operand)),
                });
            }
            "delegate" => {
                self.advance();
                let mut path = self.parse_qualified_name("an agent name")?;
                if path.len() < 2 {
                    self.error(
                        span,
                        "expected `.<method>` after the agent name in `delegate`",
                    );
                    return None;
                }
                let method = path.pop().expect("len >= 2");
                let (args, close) = self.parse_arguments()?;
                return Some(Expr {
                    span: span.merge(close),
                    kind: ExprKind::Delegate {
                        agent: path,
                        method,
                        args,
                    },
                });
            }
            "parallel"
                if self.peek_at(1).kind.is_ident("for") && !self.check_after_ident_is_dot() =>
            {
                return self.parse_parallel_expression();
            }
            _ => {}
        }
        // An ordinary name: a single-segment path. Dotted access is handled by
        // the postfix parser so `Agent.with { ... }` and method calls work.
        self.advance();
        Some(Expr {
            span,
            kind: ExprKind::Path(vec![Ident { text: name, span }]),
        })
    }

    fn check_after_ident_is_dot(&self) -> bool {
        matches!(self.peek_at(1).kind, TokenKind::Dot)
    }

    fn is_call_or_member(&self) -> bool {
        matches!(self.peek_at(1).kind, TokenKind::LParen | TokenKind::Dot)
    }

    fn parse_parallel_expression(&mut self) -> Option<Expr> {
        let keyword = self.advance(); // parallel
        self.expect_keyword("for")?;
        let var = self.expect_ident("a loop variable")?;
        self.expect_keyword("in")?;
        let iter = self.parse_expression()?;
        let max = if self.check_keyword("max") {
            self.advance();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        let (body, close) = self.parse_braced_block()?;
        Some(Expr {
            span: keyword.span.merge(close),
            kind: ExprKind::Parallel {
                var,
                iter: Box::new(iter),
                max,
                body,
            },
        })
    }

    fn try_parse_lambda(&mut self) -> Option<Expr> {
        let save_pos = self.pos;
        let save_diagnostics = self.diagnostics.len();
        if let Some(lambda) = self.parse_lambda() {
            return Some(lambda);
        }
        self.pos = save_pos;
        self.diagnostics.truncate(save_diagnostics);
        None
    }

    fn parse_lambda(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();
        loop {
            self.skip_newlines();
            if let Some(close) = self.eat(&TokenKind::RParen) {
                self.skip_newlines();
                self.eat(&TokenKind::FatArrow)?;
                let body = self.parse_lambda_body()?;
                let span = start.merge(close).merge(body.span_hint(start));
                return Some(Expr {
                    span,
                    kind: ExprKind::Lambda { params, body },
                });
            }
            if self.at_eof() {
                return None;
            }
            params.push(self.parse_parameter()?);
            self.skip_newlines();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            let close = self.expect(TokenKind::RParen, "`)`")?;
            self.skip_newlines();
            self.eat(&TokenKind::FatArrow)?;
            let body = self.parse_lambda_body()?;
            let span = start.merge(close).merge(body.span_hint(start));
            return Some(Expr {
                span,
                kind: ExprKind::Lambda { params, body },
            });
        }
    }

    /// Single-parameter lambda without parentheses or a type: `x => expr`.
    fn parse_simple_lambda(&mut self) -> Option<Expr> {
        let name = self.expect_ident("a parameter name")?;
        let start = name.span;
        self.expect(TokenKind::FatArrow, "`=>`")?;
        self.skip_newlines();
        let body = self.parse_lambda_body()?;
        let span = start.merge(body.span_hint(start));
        let params = vec![Param {
            name,
            optional: false,
            ty: None,
            default: None,
            span: start,
        }];
        Some(Expr {
            span,
            kind: ExprKind::Lambda { params, body },
        })
    }

    fn parse_lambda_body(&mut self) -> Option<LambdaBody> {
        self.skip_newlines();
        if self.check(&TokenKind::LBrace) {
            let (stmts, _) = self.parse_braced_block()?;
            Some(LambdaBody::Block(stmts))
        } else {
            let expr = self.parse_expression()?;
            Some(LambdaBody::Expr(Box::new(expr)))
        }
    }

    fn parse_list_literal(&mut self) -> Option<Expr> {
        let open = self.expect(TokenKind::LBracket, "`[`")?;
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if let Some(close) = self.eat(&TokenKind::RBracket) {
                return Some(Expr {
                    span: open.merge(close),
                    kind: ExprKind::List(items),
                });
            }
            if self.at_eof() {
                self.error_here("unclosed `[`; expected `]`");
                return Some(Expr {
                    span: open,
                    kind: ExprKind::List(items),
                });
            }
            items.push(self.parse_expression()?);
            self.skip_newlines();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            if let Some(close) = self.eat(&TokenKind::RBracket) {
                return Some(Expr {
                    span: open.merge(close),
                    kind: ExprKind::List(items),
                });
            }
            self.error_here(format!(
                "expected `,` or `]`, found {}",
                self.peek().describe()
            ));
            return None;
        }
    }

    fn parse_object_literal(&mut self) -> Option<Expr> {
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut entries = Vec::new();
        loop {
            self.skip_terminators();
            if let Some(close) = self.eat(&TokenKind::RBrace) {
                return Some(Expr {
                    span: open.merge(close),
                    kind: ExprKind::Object(entries),
                });
            }
            if self.at_eof() {
                self.error_here("unclosed `{` in object; expected `}`");
                return Some(Expr {
                    span: open,
                    kind: ExprKind::Object(entries),
                });
            }
            entries.push(self.parse_object_entry()?);
            self.skip_newlines();
            if self.eat(&TokenKind::Comma).is_some() {
                continue;
            }
            self.skip_newlines();
        }
    }

    fn parse_object_entry(&mut self) -> Option<ObjectEntry> {
        let key = match &self.peek().kind {
            TokenKind::Ident(text) => {
                let text = text.clone();
                let span = self.advance().span;
                ObjectKey::Ident(Ident { text, span })
            }
            TokenKind::String(_) => ObjectKey::String(self.parse_string_lit("a key")?),
            _ => {
                self.error_here(format!(
                    "expected an object key, found {}",
                    self.peek().describe()
                ));
                return None;
            }
        };
        self.expect(TokenKind::Colon, "`:`")?;
        let value = self.parse_expression()?;
        let key_span = match &key {
            ObjectKey::Ident(i) => i.span,
            ObjectKey::String(s) => s.span,
        };
        Some(ObjectEntry {
            span: key_span.merge(value.span),
            key,
            value,
        })
    }

    fn parse_arguments(&mut self) -> Option<(Vec<Argument>, Span)> {
        let open = self.expect(TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        loop {
            self.skip_newlines();
            if let Some(close) = self.eat(&TokenKind::RParen) {
                return Some((args, close));
            }
            if self.at_eof() {
                self.error_here("unclosed `(` in call; expected `)`");
                return Some((args, open));
            }
            args.push(self.parse_argument()?);
            self.skip_newlines();
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

    fn parse_argument(&mut self) -> Option<Argument> {
        self.skip_newlines();
        let named = matches!(self.peek().kind, TokenKind::Ident(_))
            && matches!(self.peek_at(1).kind, TokenKind::Colon);
        if named {
            let name = self.expect_ident("an argument name")?;
            self.advance(); // `:`
            self.skip_newlines();
            let value = self.parse_expression()?;
            let span = name.span.merge(value.span);
            return Some(Argument {
                name: Some(name),
                value,
                span,
            });
        }
        let value = self.parse_expression()?;
        let span = value.span;
        Some(Argument {
            name: None,
            value,
            span,
        })
    }

    // --- literals --------------------------------------------------------

    fn parse_literal(&mut self) -> Option<Literal> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Integer(value) => {
                self.advance();
                Some(Literal::Int(value))
            }
            TokenKind::Float(value) => {
                self.advance();
                Some(Literal::Float(value))
            }
            TokenKind::Duration { millis, unit } => {
                self.advance();
                Some(Literal::Duration { millis, unit })
            }
            TokenKind::Size { bytes, unit } => {
                self.advance();
                Some(Literal::Size { bytes, unit })
            }
            TokenKind::String(value) => {
                self.advance();
                Some(Literal::String(value))
            }
            _ => {
                self.error_here(format!("expected a literal, found {}", token.describe()));
                None
            }
        }
    }

    fn parse_string_lit(&mut self, what: &str) -> Option<StringLit> {
        let token = self.peek().clone();
        if let TokenKind::String(value) = token.kind {
            self.advance();
            Some(StringLit {
                value,
                span: token.span,
            })
        } else {
            self.error_here(format!("expected {what}, found {}", token.describe()));
            None
        }
    }
}

impl Pattern {
    fn span(&self) -> Span {
        match self {
            Pattern::Wildcard(span) => *span,
            Pattern::Binding(ident) => ident.span,
            Pattern::Literal(_, span) => *span,
            Pattern::Tuple(_, span) => *span,
        }
    }
}

impl LambdaBody {
    fn span_hint(&self, fallback: Span) -> Span {
        match self {
            LambdaBody::Expr(expr) => expr.span,
            LambdaBody::Block(stmts) => stmts.last().map(|s| s.span).unwrap_or(fallback),
        }
    }
}
