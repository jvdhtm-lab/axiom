//! Abstract syntax tree for the Axiom 1.0 surface language.
//!
//! These nodes model the *syntax* only. Semantic meaning (resolving `model:` to a
//! declared profile, checking types, validating contracts) is Phase 2.
//!
//! Per `agents.md` v4 §5.1: statements and member entries end with `;` and
//! newlines are insignificant whitespace (the lexer emits `Newline` tokens, but
//! the parser discards them). Declaration bodies are *member blocks*:
//! `name { key: value; ... }`.

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringLit {
    pub value: String,
    pub span: Span,
}

/// A parsed module: an optional `module` header, imports, then items.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub module: Option<ModuleDecl>,
    pub imports: Vec<ImportDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}

impl SourceFile {
    pub fn new(
        module: Option<ModuleDecl>,
        imports: Vec<ImportDecl>,
        items: Vec<Item>,
        span: Span,
    ) -> Self {
        SourceFile {
            module,
            imports,
            items,
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDecl {
    pub path: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub names: Vec<Ident>,
    pub from: StringLit,
    pub span: Span,
}

/// A top-level (or nested) declaration, with its optional `export` prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub exported: bool,
    pub kind: ItemKind,
    pub span: Span,
}

impl Item {
    pub fn kind_name(&self) -> &'static str {
        self.kind.kind_name()
    }

    pub fn name(&self) -> Option<&Ident> {
        self.kind.name()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Type(TypeDecl),
    Interface(InterfaceDecl),
    Enum(EnumDecl),
    Const(ConstDecl),
    Function(FunctionDecl),
    Agent(AgentDecl),
    Tool(ToolDecl),
    Query(QueryDecl),
    Operation(OperationDecl),
    Goal(GoalDecl),
    Workflow(WorkflowDecl),
    /// `team` / `contract` / `policy` / `workspace`: a named member block.
    MemberBlock(MemberBlockDecl),
}

impl ItemKind {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ItemKind::Type(_) => "type",
            ItemKind::Interface(_) => "interface",
            ItemKind::Enum(_) => "enum",
            ItemKind::Const(_) => "const",
            ItemKind::Function(_) => "fn",
            ItemKind::Agent(_) => "agent",
            ItemKind::Tool(_) => "tool",
            ItemKind::Query(_) => "query",
            ItemKind::Operation(_) => "operation",
            ItemKind::Goal(_) => "goal",
            ItemKind::Workflow(_) => "workflow",
            ItemKind::MemberBlock(d) => d.kind,
        }
    }

    pub fn name(&self) -> Option<&Ident> {
        match self {
            ItemKind::Type(d) => Some(&d.name),
            ItemKind::Interface(d) => Some(&d.name),
            ItemKind::Enum(d) => Some(&d.name),
            ItemKind::Const(d) => Some(&d.name),
            ItemKind::Function(d) => Some(&d.name),
            ItemKind::Agent(d) => Some(&d.name),
            ItemKind::Tool(d) => Some(&d.name),
            ItemKind::Query(d) => Some(&d.name),
            ItemKind::Operation(d) => Some(&d.name),
            ItemKind::Goal(d) => Some(&d.name),
            ItemKind::Workflow(d) => Some(&d.name),
            ItemKind::MemberBlock(d) => Some(&d.name),
        }
    }
}

// --- declarations ---------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub value: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub properties: Vec<PropertyType>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: Ident,
    pub members: Vec<EnumMember>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumMember {
    pub name: Ident,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: Ident,
    pub mutable: bool,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub pure: bool,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub effects: Option<Vec<Ident>>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDecl {
    pub name: Ident,
    pub implements: TypeExpr,
    pub members: Vec<Member>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub effects: Vec<Ident>,
    /// Metadata member block: `timeout`, `idempotent`, `max_output_bytes`,
    /// `binding`, ...
    pub members: Vec<Member>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub members: Vec<Member>,
    pub span: Span,
}

/// A typed, named, callable SQL operation (`query Name(params) -> T { ... }`).
#[derive(Debug, Clone, PartialEq)]
pub struct QueryDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub members: Vec<Member>,
    pub span: Span,
}

/// A versioned, server-authorized API boundary
/// (`operation Name(input: T) -> R { version: ...; handler: ...; }`).
#[derive(Debug, Clone, PartialEq)]
pub struct OperationDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub members: Vec<Member>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// `team` / `contract` / `policy` / `workspace` declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberBlockDecl {
    pub kind: &'static str,
    pub name: Ident,
    pub members: Vec<Member>,
    pub span: Span,
}

/// An entry inside a member block.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub name: Ident,
    /// Optional secondary label: `postcondition non_empty: ...;`,
    /// `criterion "…": ...;`.
    pub label: Option<MemberLabel>,
    pub value: MemberValue,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemberLabel {
    Ident(Ident),
    String(StringLit),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemberValue {
    /// `name: expr;` (or `name label: expr;`).
    Expr(Expr),
    /// `name { ... }` — a nested member block (`limits { ... }`).
    Block(Vec<Member>),
    /// `Type -> Member;` — routing entries.
    Arrow(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub optional: bool,
    /// `None` for inferred lambda parameters (`x => ...`); `Some` when annotated.
    pub ty: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub span: Span,
}

// --- types ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExprKind {
    Ref {
        path: Vec<Ident>,
        args: Vec<TypeExpr>,
    },
    Array(Box<TypeExpr>),
    Union(Vec<TypeExpr>),
    Record(Vec<PropertyType>),
    Tuple(Vec<TypeExpr>),
    /// A string-literal type such as `"unit_test"` (used in string unions).
    Str(StringLit),
}

impl TypeExpr {
    /// Render as a dotted name; composites render structurally.
    pub fn display_name(&self) -> String {
        match &self.kind {
            TypeExprKind::Ref { path, .. } => path
                .iter()
                .map(|i| i.text.as_str())
                .collect::<Vec<_>>()
                .join("."),
            TypeExprKind::Array(inner) => format!("{}[]", inner.display_name()),
            TypeExprKind::Union(parts) => parts
                .iter()
                .map(TypeExpr::display_name)
                .collect::<Vec<_>>()
                .join(" | "),
            TypeExprKind::Record(_) => "{...}".to_string(),
            TypeExprKind::Str(s) => format!("\"{}\"", s.value),
            TypeExprKind::Tuple(parts) => format!(
                "({})",
                parts
                    .iter()
                    .map(TypeExpr::display_name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    pub fn as_ref_path(&self) -> Option<String> {
        match &self.kind {
            TypeExprKind::Ref { path, .. } => Some(
                path.iter()
                    .map(|i| i.text.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            ),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyType {
    pub name: Ident,
    pub optional: bool,
    pub ty: TypeExpr,
    pub span: Span,
}

// --- statements -----------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatchClause {
    pub binding: Option<Ident>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Const(ConstDecl),
    Assignment {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    For {
        var: Ident,
        iter: Expr,
        body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        max: Option<Expr>,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Yield(Option<Expr>),
    Break,
    Continue,
    Throw(Expr),
    Try {
        body: Vec<Stmt>,
        catches: Vec<CatchClause>,
        finally: Vec<Stmt>,
    },
    Require {
        cond: Expr,
        else_expr: Option<Expr>,
        escalate: Option<Expr>,
    },
    Parallel {
        var: Ident,
        iter: Expr,
        max: Option<Expr>,
        body: Vec<Stmt>,
    },
    Retry {
        attempts: Expr,
        body: Vec<Stmt>,
    },
    Timeout {
        duration: Expr,
        body: Vec<Stmt>,
    },
    OnError {
        param: Param,
        body: Vec<Stmt>,
    },
    Checkpoint(Option<Expr>),
    Emit(Expr),
    Expr(Expr),
    /// A nested declaration inside a block.
    Item(Box<Item>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: MatchBody,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchBody {
    Expr(Expr),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard(Span),
    Binding(Ident),
    Literal(Literal, Span),
    /// A dotted enum-case or qualified pattern such as `DashboardEvent.Refresh`.
    Path(Vec<Ident>, Span),
    Tuple(Vec<Pattern>, Span),
}

// --- expressions ----------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Literal(Literal),
    Path(Vec<Ident>),
    List(Vec<Expr>),
    Object(Vec<ObjectEntry>),
    Lambda {
        params: Vec<Param>,
        body: LambdaBody,
    },
    Await(Box<Expr>),
    Delegate {
        agent: Vec<Ident>,
        method: Ident,
        args: Vec<Argument>,
    },
    Parallel {
        var: Ident,
        iter: Box<Expr>,
        max: Option<Box<Expr>>,
        body: Vec<Stmt>,
    },
    /// `iterate Agent.execute(input) { max_iterations: N; validate: [...]; ... }`
    /// — a bounded generate/validate/repair expression.
    Iterate {
        call: Box<Expr>,
        members: Vec<Member>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Conditional {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Member {
        base: Box<Expr>,
        name: Ident,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Argument>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LambdaBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    Plus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Mul,
    Div,
    Mod,
    Add,
    Sub,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Duration { millis: u64, unit: String },
    Size { bytes: u64, unit: String },
    String(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    pub name: Option<Ident>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectKey {
    Ident(Ident),
    String(StringLit),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectEntry {
    pub key: ObjectKey,
    pub value: Expr,
    pub span: Span,
}

impl ObjectEntry {
    pub fn key_text(&self) -> &str {
        match &self.key {
            ObjectKey::Ident(i) => &i.text,
            ObjectKey::String(s) => &s.value,
        }
    }
}
