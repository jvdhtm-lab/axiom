//! The shared, inspectable typed IR.
//!
//! Every node carries its [`Span`] so the IR retains source provenance, and
//! every function records its declared effects and the capabilities those
//! effects require. The IR is target-agnostic: `Program::target` records which
//! backend a particular build is for, but the node shapes are identical.

use axiom_syntax::span::Span;

/// A compilation target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Server,
    WasmBrowser,
}

impl Target {
    pub fn as_str(self) -> &'static str {
        match self {
            Target::Server => "server",
            Target::WasmBrowser => "wasm_browser",
        }
    }
}

/// A resolved type. Named user types are preserved by name; record structure is
/// kept inline where the source declares it.
#[derive(Debug, Clone, PartialEq)]
pub enum IrType {
    Unit,
    Bool,
    Int,
    Float,
    Str,
    Bytes,
    Duration,
    Timestamp,
    List(Box<IrType>),
    Map(Box<IrType>, Box<IrType>),
    Option(Box<IrType>),
    Result(Box<IrType>, Box<IrType>),
    Record(Vec<(String, IrType)>),
    Enum(String),
    Named(String),
    Generic(String),
    Unknown,
}

impl IrType {
    /// A stable, human-readable rendering used by `axiom explain` and the JSON
    /// printer.
    pub fn name(&self) -> String {
        match self {
            IrType::Unit => "()".to_string(),
            IrType::Bool => "bool".to_string(),
            IrType::Int => "int".to_string(),
            IrType::Float => "float".to_string(),
            IrType::Str => "string".to_string(),
            IrType::Bytes => "bytes".to_string(),
            IrType::Duration => "duration".to_string(),
            IrType::Timestamp => "timestamp".to_string(),
            IrType::List(inner) => format!("{}[]", inner.name()),
            IrType::Map(k, v) => format!("Map<{}, {}>", k.name(), v.name()),
            IrType::Option(inner) => format!("Option<{}>", inner.name()),
            IrType::Result(ok, err) => format!("Result<{}, {}>", ok.name(), err.name()),
            IrType::Record(fields) => {
                let inner = fields
                    .iter()
                    .map(|(n, t)| format!("{n}: {}", t.name()))
                    .collect::<Vec<_>>()
                    .join("; ");
                format!("{{ {inner} }}")
            }
            IrType::Enum(name) | IrType::Named(name) | IrType::Generic(name) => name.clone(),
            IrType::Unknown => "_".to_string(),
        }
    }
}

/// A whole program compiled for one target.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub target: Target,
    pub modules: Vec<IrModule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrModule {
    pub name: String,
    pub display: String,
    pub imports: Vec<IrImport>,
    pub types: Vec<IrTypeDecl>,
    pub enums: Vec<IrEnum>,
    pub functions: Vec<IrFunction>,
    pub agents: Vec<IrAgent>,
    pub workspaces: Vec<IrWorkspace>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrImport {
    pub names: Vec<String>,
    pub from: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrTypeDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub ty: IrType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrEnum {
    pub name: String,
    pub variants: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrFunction {
    pub name: String,
    pub exported: bool,
    pub api: bool,
    pub is_async: bool,
    pub generics: Vec<String>,
    pub params: Vec<(String, IrType)>,
    pub ret: IrType,
    pub effects: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub body: IrBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrAgent {
    pub name: String,
    pub exported: bool,
    pub params: Vec<(String, IrType)>,
    pub ret: IrType,
    pub members: Vec<(String, IrExpr)>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrWorkspace {
    pub name: String,
    pub exported: bool,
    pub members: Vec<(String, IrExpr)>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrBlock {
    pub stmts: Vec<IrStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrStmt {
    pub kind: IrStmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrStmtKind {
    Let {
        name: String,
        ty: Option<IrType>,
        value: IrExpr,
    },
    For {
        var: String,
        iter: IrExpr,
        body: IrBlock,
    },
    Return(Option<IrExpr>),
    Expr(IrExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrExpr {
    pub kind: IrExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrExprKind {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    Duration(u64),
    /// A local variable reference.
    Local(String),
    /// A top-level function reference.
    Function(String),
    /// A type/namespace root such as `ui` or `db`.
    Namespace(String),
    List(Vec<IrExpr>),
    Record(Vec<(String, IrExpr)>),
    /// An enum constructor value such as `AppError.NotFound`.
    Variant {
        name: String,
        args: Vec<IrExpr>,
    },
    Field(Box<IrExpr>, String),
    Index(Box<IrExpr>, Box<IrExpr>),
    Call {
        callee: Box<IrExpr>,
        args: Vec<IrArg>,
    },
    Unary {
        op: IrUnaryOp,
        operand: Box<IrExpr>,
    },
    Binary {
        op: IrBinaryOp,
        lhs: Box<IrExpr>,
        rhs: Box<IrExpr>,
    },
    Await(Box<IrExpr>),
    If {
        cond: Box<IrExpr>,
        then: IrBlock,
        otherwise: Option<Box<IrExpr>>,
    },
    Match {
        scrutinee: Box<IrExpr>,
        arms: Vec<IrArm>,
    },
    Block(IrBlock),
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrArg {
    pub name: Option<String>,
    pub value: IrExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrArm {
    pub pattern: IrPattern,
    pub body: IrBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrPattern {
    Wildcard,
    Binding(String),
    Int(i64),
    Str(String),
    Bool(bool),
    Variant {
        name: String,
        inner: Option<Vec<IrPattern>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrUnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrBinaryOp {
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

impl Target {
    /// The set of effects each target may use.
    pub fn allows_effect(self, effect: &str) -> bool {
        match self {
            Target::Server => true,
            Target::WasmBrowser => matches!(
                effect,
                "api.call" | "browser.storage" | "browser.navigation"
            ),
        }
    }
}
