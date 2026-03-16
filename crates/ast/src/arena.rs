use super::ast::*;
use super::ast_type::{AstType, AstTypeId};

#[derive(Debug, Default)]
pub struct AstArena {
    pub stmts: Vec<Stmt>,
    pub exprs: Vec<TypedExpr>,
    pub types: Vec<AstType>,
    pub params: Vec<Param>,
    pub methods: Vec<Method>,
    pub fields: Vec<Field>,
    pub class_decls: Vec<ClassDecl>,
    pub class_members: Vec<ClassMember>,
}

impl AstArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc_stmt(&mut self, stmt: Stmt) -> StmtId {
        let id = StmtId(self.stmts.len() as u32);
        self.stmts.push(stmt);
        id
    }

    pub fn alloc_expr(&mut self, expr: Expr) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(TypedExpr::new(expr));
        id
    }

    pub fn alloc_type(&mut self, ty: AstType) -> AstTypeId {
        let id = AstTypeId(self.types.len() as u32);
        self.types.push(ty);
        id
    }

    pub fn alloc_param(&mut self, param: Param) -> ParamId {
        let id = ParamId(self.params.len() as u32);
        self.params.push(param);
        id
    }

    pub fn alloc_method(&mut self, method: Method) -> MethodId {
        let id = MethodId(self.methods.len() as u32);
        self.methods.push(method);
        id
    }

    pub fn alloc_field(&mut self, field: Field) -> FieldId {
        let id = FieldId(self.fields.len() as u32);
        self.fields.push(field);
        id
    }

    pub fn alloc_class_decl(&mut self, class: ClassDecl) -> ClassDeclId {
        let id = ClassDeclId(self.class_decls.len() as u32);
        self.class_decls.push(class);
        id
    }

    pub fn alloc_class_member(&mut self, member: ClassMember) -> ClassMemberId {
        let id = ClassMemberId(self.class_members.len() as u32);
        self.class_members.push(member);
        id
    }

    pub fn stmt(&self, id: StmtId) -> &Stmt {
        &self.stmts[id.0 as usize]
    }

    pub fn expr(&self, id: ExprId) -> &Expr {
        &self.exprs[id.0 as usize].expr
    }

    pub fn expr_mut(&mut self, id: ExprId) -> &mut Expr {
        &mut self.exprs[id.0 as usize].expr
    }

    pub fn expr_typed(&self, id: ExprId) -> &TypedExpr {
        &self.exprs[id.0 as usize]
    }

    pub fn expr_typed_mut(&mut self, id: ExprId) -> &mut TypedExpr {
        &mut self.exprs[id.0 as usize]
    }

    pub fn ty(&self, id: AstTypeId) -> &AstType {
        &self.types[id.0 as usize]
    }

    pub fn ty_mut(&mut self, id: AstTypeId) -> &mut AstType {
        &mut self.types[id.0 as usize]
    }

    pub fn param(&self, id: ParamId) -> &Param {
        &self.params[id.0 as usize]
    }

    pub fn method(&self, id: MethodId) -> &Method {
        &self.methods[id.0 as usize]
    }

    pub fn field(&self, id: FieldId) -> &Field {
        &self.fields[id.0 as usize]
    }

    pub fn class_decl(&self, id: ClassDeclId) -> &ClassDecl {
        &self.class_decls[id.0 as usize]
    }

    pub fn class_member(&self, id: ClassMemberId) -> &ClassMember {
        &self.class_members[id.0 as usize]
    }
}
