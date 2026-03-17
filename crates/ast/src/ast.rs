use super::ast_type::{AstTypeId, AstTypeParam};
use rajac_base::shared_string::SharedString;
use rajac_types::{FieldId as ResolvedFieldId, Ident, MethodId as ResolvedMethodId, TypeId};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span(pub Range<usize>);

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode<T> {
    pub kind: T,
    pub span: Span,
}

impl<T> AstNode<T> {
    pub fn new(kind: T, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Plus,
    Minus,
    Bang,
    Tilde,
    Increment,
    Decrement,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    LShift,
    RShift,
    ARShift,
    Lt,
    LtEq,
    Gt,
    GtEq,
    EqEq,
    BangEq,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    Eq,
    AddEq,
    SubEq,
    MulEq,
    DivEq,
    ModEq,
    AndEq,
    OrEq,
    XorEq,
    LShiftEq,
    RShiftEq,
    ARShiftEq,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiteralKind {
    Int,
    Long,
    Float,
    Double,
    Char,
    String,
    Bool,
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Literal {
    pub kind: LiteralKind,
    pub value: SharedString,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    pub statements: Vec<StmtId>,
    pub source: SharedString,
    pub package: Option<PackageDecl>,
    pub imports: Vec<ImportDecl>,
    pub classes: Vec<ClassDeclId>,
}

impl Ast {
    pub fn new(source: SharedString) -> Self {
        Self {
            statements: Vec::new(),
            source,
            package: None,
            imports: Vec::new(),
            classes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackageDecl {
    pub name: QualifiedName,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub name: QualifiedName,
    pub is_static: bool,
    pub is_on_demand: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedName {
    pub segments: Vec<SharedString>,
}

impl QualifiedName {
    pub fn new(segments: Vec<SharedString>) -> Self {
        Self { segments }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClassKind {
    Class,
    Interface,
    Enum,
    Record,
    Annotation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub kind: ClassKind,
    pub name: Ident,
    pub type_params: Vec<AstTypeParam>,
    pub extends: Option<AstTypeId>,
    pub implements: Vec<AstTypeId>,
    pub permits: Vec<AstTypeId>,
    pub enum_entries: Vec<EnumEntry>,
    pub members: Vec<ClassMemberId>,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClassMember {
    Field(Field),
    Method(Method),
    Constructor(Constructor),
    StaticBlock(StmtId),
    NestedClass(ClassDeclId),
    NestedInterface(ClassDeclId),
    NestedEnum(ClassDeclId),
    NestedRecord(ClassDeclId),
    NestedAnnotation(ClassDeclId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constructor {
    pub name: Ident,
    pub params: Vec<ParamId>,
    pub body: Option<StmtId>,
    pub throws: Vec<AstTypeId>,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumEntry {
    pub name: Ident,
    pub args: Vec<ExprId>,
    pub body: Option<Vec<ClassMemberId>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Empty,
    Block(Vec<StmtId>),
    Expr(ExprId),
    If {
        condition: ExprId,
        then_branch: StmtId,
        else_branch: Option<StmtId>,
    },
    While {
        condition: ExprId,
        body: StmtId,
    },
    DoWhile {
        body: StmtId,
        condition: ExprId,
    },
    For {
        init: Option<ForInit>,
        condition: Option<ExprId>,
        update: Option<ExprId>,
        body: StmtId,
    },
    Switch {
        expr: ExprId,
        cases: Vec<SwitchCase>,
    },
    Return(Option<ExprId>),
    Break(Option<Ident>),
    Continue(Option<Ident>),
    Label(Ident, StmtId),
    Try {
        try_block: StmtId,
        catches: Vec<CatchClause>,
        finally_block: Option<StmtId>,
    },
    Throw(ExprId),
    Synchronized {
        expr: Option<ExprId>,
        block: StmtId,
    },
    LocalVar {
        ty: AstTypeId,
        name: Ident,
        modifiers: Modifiers,
        initializer: Option<ExprId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForInit {
    Expr(ExprId),
    LocalVar {
        ty: AstTypeId,
        name: Ident,
        modifiers: Modifiers,
        initializer: Option<ExprId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatchClause {
    pub param: ParamId,
    pub body: StmtId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub labels: Vec<SwitchLabel>,
    pub body: Vec<StmtId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SwitchLabel {
    Case(ExprId),
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Error,
    Ident(Ident),
    Literal(Literal),
    Unary {
        op: UnaryOp,
        expr: ExprId,
    },
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Assign {
        op: AssignOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Ternary {
        condition: ExprId,
        then_expr: ExprId,
        else_expr: ExprId,
    },
    Cast {
        ty: AstTypeId,
        expr: ExprId,
    },
    InstanceOf {
        expr: ExprId,
        ty: AstTypeId,
    },
    FieldAccess {
        expr: ExprId,
        name: Ident,
        field_id: Option<ResolvedFieldId>,
    },
    MethodCall {
        expr: Option<ExprId>,
        name: Ident,
        type_args: Option<Vec<AstTypeId>>,
        args: Vec<ExprId>,
        method_id: Option<ResolvedMethodId>,
    },
    New {
        ty: AstTypeId,
        args: Vec<ExprId>,
    },
    NewArray {
        ty: AstTypeId,
        dimensions: Vec<ExprId>,
        initializer: Option<ExprId>,
    },
    ArrayInitializer {
        elements: Vec<ExprId>,
    },
    ArrayAccess {
        array: ExprId,
        index: ExprId,
    },
    ArrayLength {
        array: ExprId,
    },
    This(Option<ExprId>),
    ThisCall {
        args: Vec<ExprId>,
        method_id: Option<ResolvedMethodId>,
    },
    Super,
    SuperCall {
        name: Ident,
        type_args: Option<Vec<AstTypeId>>,
        args: Vec<ExprId>,
        method_id: Option<ResolvedMethodId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypedExpr {
    pub expr: Expr,
    pub ty: TypeId,
}

impl TypedExpr {
    pub fn new(expr: Expr) -> Self {
        Self {
            expr,
            ty: TypeId::INVALID,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub ty: AstTypeId,
    pub name: Ident,
    pub modifiers: Modifiers,
    pub varargs: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub name: Ident,
    pub params: Vec<ParamId>,
    pub return_ty: AstTypeId,
    pub body: Option<StmtId>,
    pub throws: Vec<AstTypeId>,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: Ident,
    pub ty: AstTypeId,
    pub initializer: Option<ExprId>,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StmtId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassDeclId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassMemberId(pub u32);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(pub u32);

impl Modifiers {
    pub const PUBLIC: u32 = 0x0001;
    pub const PRIVATE: u32 = 0x0002;
    pub const PROTECTED: u32 = 0x0004;
    pub const STATIC: u32 = 0x0008;
    pub const FINAL: u32 = 0x0010;
    pub const SYNCHRONIZED: u32 = 0x0020;
    pub const VOLATILE: u32 = 0x0040;
    pub const TRANSIENT: u32 = 0x0080;
    pub const NATIVE: u32 = 0x0100;
    pub const INTERFACE: u32 = 0x0200;
    pub const ABSTRACT: u32 = 0x0400;
    pub const STRICTFP: u32 = 0x0800;
    pub const SYNTHETIC: u32 = 0x1000;
    pub const ANNOTATION: u32 = 0x2000;
    pub const ENUM: u32 = 0x4000;
    pub const MODULE: u32 = 0x8000;

    pub fn is_public(&self) -> bool {
        self.0 & Self::PUBLIC != 0
    }

    pub fn is_private(&self) -> bool {
        self.0 & Self::PRIVATE != 0
    }

    pub fn is_protected(&self) -> bool {
        self.0 & Self::PROTECTED != 0
    }

    pub fn is_static(&self) -> bool {
        self.0 & Self::STATIC != 0
    }

    pub fn is_final(&self) -> bool {
        self.0 & Self::FINAL != 0
    }

    pub fn is_abstract(&self) -> bool {
        self.0 & Self::ABSTRACT != 0
    }
}
