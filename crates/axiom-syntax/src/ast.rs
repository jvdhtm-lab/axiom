//! The Axiom 1.0 abstract syntax tree.
//!
//! The surface is deliberately small: the only declaration forms are `type`,
//! `enum`, `fn` (with `api`/`async` modifiers and an optional `effects` clause),
//! `agent`, and `workspace`. Runtime concepts such as iteration, validation,
//! caching, persistence, retries, checkpoints, database queries, tools, and
//! artifact publishing are ordinary calls, not AST nodes.

use crate::span::Span;

/// A parsed `.ax` file: one `module` header, its imports, and its declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub module: Option<ModuleDecl>,
    pub imports: Vec<ImportDecl>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDecl {
    /// Dotted module path, e.g. `["examples", "frontend"]`.
    pub path: Vec<String>,
    pub span: Span,
}

impl ModuleDecl {
    pub fn dotted(&self) -> String {
        self.path.join(".")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    /// Imported names, e.g. `["DashboardInput", "AppError"]`.
    pub names: Vec<String>,
    /// The source string literal, e.g. `"./shared.ax"`.
    pub from: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub exported: bool,
    pub kind: DeclKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeclKind {
    Type(TypeDecl),
    Enum(EnumDecl),
    Function(FunctionDecl),
    Agent(AgentDecl),
    Workspace(WorkspaceDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    /// `api` — published as a typed server operation.
    pub api: bool,
    /// `async` — may use `await`.
    pub is_async: bool,
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    /// `None` when no `effects` clause is present (a pure function);
    /// `Some(list)` otherwise.
    pub effects: Option<Vec<EffectName>>,
    pub body: Block,
    pub span: Span,
}

impl FunctionDecl {
    pub fn is_pure(&self) -> bool {
        self.effects.is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectName {
    /// Dotted effect path, e.g. `["db", "read"]`.
    pub path: Vec<String>,
    pub span: Span,
}

impl EffectName {
    pub fn dotted(&self) -> String {
        self.path.join(".")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub members: Vec<Member>,
    pub span: Span,
}

impl AgentDecl {
    /// The last member with `name` wins, matching record-like metadata.
    pub fn member(&self, name: &str) -> Option<&Expr> {
        self.members
            .iter()
            .rev()
            .find(|m| m.name == name)
            .map(|m| &m.value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceDecl {
    pub name: String,
    pub members: Vec<Member>,
    pub span: Span,
}

impl WorkspaceDecl {
    pub fn member(&self, name: &str) -> Option<&Expr> {
        self.members
            .iter()
            .rev()
            .find(|m| m.name == name)
            .map(|m| &m.value)
    }
}

/// A `name: value` entry inside an agent/workspace member block.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    /// A named type reference, possibly generic: `int`, `Option<T>`, `pkg.Name`.
    Named {
        path: Vec<String>,
        args: Vec<TypeExpr>,
    },
    /// `T[]`.
    List(Box<TypeExpr>),
    /// `{ a: T; b: U }`.
    Record(Vec<FieldType>),
}

impl TypeExpr {
    pub fn dotted(&self) -> Option<String> {
        match &self.kind {
            TypeKind::Named { path, .. } => Some(path.join(".")),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldType {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let {
        name: String,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    For {
        var: String,
        iter: Expr,
        body: Block,
    },
    Return(Option<Expr>),
    Expr(Expr),
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Duration {
        millis: u64,
        unit: String,
    },
    Size {
        bytes: u64,
        unit: String,
    },
    /// A bare identifier (including prelude constructors such as `None`).
    Ident(String),
    /// `[a, b, c]`.
    List(Vec<Argument>),
    /// `{ a: 1, b: 2 }`.
    Record(Vec<Member>),
    /// `expr.name`.
    Field(Box<Expr>, String),
    /// `expr[expr]`.
    Index(Box<Expr>, Box<Expr>),
    /// `callee(args)` or `callee<T>(args)`.
    Call {
        callee: Box<Expr>,
        type_args: Vec<TypeExpr>,
        args: Vec<Argument>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `await expr`.
    Await(Box<Expr>),
    /// `if cond { .. } else { .. }`, usable in statement position.
    If {
        cond: Box<Expr>,
        then: Block,
        otherwise: Option<Box<Expr>>,
    },
    /// `match scrutinee { arms }`.
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// A braced block used as a value.
    Block(Block),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Mul,
    Div,
    Rem,
    Add,
    Sub,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    /// A named argument such as `database: "app"`, or `None` for a positional arg.
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    /// A bare body is wrapped in a single-statement block.
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard,
    Binding(String),
    Literal(PatternLiteral),
    /// `Ok(x)` / `Err(_)` / `DashboardEventKind.Refresh`.
    Constructor {
        path: Vec<String>,
        inner: Option<Vec<Pattern>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternLiteral {
    Int(i64),
    Str(String),
    Bool(bool),
}
