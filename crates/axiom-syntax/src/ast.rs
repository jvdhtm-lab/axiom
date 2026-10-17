//! Abstract syntax tree for the Axiom surface language.
//!
//! These nodes model the *syntax* only. Semantic meaning (resolving `model:` to a
//! declared profile, checking types, validating contracts) is Phase 2.

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub decls: Vec<Decl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Integration(IntegrationDecl),
    Model(ModelDecl),
    Agent(AgentDecl),
    Instance(InstanceDecl),
    Type(TypeDecl),
    Contract(ContractDecl),
    Goal(GoalDecl),
    Tool(ToolDecl),
    Runtime(RuntimeDecl),
    Workflow(WorkflowDecl),
}

impl Decl {
    pub fn span(&self) -> Span {
        match self {
            Decl::Integration(d) => d.span,
            Decl::Model(d) => d.span,
            Decl::Agent(d) => d.span,
            Decl::Instance(d) => d.span,
            Decl::Type(d) => d.span,
            Decl::Contract(d) => d.span,
            Decl::Goal(d) => d.span,
            Decl::Tool(d) => d.span,
            Decl::Runtime(d) => d.span,
            Decl::Workflow(d) => d.span,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Decl::Integration(_) => "integration",
            Decl::Model(_) => "model",
            Decl::Agent(_) => "agent",
            Decl::Instance(_) => "agent instance",
            Decl::Type(_) => "type",
            Decl::Contract(_) => "contract",
            Decl::Goal(_) => "goal",
            Decl::Tool(_) => "tool",
            Decl::Runtime(_) => "runtime",
            Decl::Workflow(_) => "workflow",
        }
    }

    pub fn name(&self) -> Option<&Ident> {
        match self {
            Decl::Integration(d) => Some(&d.name),
            Decl::Model(d) => Some(&d.name),
            Decl::Agent(d) => Some(&d.name),
            Decl::Instance(d) => Some(&d.name),
            Decl::Type(d) => Some(&d.name),
            Decl::Contract(d) => Some(&d.name),
            Decl::Goal(d) => Some(&d.name),
            Decl::Tool(d) => Some(&d.name),
            Decl::Runtime(d) => d.name.as_ref(),
            Decl::Workflow(d) => Some(&d.name),
        }
    }
}

/// A `key: value` pair inside a declaration or object literal.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: Ident,
    pub value: Value,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrationDecl {
    pub name: Ident,
    pub entries: Vec<Entry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelDecl {
    pub name: Ident,
    pub entries: Vec<Entry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalDecl {
    pub name: Ident,
    pub entries: Vec<Entry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContractDecl {
    pub name: Ident,
    pub entries: Vec<Entry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeDecl {
    pub name: Option<Ident>,
    pub entries: Vec<Entry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub entries: Vec<Entry>,
    pub span: Span,
}

/// A tool or workflow parameter: a name with an optional type annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Type,
    Interface,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub kind: TypeKind,
    pub name: Ident,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: Ident,
    pub ty: TypeRef,
    pub span: Span,
}

/// A type reference: a path with optional generic arguments, e.g.
/// `Agent<FeatureSpec, ExecutionPlan>` or `List<Task>`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub path: Vec<Ident>,
    pub args: Vec<TypeRef>,
    pub span: Span,
}

impl TypeRef {
    /// Render as a dotted name without generic arguments.
    pub fn name(&self) -> String {
        self.path
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDecl {
    pub name: Ident,
    pub implements: Option<TypeRef>,
    pub items: Vec<AgentItem>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentItem {
    Entry(Entry),
    Permissions(Vec<Entry>, Span),
    Constraints(Vec<Entry>, Span),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceDecl {
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringLit {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValueArg {
    pub name: Option<Ident>,
    pub value: Value,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallValue {
    pub callee: Ident,
    pub args: Vec<ValueArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(StringLit),
    Integer(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Duration {
        millis: u64,
        unit: String,
        span: Span,
    },
    Path(Vec<Ident>, Span),
    Call(CallValue),
    Array(Vec<Value>, Span),
    Object(Vec<Entry>, Span),
}

impl Value {
    pub fn span(&self) -> Span {
        match self {
            Value::String(s) => s.span,
            Value::Integer(_, s) | Value::Float(_, s) | Value::Bool(_, s) => *s,
            Value::Duration { span, .. } => *span,
            Value::Path(_, s) => *s,
            Value::Call(c) => c.span,
            Value::Array(_, s) | Value::Object(_, s) => *s,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Value::String(_) => "string",
            Value::Integer(_, _) => "integer",
            Value::Float(_, _) => "float",
            Value::Bool(_, _) => "boolean",
            Value::Duration { .. } => "duration",
            Value::Path(_, _) => "identifier",
            Value::Call(_) => "call",
            Value::Array(_, _) => "array",
            Value::Object(_, _) => "object",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub items: Vec<WorkflowItem>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowItem {
    Let {
        name: Ident,
        value: Expr,
        span: Span,
    },
    Expr(Expr),
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Require {
        condition: Expr,
        message: Option<StringLit>,
        span: Span,
    },
    Parallel {
        items: Vec<WorkflowItem>,
        span: Span,
    },
    If {
        condition: Expr,
        then_items: Vec<WorkflowItem>,
        else_items: Vec<WorkflowItem>,
        span: Span,
    },
}

impl WorkflowItem {
    pub fn span(&self) -> Span {
        match self {
            WorkflowItem::Let { span, .. } => *span,
            WorkflowItem::Expr(e) => e.span(),
            WorkflowItem::Return { span, .. } => *span,
            WorkflowItem::Require { span, .. } => *span,
            WorkflowItem::Parallel { span, .. } => *span,
            WorkflowItem::If { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Path(Vec<Ident>, Span),
    Call {
        callee: Vec<Ident>,
        args: Vec<Expr>,
        span: Span,
    },
    With {
        base: Box<Expr>,
        entries: Vec<Entry>,
        span: Span,
    },
    String(StringLit),
    Integer(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Duration {
        millis: u64,
        unit: String,
        span: Span,
    },
    Array(Vec<Expr>, Span),
    Object(Vec<Entry>, Span),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Path(_, s) => *s,
            Expr::Call { span, .. } => *span,
            Expr::With { span, .. } => *span,
            Expr::String(s) => s.span,
            Expr::Integer(_, s) | Expr::Float(_, s) | Expr::Bool(_, s) => *s,
            Expr::Duration { span, .. } => *span,
            Expr::Array(_, s) | Expr::Object(_, s) => *s,
        }
    }
}
