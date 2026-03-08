use rajac_base::shared_string::SharedString;
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
pub struct Ident(pub SharedString);

impl Ident {
    pub fn new(name: SharedString) -> Self {
        Self(name)
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
    pub type_params: Vec<TypeParam>,
    pub extends: Option<TypeId>,
    pub implements: Vec<TypeId>,
    pub permits: Vec<TypeId>,
    pub members: Vec<ClassMemberId>,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: Ident,
    pub bounds: Vec<TypeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClassMember {
    Field(Field),
    Method(Method),
    Constructor(Constructor),
    StaticBlock(StmtId),
    NestedClass(ClassDeclId),
    NestedInterface(ClassDeclId),
    NestedEnum(EnumDecl),
    NestedRecord(ClassDeclId),
    NestedAnnotation(ClassDeclId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constructor {
    pub name: Ident,
    pub params: Vec<ParamId>,
    pub body: Option<StmtId>,
    pub throws: Vec<TypeId>,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: Ident,
    pub implements: Vec<TypeId>,
    pub entries: Vec<EnumEntry>,
    pub members: Vec<ClassMemberId>,
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
        ty: TypeId,
        name: Ident,
        initializer: Option<ExprId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForInit {
    Expr(ExprId),
    LocalVar {
        ty: TypeId,
        name: Ident,
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
        ty: TypeId,
        expr: ExprId,
    },
    InstanceOf {
        expr: ExprId,
        ty: TypeId,
    },
    FieldAccess {
        expr: ExprId,
        name: Ident,
    },
    MethodCall {
        expr: Option<ExprId>,
        name: Ident,
        type_args: Option<Vec<TypeId>>,
        args: Vec<ExprId>,
    },
    New {
        ty: TypeId,
        args: Vec<ExprId>,
    },
    NewArray {
        ty: TypeId,
        dimensions: Vec<ExprId>,
    },
    ArrayAccess {
        array: ExprId,
        index: ExprId,
    },
    ArrayLength {
        array: ExprId,
    },
    This(Option<ExprId>),
    Super,
    SuperCall {
        name: Ident,
        type_args: Option<Vec<TypeId>>,
        args: Vec<ExprId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Error,
    Primitive(PrimitiveType),
    Class {
        name: Ident,
        type_args: Option<Vec<TypeId>>,
    },
    Array {
        ty: TypeId,
    },
    TypeVariable {
        name: Ident,
    },
    Wildcard {
        bound: Option<WildcardBound>,
    },
    NonCanonical,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrimitiveType {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    Void,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WildcardBound {
    Extends(TypeId),
    Super(TypeId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub ty: TypeId,
    pub name: Ident,
    pub varargs: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub name: Ident,
    pub params: Vec<ParamId>,
    pub return_ty: TypeId,
    pub body: Option<StmtId>,
    pub throws: Vec<TypeId>,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: Ident,
    pub ty: TypeId,
    pub initializer: Option<ExprId>,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StmtId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumDeclId(pub u32);

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

    pub fn is_public(self) -> bool {
        self.0 & Self::PUBLIC != 0
    }

    pub fn is_private(self) -> bool {
        self.0 & Self::PRIVATE != 0
    }

    pub fn is_protected(self) -> bool {
        self.0 & Self::PROTECTED != 0
    }

    pub fn is_static(self) -> bool {
        self.0 & Self::STATIC != 0
    }

    pub fn is_final(self) -> bool {
        self.0 & Self::FINAL != 0
    }

    pub fn is_abstract(self) -> bool {
        self.0 & Self::ABSTRACT != 0
    }
}
