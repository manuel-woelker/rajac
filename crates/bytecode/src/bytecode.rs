use rajac_ast::{
    AstArena, AstType, Expr as AstExpr, ExprId, Literal, LiteralKind, ParamId, PrimitiveType, Stmt,
    StmtId,
};
use rajac_base::result::RajacResult;
use rajac_symbols::SymbolTable;
use rajac_types::{Ident, Type, TypeArena, TypeId};
use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::Instruction;

pub struct BytecodeEmitter {
    code_items: Vec<CodeItem>,
}

impl BytecodeEmitter {
    pub fn new() -> Self {
        Self {
            code_items: Vec::new(),
        }
    }

    fn emit(&mut self, instruction: Instruction) {
        self.code_items.push(CodeItem::Instruction(instruction));
    }

    fn emit_branch(&mut self, kind: BranchKind, label: LabelId) {
        self.code_items.push(CodeItem::Branch {
            kind,
            target: label,
        });
    }

    fn bind_label(&mut self, label: LabelId) {
        self.code_items.push(CodeItem::Label(label));
    }

    fn is_empty(&self) -> bool {
        self.code_items
            .iter()
            .all(|item| matches!(item, CodeItem::Label(_)))
    }

    fn last_instruction(&self) -> Option<&Instruction> {
        self.code_items.iter().rev().find_map(|item| match item {
            CodeItem::Instruction(instruction) => Some(instruction),
            CodeItem::Branch { .. } | CodeItem::Label(_) => None,
        })
    }

    fn finalize(&self) -> Vec<Instruction> {
        let mut offset = 0u16;
        let mut labels = std::collections::HashMap::new();

        for item in &self.code_items {
            match item {
                CodeItem::Instruction(_) | CodeItem::Branch { .. } => {
                    offset = offset.saturating_add(1);
                }
                CodeItem::Label(label) => {
                    labels.insert(*label, offset);
                }
            }
        }

        let mut instructions = Vec::new();
        let terminal_offset = offset;
        /* 📖 # Why append a terminal nop for some label targets?
        The classfile library expects branch targets to resolve to an actual instruction index.
        Control-flow lowering can bind a label after the last emitted instruction, for example when
        both `if` branches return but the shared end label is still referenced by a `goto`.
        Appending a `nop` turns that terminal label into a concrete instruction boundary without
        changing program behavior, which keeps serialization valid.
        */
        let needs_terminal_nop = self.code_items.iter().any(|item| match item {
            CodeItem::Branch { target, .. } => labels.get(target).copied() == Some(terminal_offset),
            CodeItem::Instruction(_) | CodeItem::Label(_) => false,
        });

        for item in &self.code_items {
            match item {
                CodeItem::Instruction(instruction) => instructions.push(instruction.clone()),
                CodeItem::Branch { kind, target } => {
                    let offset = labels.get(target).copied().unwrap_or_default();
                    instructions.push(branch_instruction(*kind, offset));
                }
                CodeItem::Label(_) => {}
            }
        }

        if needs_terminal_nop {
            instructions.push(Instruction::Nop);
        }

        instructions
    }
}

impl Default for BytecodeEmitter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CodeGenerator<'arena> {
    arena: &'arena AstArena,
    type_arena: &'arena TypeArena,
    symbol_table: &'arena SymbolTable,
    constant_pool: &'arena mut ConstantPool,
    emitter: BytecodeEmitter,
    max_stack: u16,
    current_stack: i32,
    max_locals: u16,
    next_local_slot: u16,
    local_vars: std::collections::HashMap<String, LocalVar>,
    next_label_id: u32,
}

impl<'arena> CodeGenerator<'arena> {
    pub fn new(
        arena: &'arena AstArena,
        type_arena: &'arena TypeArena,
        symbol_table: &'arena SymbolTable,
        constant_pool: &'arena mut ConstantPool,
    ) -> Self {
        Self {
            arena,
            type_arena,
            symbol_table,
            constant_pool,
            emitter: BytecodeEmitter::new(),
            max_stack: 0,
            current_stack: 0,
            max_locals: 1,
            next_local_slot: 1, // slot 0 is for 'this'
            local_vars: std::collections::HashMap::new(),
            next_label_id: 0,
        }
    }

    pub fn generate_method_body(
        &mut self,
        is_static: bool,
        params: &[ParamId],
        body_id: StmtId,
    ) -> RajacResult<(Vec<Instruction>, u16, u16)> {
        self.initialize_method_locals(is_static, params);

        self.emit_statement(body_id)?;

        if self.emulator().is_empty()
            || !matches!(
                self.emulator().last_instruction(),
                Some(
                    Instruction::Return
                        | Instruction::Areturn
                        | Instruction::Ireturn
                        | Instruction::Freturn
                        | Instruction::Dreturn
                        | Instruction::Lreturn
                )
            )
        {
            self.emit(Instruction::Return);
        }

        Ok((self.emitter.finalize(), self.max_stack, self.max_locals))
    }

    pub fn generate_constructor_body(
        &mut self,
        super_internal_name: &str,
    ) -> RajacResult<(Vec<Instruction>, u16, u16)> {
        self.max_locals = 1;

        self.emit(Instruction::Aload_0);
        let super_class = self.constant_pool.add_class(super_internal_name)?;
        let super_init = self
            .constant_pool
            .add_method_ref(super_class, "<init>", "()V")?;
        self.emit(Instruction::Invokespecial(super_init));

        self.emit(Instruction::Return);

        Ok((self.emitter.finalize(), self.max_stack, self.max_locals))
    }

    fn emit(&mut self, instruction: Instruction) {
        let delta = stack_effect(&instruction);
        self.current_stack = (self.current_stack + delta).max(0);
        self.max_stack = self.max_stack.max(self.current_stack as u16);
        self.emulator().emit(instruction);
    }

    fn emulator(&mut self) -> &mut BytecodeEmitter {
        &mut self.emitter
    }

    fn new_label(&mut self) -> LabelId {
        let label = LabelId(self.next_label_id);
        self.next_label_id += 1;
        label
    }

    fn bind_label(&mut self, label: LabelId) {
        self.emitter.bind_label(label);
    }

    fn emit_branch(&mut self, kind: BranchKind, label: LabelId) {
        let delta = stack_effect(&branch_instruction(kind, 0));
        self.current_stack = (self.current_stack + delta).max(0);
        self.max_stack = self.max_stack.max(self.current_stack as u16);
        self.emitter.emit_branch(kind, label);
    }

    pub fn emit_statement(&mut self, stmt_id: StmtId) -> RajacResult<()> {
        let stmt = self.arena.stmt(stmt_id);
        match stmt {
            Stmt::Empty => {}
            Stmt::Block(stmts) => {
                for &stmt_id in stmts {
                    self.emit_statement(stmt_id)?;
                }
            }
            Stmt::Expr(expr_id) => {
                self.emit_expression(*expr_id)?;
            }
            Stmt::Return(None) => {
                self.emit(Instruction::Return);
            }
            Stmt::Return(Some(expr_id)) => {
                self.emit_expression(*expr_id)?;
                let expr_ty = self.arena.expr_typed(*expr_id).ty;
                let expr_kind = self.kind_for_expr(*expr_id, expr_ty);
                self.emit(self.return_instruction_for_kind(expr_kind));
            }
            Stmt::LocalVar {
                ty,
                name,
                initializer,
            } => {
                if let Some(expr_id) = initializer {
                    self.emit_expression(*expr_id)?;
                    let ty = self.arena.ty(*ty);
                    let kind = local_kind_from_ast_type(ty);
                    let local_ty = ty.ty();
                    let slot = self.allocate_local(kind);
                    self.local_vars.insert(
                        name.as_str().to_string(),
                        LocalVar {
                            slot,
                            kind,
                            ty: local_ty,
                        },
                    );
                    self.emit_store(slot, kind);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if let Some(else_branch) = else_branch {
                    let else_label = self.new_label();
                    let then_terminates = self.statement_terminates(*then_branch);

                    self.emit_false_branch_condition(*condition, else_label)?;
                    self.emit_statement(*then_branch)?;

                    if then_terminates {
                        self.current_stack = 0;
                    } else {
                        let end_label = self.new_label();
                        self.emit_branch(BranchKind::Goto, end_label);
                        self.current_stack = 0;
                        self.bind_label(else_label);
                        self.emit_statement(*else_branch)?;
                        self.bind_label(end_label);
                        return Ok(());
                    }

                    self.bind_label(else_label);
                    self.emit_statement(*else_branch)?;
                } else {
                    let then_label = self.new_label();
                    let end_label = self.new_label();

                    self.emit_condition(*condition, then_label, end_label)?;
                    self.bind_label(then_label);
                    self.emit_statement(*then_branch)?;
                    self.bind_label(end_label);
                }
            }
            Stmt::While { .. } | Stmt::For { .. } | Stmt::DoWhile { .. } => {}
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Label(_, _) | Stmt::Switch { .. } => {}
            Stmt::Throw(_) => {}
            Stmt::Try { .. } | Stmt::Synchronized { .. } => {}
        }
        Ok(())
    }

    pub fn emit_expression(&mut self, expr_id: ExprId) -> RajacResult<()> {
        let typed_expr = self.arena.expr_typed(expr_id);
        let expr_ty = typed_expr.ty;
        let expr_kind = self.kind_for_expr(expr_id, expr_ty);
        let expr = &typed_expr.expr;
        match expr {
            AstExpr::Error => {}
            AstExpr::Ident(ident) => {
                if let Some(local) = self.local_vars.get(ident.as_str()) {
                    self.emit_load(local.slot, local.kind);
                }
            }
            AstExpr::Literal(literal) => {
                self.emit_literal(literal)?;
            }
            AstExpr::Unary { op, expr } => match op {
                rajac_ast::UnaryOp::Minus => {
                    self.emit_expression(*expr)?;
                    self.emit(self.neg_instruction_for_kind(expr_kind));
                }
                rajac_ast::UnaryOp::Bang => {
                    self.emit_boolean_expression(expr_id)?;
                }
                _ => {
                    self.emit_expression(*expr)?;
                }
            },
            AstExpr::Binary { op, lhs, rhs } => match op {
                rajac_ast::BinaryOp::And => {
                    self.emit_logical_and(*lhs, *rhs)?;
                }
                rajac_ast::BinaryOp::Or => {
                    self.emit_logical_or(*lhs, *rhs)?;
                }
                _ => {
                    self.emit_binary_op(op, *lhs, *rhs, expr_kind)?;
                }
            },
            AstExpr::Assign { .. } => {}
            AstExpr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                let then_label = self.new_label();
                let else_label = self.new_label();
                let end_label = self.new_label();

                self.emit_condition(*condition, then_label, else_label)?;
                self.bind_label(then_label);
                self.emit_expression(*then_expr)?;
                self.emit_branch(BranchKind::Goto, end_label);
                self.current_stack = 0;
                self.bind_label(else_label);
                self.emit_expression(*else_expr)?;
                self.bind_label(end_label);
            }
            AstExpr::Cast { ty, expr } => {
                self.emit_expression(*expr)?;
                self.emit_cast(*ty)?;
            }
            AstExpr::InstanceOf { expr, ty } => {
                self.emit_expression(*expr)?;
                let class_name = type_to_internal_class_name(*ty);
                let _ = class_name;
                self.emit(Instruction::Instanceof(0));
            }
            AstExpr::FieldAccess { expr, name, .. } => {
                self.emit_field_access(*expr, name)?;
            }
            AstExpr::MethodCall {
                expr,
                name,
                type_args: _,
                args,
                method_id,
                ..
            } => {
                self.emit_method_call(expr.as_ref(), name, args, *method_id, expr_ty)?;
            }
            AstExpr::New { ty, args } => {
                let class_name = type_to_internal_class_name_from_type_id(*ty);
                let _ = (class_name, args);
                self.emit(Instruction::New(0));
                self.emit(Instruction::Dup);
                self.emit(Instruction::Invokespecial(0));
            }
            AstExpr::NewArray { ty, dimensions } => {
                for dim in dimensions {
                    self.emit_expression(*dim)?;
                }
                let element_desc = type_to_descriptor(*ty);
                let _ = element_desc;
                self.emit(Instruction::Anewarray(0));
            }
            AstExpr::ArrayAccess { array, index } => {
                self.emit_expression(*array)?;
                self.emit_expression(*index)?;
                self.emit(Instruction::Aaload);
            }
            AstExpr::ArrayLength { array } => {
                self.emit_expression(*array)?;
                self.emit(Instruction::Arraylength);
            }
            AstExpr::This(_) => {
                self.emit(Instruction::Aload_0);
            }
            AstExpr::Super => {
                self.emit(Instruction::Aload_0);
            }
            AstExpr::SuperCall { name, args, .. } => {
                self.emit(Instruction::Aload_0);
                for &arg in args {
                    self.emit_expression(arg)?;
                }
                let _ = name;
                self.emit(Instruction::Invokespecial(0));
            }
        }
        Ok(())
    }

    fn emit_literal(&mut self, literal: &Literal) -> RajacResult<()> {
        match literal.kind {
            LiteralKind::Int => {
                if let Some(value) = parse_int_literal(literal.value.as_str()) {
                    match value {
                        -1 => self.emit(Instruction::Iconst_m1),
                        0 => self.emit(Instruction::Iconst_0),
                        1 => self.emit(Instruction::Iconst_1),
                        2 => self.emit(Instruction::Iconst_2),
                        3 => self.emit(Instruction::Iconst_3),
                        4 => self.emit(Instruction::Iconst_4),
                        5 => self.emit(Instruction::Iconst_5),
                        -128..=127 => self.emit(Instruction::Bipush(value as i8)),
                        -32768..=32767 => self.emit(Instruction::Sipush(value as i16)),
                        _ => {
                            let constant_index = self.constant_pool.add_integer(value)?;
                            self.emit_loadable_constant(constant_index);
                        }
                    }
                }
            }
            LiteralKind::Long => {
                if let Some(value) = parse_long_literal(literal.value.as_str()) {
                    match value {
                        0 => self.emit(Instruction::Lconst_0),
                        1 => self.emit(Instruction::Lconst_1),
                        _ => {
                            let constant_index = self.constant_pool.add_long(value)?;
                            self.emit(Instruction::Ldc2_w(constant_index));
                        }
                    }
                }
            }
            LiteralKind::Float => {
                if let Some(value) = parse_float_literal(literal.value.as_str()) {
                    match value {
                        0.0 => self.emit(Instruction::Fconst_0),
                        1.0 => self.emit(Instruction::Fconst_1),
                        2.0 => self.emit(Instruction::Fconst_2),
                        _ => {
                            let constant_index = self.constant_pool.add_float(value)?;
                            self.emit_loadable_constant(constant_index);
                        }
                    }
                }
            }
            LiteralKind::Double => {
                if let Some(value) = parse_double_literal(literal.value.as_str()) {
                    match value {
                        0.0 => self.emit(Instruction::Dconst_0),
                        1.0 => self.emit(Instruction::Dconst_1),
                        _ => {
                            let constant_index = self.constant_pool.add_double(value)?;
                            self.emit(Instruction::Ldc2_w(constant_index));
                        }
                    }
                }
            }
            LiteralKind::Char => {
                if let Some(value) = parse_char_literal(literal.value.as_str()) {
                    let code = value as i32;
                    match code {
                        0 => self.emit(Instruction::Iconst_0),
                        1 => self.emit(Instruction::Iconst_1),
                        2 => self.emit(Instruction::Iconst_2),
                        3 => self.emit(Instruction::Iconst_3),
                        4 => self.emit(Instruction::Iconst_4),
                        5 => self.emit(Instruction::Iconst_5),
                        -128..=127 => self.emit(Instruction::Bipush(code as i8)),
                        -32768..=32767 => self.emit(Instruction::Sipush(code as i16)),
                        _ => {
                            let constant_index = self.constant_pool.add_integer(code)?;
                            self.emit_loadable_constant(constant_index);
                        }
                    }
                }
            }
            LiteralKind::String => {
                let string_index = self.constant_pool.add_string(literal.value.as_str())?;
                if string_index <= u8::MAX as u16 {
                    self.emit(Instruction::Ldc(string_index as u8));
                } else {
                    self.emit(Instruction::Ldc_w(string_index));
                }
            }
            LiteralKind::Bool => {
                if literal.value.as_str() == "true" {
                    self.emit(Instruction::Iconst_1);
                } else {
                    self.emit(Instruction::Iconst_0);
                }
            }
            LiteralKind::Null => {
                self.emit(Instruction::Aconst_null);
            }
        }
        Ok(())
    }

    fn emit_loadable_constant(&mut self, constant_index: u16) {
        if constant_index <= u8::MAX as u16 {
            self.emit(Instruction::Ldc(constant_index as u8));
        } else {
            self.emit(Instruction::Ldc_w(constant_index));
        }
    }

    fn emit_boolean_expression(&mut self, expr_id: ExprId) -> RajacResult<()> {
        let true_label = self.new_label();
        let false_label = self.new_label();
        let end_label = self.new_label();

        self.emit_condition(expr_id, true_label, false_label)?;
        self.bind_label(true_label);
        self.emit(Instruction::Iconst_1);
        self.emit_branch(BranchKind::Goto, end_label);
        self.current_stack = 0;
        self.bind_label(false_label);
        self.emit(Instruction::Iconst_0);
        self.bind_label(end_label);

        Ok(())
    }

    fn emit_condition(
        &mut self,
        expr_id: ExprId,
        true_label: LabelId,
        false_label: LabelId,
    ) -> RajacResult<()> {
        let typed_expr = self.arena.expr_typed(expr_id);
        let expr = &typed_expr.expr;

        match expr {
            AstExpr::Literal(literal) if matches!(literal.kind, LiteralKind::Bool) => {
                if literal.value.as_str() == "true" {
                    self.emit_branch(BranchKind::Goto, true_label);
                } else {
                    self.emit_branch(BranchKind::Goto, false_label);
                }
            }
            AstExpr::Unary {
                op: rajac_ast::UnaryOp::Bang,
                expr,
            } => {
                self.emit_condition(*expr, false_label, true_label)?;
            }
            AstExpr::Binary {
                op: rajac_ast::BinaryOp::And,
                lhs,
                rhs,
            } => {
                let rhs_label = self.new_label();
                self.emit_condition(*lhs, rhs_label, false_label)?;
                self.bind_label(rhs_label);
                self.emit_condition(*rhs, true_label, false_label)?;
            }
            AstExpr::Binary {
                op: rajac_ast::BinaryOp::Or,
                lhs,
                rhs,
            } => {
                let rhs_label = self.new_label();
                self.emit_condition(*lhs, true_label, rhs_label)?;
                self.bind_label(rhs_label);
                self.emit_condition(*rhs, true_label, false_label)?;
            }
            AstExpr::Binary { op, lhs, rhs }
                if matches!(
                    op,
                    rajac_ast::BinaryOp::EqEq
                        | rajac_ast::BinaryOp::BangEq
                        | rajac_ast::BinaryOp::Lt
                        | rajac_ast::BinaryOp::LtEq
                        | rajac_ast::BinaryOp::Gt
                        | rajac_ast::BinaryOp::GtEq
                ) =>
            {
                self.emit_comparison_condition(op.clone(), *lhs, *rhs, true_label, false_label)?;
            }
            _ => {
                self.emit_expression(expr_id)?;
                self.emit_branch(BranchKind::IfNe, true_label);
                self.emit_branch(BranchKind::Goto, false_label);
            }
        }

        Ok(())
    }

    fn emit_false_branch_condition(
        &mut self,
        expr_id: ExprId,
        false_label: LabelId,
    ) -> RajacResult<()> {
        let typed_expr = self.arena.expr_typed(expr_id);
        let expr = &typed_expr.expr;

        match expr {
            AstExpr::Literal(literal) if matches!(literal.kind, LiteralKind::Bool) => {
                if literal.value.as_str() == "false" {
                    self.emit_branch(BranchKind::Goto, false_label);
                }
            }
            AstExpr::Binary { op, lhs, rhs }
                if matches!(
                    op,
                    rajac_ast::BinaryOp::EqEq
                        | rajac_ast::BinaryOp::BangEq
                        | rajac_ast::BinaryOp::Lt
                        | rajac_ast::BinaryOp::LtEq
                        | rajac_ast::BinaryOp::Gt
                        | rajac_ast::BinaryOp::GtEq
                ) =>
            {
                self.emit_comparison_false_branch(op.clone(), *lhs, *rhs, false_label)?;
            }
            _ => {
                self.emit_expression(expr_id)?;
                self.emit_branch(BranchKind::IfEq, false_label);
            }
        }

        Ok(())
    }

    fn emit_comparison_condition(
        &mut self,
        op: rajac_ast::BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
        true_label: LabelId,
        false_label: LabelId,
    ) -> RajacResult<()> {
        let lhs_kind = self.kind_for_expr(lhs, self.arena.expr_typed(lhs).ty);
        let rhs_kind = self.kind_for_expr(rhs, self.arena.expr_typed(rhs).ty);
        let comparison_kind = promote_numeric_kind(lhs_kind, rhs_kind);
        let lhs_is_null = is_null_literal(self.arena.expr(lhs));
        let rhs_is_null = is_null_literal(self.arena.expr(rhs));

        if matches!(lhs_kind, LocalVarKind::Reference)
            || matches!(rhs_kind, LocalVarKind::Reference)
        {
            if rhs_is_null {
                self.emit_expression(lhs)?;
                self.emit_branch(
                    if matches!(op, rajac_ast::BinaryOp::EqEq) {
                        BranchKind::IfNull
                    } else {
                        BranchKind::IfNonNull
                    },
                    true_label,
                );
            } else if lhs_is_null {
                self.emit_expression(rhs)?;
                self.emit_branch(
                    if matches!(op, rajac_ast::BinaryOp::EqEq) {
                        BranchKind::IfNull
                    } else {
                        BranchKind::IfNonNull
                    },
                    true_label,
                );
            } else {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit_branch(
                    match op {
                        rajac_ast::BinaryOp::EqEq => BranchKind::IfAcmpEq,
                        rajac_ast::BinaryOp::BangEq => BranchKind::IfAcmpNe,
                        _ => unreachable!(),
                    },
                    true_label,
                );
            }

            self.emit_branch(BranchKind::Goto, false_label);
            return Ok(());
        }

        self.emit_expression(lhs)?;
        self.emit_expression(rhs)?;

        match comparison_kind {
            LocalVarKind::Long => {
                self.emit(Instruction::Lcmp);
                self.emit_branch(branch_kind_for_zero_compare(op), true_label);
            }
            LocalVarKind::Float => {
                self.emit(Instruction::Fcmpl);
                self.emit_branch(branch_kind_for_zero_compare(op), true_label);
            }
            LocalVarKind::Double => {
                self.emit(Instruction::Dcmpl);
                self.emit_branch(branch_kind_for_zero_compare(op), true_label);
            }
            LocalVarKind::IntLike => {
                self.emit_branch(branch_kind_for_int_compare(op), true_label);
            }
            LocalVarKind::Reference => unreachable!(),
        }

        self.emit_branch(BranchKind::Goto, false_label);
        Ok(())
    }

    fn emit_comparison_false_branch(
        &mut self,
        op: rajac_ast::BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
        false_label: LabelId,
    ) -> RajacResult<()> {
        let lhs_kind = self.kind_for_expr(lhs, self.arena.expr_typed(lhs).ty);
        let rhs_kind = self.kind_for_expr(rhs, self.arena.expr_typed(rhs).ty);
        let comparison_kind = promote_numeric_kind(lhs_kind, rhs_kind);
        let lhs_is_null = is_null_literal(self.arena.expr(lhs));
        let rhs_is_null = is_null_literal(self.arena.expr(rhs));

        if matches!(lhs_kind, LocalVarKind::Reference)
            || matches!(rhs_kind, LocalVarKind::Reference)
        {
            if rhs_is_null {
                self.emit_expression(lhs)?;
                self.emit_branch(inverse_null_branch_kind(op), false_label);
            } else if lhs_is_null {
                self.emit_expression(rhs)?;
                self.emit_branch(inverse_null_branch_kind(op), false_label);
            } else {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit_branch(inverse_reference_branch_kind(op), false_label);
            }
            return Ok(());
        }

        self.emit_expression(lhs)?;
        self.emit_expression(rhs)?;

        match comparison_kind {
            LocalVarKind::Long => {
                self.emit(Instruction::Lcmp);
                self.emit_branch(inverse_zero_compare_branch_kind(op), false_label);
            }
            LocalVarKind::Float => {
                self.emit(Instruction::Fcmpl);
                self.emit_branch(inverse_zero_compare_branch_kind(op), false_label);
            }
            LocalVarKind::Double => {
                self.emit(Instruction::Dcmpl);
                self.emit_branch(inverse_zero_compare_branch_kind(op), false_label);
            }
            LocalVarKind::IntLike => {
                self.emit_branch(inverse_int_compare_branch_kind(op), false_label);
            }
            LocalVarKind::Reference => unreachable!(),
        }

        Ok(())
    }

    fn statement_terminates(&self, stmt_id: StmtId) -> bool {
        match self.arena.stmt(stmt_id) {
            Stmt::Return(_) => true,
            Stmt::Block(stmts) => stmts
                .last()
                .copied()
                .is_some_and(|last_stmt| self.statement_terminates(last_stmt)),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => else_branch.as_ref().is_some_and(|else_branch| {
                self.statement_terminates(*then_branch) && self.statement_terminates(*else_branch)
            }),
            _ => false,
        }
    }

    fn emit_binary_op(
        &mut self,
        op: &rajac_ast::BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
        result_kind: LocalVarKind,
    ) -> RajacResult<()> {
        use rajac_ast::BinaryOp;

        // Handle string concatenation: if either operand is a string literal
        // and the operation is Add, compute the result at compile time
        if matches!(op, BinaryOp::Add) {
            let lhs_expr = self.arena.expr(lhs);
            let rhs_expr = self.arena.expr(rhs);

            let lhs_string = match lhs_expr {
                AstExpr::Literal(lit) if matches!(lit.kind, LiteralKind::String) => {
                    Some(lit.value.as_str().to_string())
                }
                _ => None,
            };
            let rhs_string = match rhs_expr {
                AstExpr::Literal(lit) if matches!(lit.kind, LiteralKind::String) => {
                    Some(lit.value.as_str().to_string())
                }
                _ => None,
            };
            let lhs_int = match lhs_expr {
                AstExpr::Literal(lit) if matches!(lit.kind, LiteralKind::Int) => {
                    lit.value.as_str().parse::<i32>().ok()
                }
                _ => None,
            };
            let rhs_int = match rhs_expr {
                AstExpr::Literal(lit) if matches!(lit.kind, LiteralKind::Int) => {
                    lit.value.as_str().parse::<i32>().ok()
                }
                _ => None,
            };

            // String + anything or anything + String = string concatenation
            // Check for compile-time constant string concatenation
            if let (Some(lhs_lit), Some(rhs_lit)) = (&lhs_string, &rhs_string) {
                let result = format!("{}{}", lhs_lit, rhs_lit);
                let string_index = self.constant_pool.add_string(&result)?;
                if string_index <= u8::MAX as u16 {
                    self.emit(Instruction::Ldc(string_index as u8));
                } else {
                    self.emit(Instruction::Ldc_w(string_index));
                }
                return Ok(());
            }
            if let (Some(lhs_lit), Some(rhs_int)) = (&lhs_string, rhs_int) {
                let result = format!("{}{}", lhs_lit, rhs_int);
                let string_index = self.constant_pool.add_string(&result)?;
                if string_index <= u8::MAX as u16 {
                    self.emit(Instruction::Ldc(string_index as u8));
                } else {
                    self.emit(Instruction::Ldc_w(string_index));
                }
                return Ok(());
            }
            if let (Some(rhs_lit), Some(lhs_int)) = (&rhs_string, lhs_int) {
                let result = format!("{}{}", lhs_int, rhs_lit);
                let string_index = self.constant_pool.add_string(&result)?;
                if string_index <= u8::MAX as u16 {
                    self.emit(Instruction::Ldc(string_index as u8));
                } else {
                    self.emit(Instruction::Ldc_w(string_index));
                }
                return Ok(());
            }

            // String + anything or anything + String = string concatenation (non-constant)
            // Use invokedynamic (same as OpenJDK 11+)
            if lhs_string.is_some() || rhs_string.is_some() {
                // Emit both operands
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;

                // invokedynamic for string concatenation
                // Bootstrap method: StringConcatFactory.makeConcatWithConstants
                let invokedynamic = self.constant_pool.add_invoke_dynamic(
                    0,
                    "makeConcatWithConstants",
                    "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                )?;
                self.emit(Instruction::Invokedynamic(invokedynamic));

                return Ok(());
            }
        }

        match op {
            BinaryOp::Add => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.arithmetic_instruction(result_kind, ArithmeticOp::Add));
            }
            BinaryOp::Sub => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.arithmetic_instruction(result_kind, ArithmeticOp::Sub));
            }
            BinaryOp::Mul => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.arithmetic_instruction(result_kind, ArithmeticOp::Mul));
            }
            BinaryOp::Div => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.arithmetic_instruction(result_kind, ArithmeticOp::Div));
            }
            BinaryOp::Mod => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.arithmetic_instruction(result_kind, ArithmeticOp::Rem));
            }
            BinaryOp::BitAnd => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.bitwise_instruction(result_kind, BitwiseOp::And));
            }
            BinaryOp::BitOr => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.bitwise_instruction(result_kind, BitwiseOp::Or));
            }
            BinaryOp::BitXor => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.bitwise_instruction(result_kind, BitwiseOp::Xor));
            }
            BinaryOp::LShift => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.shift_instruction(result_kind, ShiftOp::Left));
            }
            BinaryOp::RShift => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.shift_instruction(result_kind, ShiftOp::Right));
            }
            BinaryOp::ARShift => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.shift_instruction(result_kind, ShiftOp::UnsignedRight));
            }
            BinaryOp::Lt
            | BinaryOp::LtEq
            | BinaryOp::Gt
            | BinaryOp::GtEq
            | BinaryOp::EqEq
            | BinaryOp::BangEq => {
                self.emit_boolean_expression_expr(op.clone(), lhs, rhs)?;
            }
            BinaryOp::And | BinaryOp::Or => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(self.bitwise_instruction(result_kind, BitwiseOp::And));
            }
        }
        Ok(())
    }

    fn emit_boolean_expression_expr(
        &mut self,
        op: rajac_ast::BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    ) -> RajacResult<()> {
        let true_label = self.new_label();
        let false_label = self.new_label();
        let end_label = self.new_label();

        self.emit_comparison_false_branch(op, lhs, rhs, false_label)?;
        self.bind_label(true_label);
        self.emit(Instruction::Iconst_1);
        self.emit_branch(BranchKind::Goto, end_label);
        self.current_stack = 0;
        self.bind_label(false_label);
        self.emit(Instruction::Iconst_0);
        self.bind_label(end_label);

        Ok(())
    }

    fn emit_cast(&mut self, target_ty: rajac_ast::AstTypeId) -> RajacResult<()> {
        let target = self.arena.ty(target_ty);
        match target {
            AstType::Primitive {
                kind: PrimitiveType::Byte,
                ty: _,
            } => {
                self.emit(Instruction::I2b);
            }
            AstType::Primitive {
                kind: PrimitiveType::Char,
                ty: _,
            } => {
                self.emit(Instruction::I2c);
            }
            AstType::Primitive {
                kind: PrimitiveType::Short,
                ty: _,
            } => {
                self.emit(Instruction::I2s);
            }
            AstType::Primitive {
                kind: PrimitiveType::Long,
                ty: _,
            } => {
                self.emit(Instruction::I2l);
            }
            AstType::Primitive {
                kind: PrimitiveType::Float,
                ty: _,
            } => {
                self.emit(Instruction::I2f);
            }
            AstType::Primitive {
                kind: PrimitiveType::Double,
                ty: _,
            } => {
                self.emit(Instruction::I2d);
            }
            _ => {}
        }
        Ok(())
    }

    fn emit_field_access(&mut self, target: ExprId, name: &Ident) -> RajacResult<()> {
        let target_expr = self.arena.expr(target);

        let is_system_out = match target_expr {
            AstExpr::Ident(ident) => ident.as_str() == "System" && name.as_str() == "out",
            AstExpr::FieldAccess {
                expr: inner_target,
                name: field_name,
                ..
            } => {
                if field_name.as_str() == "out" {
                    let inner = self.arena.expr(*inner_target);
                    matches!(inner, AstExpr::Ident(ident) if ident.as_str() == "System")
                } else {
                    false
                }
            }
            _ => false,
        };

        if is_system_out {
            return self.emit_system_out();
        }

        Ok(())
    }

    fn emit_system_out(&mut self) -> RajacResult<()> {
        let system_class = self.constant_pool.add_class("java/lang/System")?;
        let system_out =
            self.constant_pool
                .add_field_ref(system_class, "out", "Ljava/io/PrintStream;")?;
        self.emit(Instruction::Getstatic(system_out));
        Ok(())
    }

    fn emit_method_call(
        &mut self,
        target: Option<&ExprId>,
        name: &Ident,
        args: &[ExprId],
        method_id: Option<rajac_types::MethodId>,
        return_type: TypeId,
    ) -> RajacResult<()> {
        if let Some(target_expr_id) = target {
            self.emit_expression(*target_expr_id)?;
        } else {
            self.emit(Instruction::Aload_0);
        }

        for arg in args {
            self.emit_expression(*arg)?;
        }

        let descriptor = method_id
            .map(|method_id| {
                let signature = self.symbol_table.method_arena().get(method_id);
                method_descriptor_from_signature(signature, self.type_arena)
            })
            .or_else(|| infer_method_descriptor(name, args, self.symbol_table, self.type_arena))
            .unwrap_or_else(|| {
                method_descriptor_from_parts(
                    args.iter()
                        .map(|arg| self.expression_type_id(*arg))
                        .collect(),
                    return_type,
                    self.type_arena,
                )
            });
        let owner = target
            .map(|expr_id| self.expression_type_id(*expr_id))
            .map(|type_id| type_id_to_internal_name(type_id, self.type_arena))
            .unwrap_or_else(|| "java/lang/Object".to_string());

        let owner_class = self.constant_pool.add_class(&owner)?;
        let method_ref =
            self.constant_pool
                .add_method_ref(owner_class, name.as_str(), &descriptor)?;
        self.emit(Instruction::Invokevirtual(method_ref));

        Ok(())
    }

    fn initialize_method_locals(&mut self, is_static: bool, params: &[ParamId]) {
        if is_static {
            self.max_locals = 0;
            self.next_local_slot = 0;
        } else {
            self.max_locals = 1;
            self.next_local_slot = 1;
        }

        for param_id in params {
            let param = self.arena.param(*param_id);
            let param_ty = self.arena.ty(param.ty);
            let kind = local_kind_from_ast_type(param_ty);
            let slot = self.allocate_local(kind);
            self.local_vars.insert(
                param.name.as_str().to_string(),
                LocalVar {
                    slot,
                    kind,
                    ty: param_ty.ty(),
                },
            );
        }
    }

    fn allocate_local(&mut self, kind: LocalVarKind) -> u16 {
        let slot = self.next_local_slot;
        self.next_local_slot += kind.slot_size();
        self.max_locals = self.max_locals.max(self.next_local_slot);
        slot
    }

    fn emit_load(&mut self, slot: u16, kind: LocalVarKind) {
        match kind {
            LocalVarKind::IntLike => match slot {
                0 => self.emit(Instruction::Iload_0),
                1 => self.emit(Instruction::Iload_1),
                2 => self.emit(Instruction::Iload_2),
                3 => self.emit(Instruction::Iload_3),
                _ => self.emit(Instruction::Iload(slot as u8)),
            },
            LocalVarKind::Long => match slot {
                0 => self.emit(Instruction::Lload_0),
                1 => self.emit(Instruction::Lload_1),
                2 => self.emit(Instruction::Lload_2),
                3 => self.emit(Instruction::Lload_3),
                _ => self.emit(Instruction::Lload(slot as u8)),
            },
            LocalVarKind::Float => match slot {
                0 => self.emit(Instruction::Fload_0),
                1 => self.emit(Instruction::Fload_1),
                2 => self.emit(Instruction::Fload_2),
                3 => self.emit(Instruction::Fload_3),
                _ => self.emit(Instruction::Fload(slot as u8)),
            },
            LocalVarKind::Double => match slot {
                0 => self.emit(Instruction::Dload_0),
                1 => self.emit(Instruction::Dload_1),
                2 => self.emit(Instruction::Dload_2),
                3 => self.emit(Instruction::Dload_3),
                _ => self.emit(Instruction::Dload(slot as u8)),
            },
            LocalVarKind::Reference => match slot {
                0 => self.emit(Instruction::Aload_0),
                1 => self.emit(Instruction::Aload_1),
                2 => self.emit(Instruction::Aload_2),
                3 => self.emit(Instruction::Aload_3),
                _ => self.emit(Instruction::Aload(slot as u8)),
            },
        }
    }

    fn emit_store(&mut self, slot: u16, kind: LocalVarKind) {
        match kind {
            LocalVarKind::IntLike => match slot {
                0 => self.emit(Instruction::Istore_0),
                1 => self.emit(Instruction::Istore_1),
                2 => self.emit(Instruction::Istore_2),
                3 => self.emit(Instruction::Istore_3),
                _ => self.emit(Instruction::Istore(slot as u8)),
            },
            LocalVarKind::Long => match slot {
                0 => self.emit(Instruction::Lstore_0),
                1 => self.emit(Instruction::Lstore_1),
                2 => self.emit(Instruction::Lstore_2),
                3 => self.emit(Instruction::Lstore_3),
                _ => self.emit(Instruction::Lstore(slot as u8)),
            },
            LocalVarKind::Float => match slot {
                0 => self.emit(Instruction::Fstore_0),
                1 => self.emit(Instruction::Fstore_1),
                2 => self.emit(Instruction::Fstore_2),
                3 => self.emit(Instruction::Fstore_3),
                _ => self.emit(Instruction::Fstore(slot as u8)),
            },
            LocalVarKind::Double => match slot {
                0 => self.emit(Instruction::Dstore_0),
                1 => self.emit(Instruction::Dstore_1),
                2 => self.emit(Instruction::Dstore_2),
                3 => self.emit(Instruction::Dstore_3),
                _ => self.emit(Instruction::Dstore(slot as u8)),
            },
            LocalVarKind::Reference => match slot {
                0 => self.emit(Instruction::Astore_0),
                1 => self.emit(Instruction::Astore_1),
                2 => self.emit(Instruction::Astore_2),
                3 => self.emit(Instruction::Astore_3),
                _ => self.emit(Instruction::Astore(slot as u8)),
            },
        }
    }

    fn return_instruction_for_kind(&self, kind: LocalVarKind) -> Instruction {
        match kind {
            LocalVarKind::IntLike => Instruction::Ireturn,
            LocalVarKind::Long => Instruction::Lreturn,
            LocalVarKind::Float => Instruction::Freturn,
            LocalVarKind::Double => Instruction::Dreturn,
            LocalVarKind::Reference => Instruction::Areturn,
        }
    }

    fn neg_instruction_for_kind(&self, kind: LocalVarKind) -> Instruction {
        match kind {
            LocalVarKind::Long => Instruction::Lneg,
            LocalVarKind::Float => Instruction::Fneg,
            LocalVarKind::Double => Instruction::Dneg,
            _ => Instruction::Ineg,
        }
    }

    fn arithmetic_instruction(&self, kind: LocalVarKind, op: ArithmeticOp) -> Instruction {
        match (kind, op) {
            (LocalVarKind::Long, ArithmeticOp::Add) => Instruction::Ladd,
            (LocalVarKind::Long, ArithmeticOp::Sub) => Instruction::Lsub,
            (LocalVarKind::Long, ArithmeticOp::Mul) => Instruction::Lmul,
            (LocalVarKind::Long, ArithmeticOp::Div) => Instruction::Ldiv,
            (LocalVarKind::Long, ArithmeticOp::Rem) => Instruction::Lrem,
            (LocalVarKind::Float, ArithmeticOp::Add) => Instruction::Fadd,
            (LocalVarKind::Float, ArithmeticOp::Sub) => Instruction::Fsub,
            (LocalVarKind::Float, ArithmeticOp::Mul) => Instruction::Fmul,
            (LocalVarKind::Float, ArithmeticOp::Div) => Instruction::Fdiv,
            (LocalVarKind::Float, ArithmeticOp::Rem) => Instruction::Frem,
            (LocalVarKind::Double, ArithmeticOp::Add) => Instruction::Dadd,
            (LocalVarKind::Double, ArithmeticOp::Sub) => Instruction::Dsub,
            (LocalVarKind::Double, ArithmeticOp::Mul) => Instruction::Dmul,
            (LocalVarKind::Double, ArithmeticOp::Div) => Instruction::Ddiv,
            (LocalVarKind::Double, ArithmeticOp::Rem) => Instruction::Drem,
            _ => match op {
                ArithmeticOp::Add => Instruction::Iadd,
                ArithmeticOp::Sub => Instruction::Isub,
                ArithmeticOp::Mul => Instruction::Imul,
                ArithmeticOp::Div => Instruction::Idiv,
                ArithmeticOp::Rem => Instruction::Irem,
            },
        }
    }

    fn bitwise_instruction(&self, kind: LocalVarKind, op: BitwiseOp) -> Instruction {
        match (kind, op) {
            (LocalVarKind::Long, BitwiseOp::And) => Instruction::Land,
            (LocalVarKind::Long, BitwiseOp::Or) => Instruction::Lor,
            (LocalVarKind::Long, BitwiseOp::Xor) => Instruction::Lxor,
            _ => match op {
                BitwiseOp::And => Instruction::Iand,
                BitwiseOp::Or => Instruction::Ior,
                BitwiseOp::Xor => Instruction::Ixor,
            },
        }
    }

    fn shift_instruction(&self, kind: LocalVarKind, op: ShiftOp) -> Instruction {
        match (kind, op) {
            (LocalVarKind::Long, ShiftOp::Left) => Instruction::Lshl,
            (LocalVarKind::Long, ShiftOp::Right) => Instruction::Lshr,
            (LocalVarKind::Long, ShiftOp::UnsignedRight) => Instruction::Lushr,
            _ => match op {
                ShiftOp::Left => Instruction::Ishl,
                ShiftOp::Right => Instruction::Ishr,
                ShiftOp::UnsignedRight => Instruction::Iushr,
            },
        }
    }

    fn emit_logical_and(&mut self, lhs: ExprId, rhs: ExprId) -> RajacResult<()> {
        let false_label = self.new_label();
        let end_label = self.new_label();

        self.emit_expression(lhs)?;
        self.emit_branch(BranchKind::IfEq, false_label);
        self.emit_expression(rhs)?;
        self.emit_branch(BranchKind::IfEq, false_label);
        self.emit(Instruction::Iconst_1);
        self.emit_branch(BranchKind::Goto, end_label);
        self.current_stack = 0;
        self.bind_label(false_label);
        self.emit(Instruction::Iconst_0);
        self.bind_label(end_label);

        Ok(())
    }

    fn emit_logical_or(&mut self, lhs: ExprId, rhs: ExprId) -> RajacResult<()> {
        let true_label = self.new_label();
        let false_label = self.new_label();
        let end_label = self.new_label();

        self.emit_expression(lhs)?;
        self.emit_branch(BranchKind::IfNe, true_label);
        self.emit_expression(rhs)?;
        self.emit_branch(BranchKind::IfEq, false_label);
        self.bind_label(true_label);
        self.emit(Instruction::Iconst_1);
        self.emit_branch(BranchKind::Goto, end_label);
        self.current_stack = 0;
        self.bind_label(false_label);
        self.emit(Instruction::Iconst_0);
        self.bind_label(end_label);

        Ok(())
    }

    fn kind_for_expr(&self, expr_id: ExprId, expr_ty: TypeId) -> LocalVarKind {
        if expr_ty != TypeId::INVALID {
            return local_kind_from_type_id(expr_ty, self.type_arena);
        }
        self.infer_kind_from_expr(expr_id)
    }

    fn expression_type_id(&self, expr_id: ExprId) -> TypeId {
        let expr_ty = self.arena.expr_typed(expr_id).ty;
        if expr_ty != TypeId::INVALID {
            return expr_ty;
        }

        match self.arena.expr(expr_id) {
            AstExpr::Ident(ident) => self
                .local_vars
                .get(ident.as_str())
                .map(|local| local.ty)
                .unwrap_or(TypeId::INVALID),
            AstExpr::MethodCall { name, args, .. } if is_object_equals_call(name, args) => self
                .symbol_table
                .primitive_type_id("boolean")
                .unwrap_or(TypeId::INVALID),
            _ => TypeId::INVALID,
        }
    }

    fn infer_kind_from_expr(&self, expr_id: ExprId) -> LocalVarKind {
        let expr = self.arena.expr(expr_id);
        match expr {
            AstExpr::Literal(literal) => match literal.kind {
                LiteralKind::Long => LocalVarKind::Long,
                LiteralKind::Float => LocalVarKind::Float,
                LiteralKind::Double => LocalVarKind::Double,
                LiteralKind::String | LiteralKind::Null => LocalVarKind::Reference,
                _ => LocalVarKind::IntLike,
            },
            AstExpr::Ident(ident) => self
                .local_vars
                .get(ident.as_str())
                .map(|local| local.kind)
                .unwrap_or(LocalVarKind::Reference),
            AstExpr::Cast { ty, .. } => local_kind_from_ast_type(self.arena.ty(*ty)),
            AstExpr::Unary { expr, .. } => self.infer_kind_from_expr(*expr),
            AstExpr::Binary { op, lhs, rhs } => {
                let lhs_kind = self.infer_kind_from_expr(*lhs);
                let rhs_kind = self.infer_kind_from_expr(*rhs);
                match op {
                    rajac_ast::BinaryOp::And | rajac_ast::BinaryOp::Or => LocalVarKind::IntLike,
                    rajac_ast::BinaryOp::BitAnd
                    | rajac_ast::BinaryOp::BitOr
                    | rajac_ast::BinaryOp::BitXor
                    | rajac_ast::BinaryOp::LShift
                    | rajac_ast::BinaryOp::RShift
                    | rajac_ast::BinaryOp::ARShift => {
                        if matches!(lhs_kind, LocalVarKind::Long)
                            || matches!(rhs_kind, LocalVarKind::Long)
                        {
                            LocalVarKind::Long
                        } else {
                            LocalVarKind::IntLike
                        }
                    }
                    _ => promote_numeric_kind(lhs_kind, rhs_kind),
                }
            }
            AstExpr::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                let then_kind = self.infer_kind_from_expr(*then_expr);
                let else_kind = self.infer_kind_from_expr(*else_expr);
                promote_numeric_kind(then_kind, else_kind)
            }
            AstExpr::MethodCall { name, args, .. } if is_object_equals_call(name, args) => {
                LocalVarKind::IntLike
            }
            AstExpr::MethodCall { .. }
            | AstExpr::New { .. }
            | AstExpr::NewArray { .. }
            | AstExpr::ArrayAccess { .. }
            | AstExpr::ArrayLength { .. }
            | AstExpr::This(_)
            | AstExpr::Super
            | AstExpr::FieldAccess { .. }
            | AstExpr::SuperCall { .. } => LocalVarKind::Reference,
            AstExpr::Assign { .. } | AstExpr::InstanceOf { .. } | AstExpr::Error => {
                LocalVarKind::Reference
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LocalVar {
    slot: u16,
    kind: LocalVarKind,
    ty: TypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct LabelId(u32);

#[derive(Clone, Debug)]
enum CodeItem {
    Instruction(Instruction),
    Branch { kind: BranchKind, target: LabelId },
    Label(LabelId),
}

#[derive(Clone, Copy, Debug)]
enum LocalVarKind {
    IntLike,
    Long,
    Float,
    Double,
    Reference,
}

impl LocalVarKind {
    fn slot_size(self) -> u16 {
        match self {
            LocalVarKind::Long | LocalVarKind::Double => 2,
            _ => 1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Copy, Debug)]
enum BitwiseOp {
    And,
    Or,
    Xor,
}

#[derive(Clone, Copy, Debug)]
enum ShiftOp {
    Left,
    Right,
    UnsignedRight,
}

#[derive(Clone, Copy, Debug)]
enum BranchKind {
    IfEq,
    IfNe,
    IfLt,
    IfLe,
    IfGt,
    IfGe,
    IfIcmpEq,
    IfIcmpNe,
    IfIcmpLt,
    IfIcmpLe,
    IfIcmpGt,
    IfIcmpGe,
    IfAcmpEq,
    IfAcmpNe,
    IfNull,
    IfNonNull,
    Goto,
}

fn branch_instruction(kind: BranchKind, target: u16) -> Instruction {
    match kind {
        BranchKind::IfEq => Instruction::Ifeq(target),
        BranchKind::IfNe => Instruction::Ifne(target),
        BranchKind::IfLt => Instruction::Iflt(target),
        BranchKind::IfLe => Instruction::Ifle(target),
        BranchKind::IfGt => Instruction::Ifgt(target),
        BranchKind::IfGe => Instruction::Ifge(target),
        BranchKind::IfIcmpEq => Instruction::If_icmpeq(target),
        BranchKind::IfIcmpNe => Instruction::If_icmpne(target),
        BranchKind::IfIcmpLt => Instruction::If_icmplt(target),
        BranchKind::IfIcmpLe => Instruction::If_icmple(target),
        BranchKind::IfIcmpGt => Instruction::If_icmpgt(target),
        BranchKind::IfIcmpGe => Instruction::If_icmpge(target),
        BranchKind::IfAcmpEq => Instruction::If_acmpeq(target),
        BranchKind::IfAcmpNe => Instruction::If_acmpne(target),
        BranchKind::IfNull => Instruction::Ifnull(target),
        BranchKind::IfNonNull => Instruction::Ifnonnull(target),
        BranchKind::Goto => Instruction::Goto(target),
    }
}

fn local_kind_from_ast_type(ty: &AstType) -> LocalVarKind {
    match ty {
        AstType::Primitive { kind, ty: _ } => match kind {
            PrimitiveType::Long => LocalVarKind::Long,
            PrimitiveType::Float => LocalVarKind::Float,
            PrimitiveType::Double => LocalVarKind::Double,
            PrimitiveType::Void => LocalVarKind::Reference,
            _ => LocalVarKind::IntLike,
        },
        _ => LocalVarKind::Reference,
    }
}

fn local_kind_from_type_id(type_id: TypeId, type_arena: &TypeArena) -> LocalVarKind {
    if type_id == TypeId::INVALID {
        return LocalVarKind::Reference;
    }
    match type_arena.get(type_id) {
        Type::Primitive(primitive) => match primitive {
            rajac_types::PrimitiveType::Long => LocalVarKind::Long,
            rajac_types::PrimitiveType::Float => LocalVarKind::Float,
            rajac_types::PrimitiveType::Double => LocalVarKind::Double,
            rajac_types::PrimitiveType::Void => LocalVarKind::Reference,
            _ => LocalVarKind::IntLike,
        },
        Type::Class(_) | Type::Array(_) => LocalVarKind::Reference,
        Type::TypeVariable(_) | Type::Wildcard(_) | Type::Error => LocalVarKind::Reference,
    }
}

fn promote_numeric_kind(lhs_kind: LocalVarKind, rhs_kind: LocalVarKind) -> LocalVarKind {
    if matches!(lhs_kind, LocalVarKind::Double) || matches!(rhs_kind, LocalVarKind::Double) {
        LocalVarKind::Double
    } else if matches!(lhs_kind, LocalVarKind::Float) || matches!(rhs_kind, LocalVarKind::Float) {
        LocalVarKind::Float
    } else if matches!(lhs_kind, LocalVarKind::Long) || matches!(rhs_kind, LocalVarKind::Long) {
        LocalVarKind::Long
    } else {
        LocalVarKind::IntLike
    }
}

fn branch_kind_for_zero_compare(op: rajac_ast::BinaryOp) -> BranchKind {
    match op {
        rajac_ast::BinaryOp::EqEq => BranchKind::IfEq,
        rajac_ast::BinaryOp::BangEq => BranchKind::IfNe,
        rajac_ast::BinaryOp::Lt => BranchKind::IfLt,
        rajac_ast::BinaryOp::LtEq => BranchKind::IfLe,
        rajac_ast::BinaryOp::Gt => BranchKind::IfGt,
        rajac_ast::BinaryOp::GtEq => BranchKind::IfGe,
        _ => unreachable!(),
    }
}

fn branch_kind_for_int_compare(op: rajac_ast::BinaryOp) -> BranchKind {
    match op {
        rajac_ast::BinaryOp::EqEq => BranchKind::IfIcmpEq,
        rajac_ast::BinaryOp::BangEq => BranchKind::IfIcmpNe,
        rajac_ast::BinaryOp::Lt => BranchKind::IfIcmpLt,
        rajac_ast::BinaryOp::LtEq => BranchKind::IfIcmpLe,
        rajac_ast::BinaryOp::Gt => BranchKind::IfIcmpGt,
        rajac_ast::BinaryOp::GtEq => BranchKind::IfIcmpGe,
        _ => unreachable!(),
    }
}

fn inverse_zero_compare_branch_kind(op: rajac_ast::BinaryOp) -> BranchKind {
    match op {
        rajac_ast::BinaryOp::EqEq => BranchKind::IfNe,
        rajac_ast::BinaryOp::BangEq => BranchKind::IfEq,
        rajac_ast::BinaryOp::Lt => BranchKind::IfGe,
        rajac_ast::BinaryOp::LtEq => BranchKind::IfGt,
        rajac_ast::BinaryOp::Gt => BranchKind::IfLe,
        rajac_ast::BinaryOp::GtEq => BranchKind::IfLt,
        _ => unreachable!(),
    }
}

fn inverse_int_compare_branch_kind(op: rajac_ast::BinaryOp) -> BranchKind {
    match op {
        rajac_ast::BinaryOp::EqEq => BranchKind::IfIcmpNe,
        rajac_ast::BinaryOp::BangEq => BranchKind::IfIcmpEq,
        rajac_ast::BinaryOp::Lt => BranchKind::IfIcmpGe,
        rajac_ast::BinaryOp::LtEq => BranchKind::IfIcmpGt,
        rajac_ast::BinaryOp::Gt => BranchKind::IfIcmpLe,
        rajac_ast::BinaryOp::GtEq => BranchKind::IfIcmpLt,
        _ => unreachable!(),
    }
}

fn inverse_reference_branch_kind(op: rajac_ast::BinaryOp) -> BranchKind {
    match op {
        rajac_ast::BinaryOp::EqEq => BranchKind::IfAcmpNe,
        rajac_ast::BinaryOp::BangEq => BranchKind::IfAcmpEq,
        _ => unreachable!(),
    }
}

fn inverse_null_branch_kind(op: rajac_ast::BinaryOp) -> BranchKind {
    match op {
        rajac_ast::BinaryOp::EqEq => BranchKind::IfNonNull,
        rajac_ast::BinaryOp::BangEq => BranchKind::IfNull,
        _ => unreachable!(),
    }
}

fn is_null_literal(expr: &AstExpr) -> bool {
    matches!(
        expr,
        AstExpr::Literal(Literal {
            kind: LiteralKind::Null,
            ..
        })
    )
}

fn parse_int_literal(value: &str) -> Option<i32> {
    normalized_numeric_literal(value, &['l', 'L']).parse().ok()
}

fn parse_long_literal(value: &str) -> Option<i64> {
    normalized_numeric_literal(value, &['l', 'L']).parse().ok()
}

fn parse_float_literal(value: &str) -> Option<f32> {
    normalized_numeric_literal(value, &['f', 'F']).parse().ok()
}

fn parse_double_literal(value: &str) -> Option<f64> {
    normalized_numeric_literal(value, &['d', 'D']).parse().ok()
}

fn normalized_numeric_literal(value: &str, suffixes: &[char]) -> String {
    value
        .trim_end_matches(|c| suffixes.contains(&c))
        .replace('_', "")
}

fn parse_char_literal(value: &str) -> Option<char> {
    let inner = value.strip_prefix('\'')?.strip_suffix('\'')?;

    if let Some(hex) = inner.strip_prefix("\\u") {
        let code = u32::from_str_radix(hex, 16).ok()?;
        return char::from_u32(code);
    }

    if let Some(escaped) = inner.strip_prefix('\\') {
        return match escaped {
            "n" => Some('\n'),
            "r" => Some('\r'),
            "t" => Some('\t'),
            "\\" => Some('\\'),
            "'" => Some('\''),
            "\"" => Some('"'),
            "0" => Some('\0'),
            _ => None,
        };
    }

    let mut chars = inner.chars();
    let ch = chars.next()?;
    if chars.next().is_none() {
        Some(ch)
    } else {
        None
    }
}

fn is_object_equals_call(name: &Ident, args: &[ExprId]) -> bool {
    name.as_str() == "equals" && args.len() == 1
}

fn infer_method_descriptor(
    name: &Ident,
    args: &[ExprId],
    symbol_table: &SymbolTable,
    type_arena: &TypeArena,
) -> Option<String> {
    if is_object_equals_call(name, args) {
        let object_type = type_id_to_descriptor(TypeId::INVALID, type_arena);
        let boolean_type = symbol_table.primitive_type_id("boolean")?;
        return Some(format!(
            "({}){}",
            object_type,
            type_id_to_descriptor(boolean_type, type_arena)
        ));
    }

    None
}

fn type_to_internal_class_name(type_id: rajac_ast::AstTypeId) -> String {
    let _ = type_id;
    "java/lang/Object".to_string()
}

fn type_to_internal_class_name_from_type_id(type_id: rajac_ast::AstTypeId) -> String {
    let _ = type_id;
    "java/lang/Object".to_string()
}

fn type_to_descriptor(type_id: rajac_ast::AstTypeId) -> String {
    let _ = type_id;
    "Ljava/lang/Object;".to_string()
}

fn type_id_to_descriptor(type_id: TypeId, type_arena: &TypeArena) -> String {
    if type_id == TypeId::INVALID {
        return "Ljava/lang/Object;".to_string();
    }

    match type_arena.get(type_id) {
        Type::Primitive(primitive) => primitive.descriptor().to_string(),
        Type::Class(class_type) => format!("L{};", class_type.internal_name()),
        Type::Array(array_type) => {
            format!(
                "[{}",
                type_id_to_descriptor(array_type.element_type, type_arena)
            )
        }
        Type::TypeVariable(_) | Type::Wildcard(_) | Type::Error => "Ljava/lang/Object;".to_string(),
    }
}

fn type_id_to_internal_name(type_id: TypeId, type_arena: &TypeArena) -> String {
    if type_id == TypeId::INVALID {
        return "java/lang/Object".to_string();
    }
    match type_arena.get(type_id) {
        Type::Class(class_type) => class_type.internal_name(),
        _ => "java/lang/Object".to_string(),
    }
}

fn method_descriptor_from_signature(
    signature: &rajac_types::MethodSignature,
    type_arena: &TypeArena,
) -> String {
    method_descriptor_from_parts(signature.params.clone(), signature.return_type, type_arena)
}

fn method_descriptor_from_parts(
    arg_types: Vec<TypeId>,
    return_type: TypeId,
    type_arena: &TypeArena,
) -> String {
    let args = arg_types
        .into_iter()
        .map(|type_id| type_id_to_descriptor(type_id, type_arena))
        .collect::<String>();
    let return_type = type_id_to_descriptor(return_type, type_arena);
    format!("({}){}", args, return_type)
}

fn stack_effect(instr: &Instruction) -> i32 {
    use Instruction::*;
    match instr {
        Nop => 0,
        Aconst_null => 1,
        Iconst_m1 | Iconst_0 | Iconst_1 | Iconst_2 | Iconst_3 | Iconst_4 | Iconst_5 => 1,
        Lconst_0 | Lconst_1 => 2,
        Fconst_0 | Fconst_1 | Fconst_2 => 1,
        Dconst_0 | Dconst_1 => 2,
        Bipush(_) | Sipush(_) => 1,
        Ldc(_) => 1,
        Ldc_w(_) => 1,
        Ldc2_w(_) => 2,
        Iload(_) | Fload(_) | Aload(_) => 1,
        Lload(_) | Dload(_) => 2,
        Iload_0 | Iload_1 | Iload_2 | Iload_3 | Fload_0 | Fload_1 | Fload_2 | Fload_3 | Aload_0
        | Aload_1 | Aload_2 | Aload_3 => 1,
        Lload_0 | Lload_1 | Lload_2 | Lload_3 | Dload_0 | Dload_1 | Dload_2 | Dload_3 => 2,
        Istore(_) | Fstore(_) | Astore(_) => -1,
        Lstore(_) | Dstore(_) => -2,
        Istore_0 | Istore_1 | Istore_2 | Istore_3 | Fstore_0 | Fstore_1 | Fstore_2 | Fstore_3
        | Astore_0 | Astore_1 | Astore_2 | Astore_3 => -1,
        Lstore_0 | Lstore_1 | Lstore_2 | Lstore_3 | Dstore_0 | Dstore_1 | Dstore_2 | Dstore_3 => -2,
        Pop => -1,
        Pop2 => -2,
        Dup => 1,
        Dup_x1 => 0,
        Dup_x2 => -1,
        Dup2 => 2,
        Swap => 0,
        Iadd | Isub | Imul | Idiv | Irem | Iand | Ior | Ixor => -1,
        Ladd | Lsub | Lmul | Ldiv | Lrem | Land | Lor | Lxor => -2,
        Fadd | Fsub | Fmul | Fdiv | Frem => -1,
        Dadd | Dsub | Dmul | Ddiv | Drem => -2,
        Lcmp => -3,
        Fcmpl | Fcmpg => -1,
        Dcmpl | Dcmpg => -3,
        Ineg => 0,
        Lneg => 0,
        Fneg | Dneg => 0,
        Ishl | Lshl | Ishr | Lshr | Iushr | Lushr => -1,
        Ireturn | Freturn | Areturn => -1,
        Return => 0,
        Lreturn | Dreturn => -2,
        Getstatic(_) => 1,
        Putstatic(_) => -1,
        Getfield(_) => 0,
        Putfield(_) => -2,
        Invokevirtual(_) => -1,
        Invokespecial(_) => -1,
        Invokestatic(_) => 0,
        New(_) => 1,
        Anewarray(_) => 0,
        Multianewarray(_, _) => 0,
        Arraylength => 0,
        Athrow => 0,
        Checkcast(_) => 0,
        Instanceof(_) => 0,
        Ifeq(_) | Ifne(_) | Iflt(_) | Ifge(_) | Ifgt(_) | Ifle(_) | Ifnull(_) | Ifnonnull(_) => -1,
        If_icmpeq(_) | If_icmpne(_) | If_icmplt(_) | If_icmpge(_) | If_icmpgt(_) | If_icmple(_)
        | If_acmpeq(_) | If_acmpne(_) => -2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ristretto_classfile::ConstantPool;

    #[test]
    fn finalize_adds_nop_for_branch_to_terminal_label() {
        let mut emitter = BytecodeEmitter::new();
        let end_label = LabelId(0);

        emitter.emit(Instruction::Iconst_0);
        emitter.emit_branch(BranchKind::Goto, end_label);
        emitter.bind_label(end_label);

        let instructions = emitter.finalize();

        assert_eq!(
            instructions,
            vec![
                Instruction::Iconst_0,
                Instruction::Goto(2),
                Instruction::Nop
            ]
        );
    }

    #[test]
    fn finalize_does_not_add_nop_for_unreferenced_terminal_label() {
        let mut emitter = BytecodeEmitter::new();
        let end_label = LabelId(0);

        emitter.emit(Instruction::Iconst_0);
        emitter.bind_label(end_label);

        let instructions = emitter.finalize();

        assert_eq!(instructions, vec![Instruction::Iconst_0]);
    }

    #[test]
    fn emit_literal_uses_constant_pool_for_large_numeric_values() -> RajacResult<()> {
        let arena = AstArena::new();
        let type_arena = TypeArena::new();
        let symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();
        let mut generator =
            CodeGenerator::new(&arena, &type_arena, &symbol_table, &mut constant_pool);

        generator.emit_literal(&Literal {
            kind: LiteralKind::Int,
            value: "2147483647".into(),
        })?;
        generator.emit_literal(&Literal {
            kind: LiteralKind::Long,
            value: "9223372036854775807L".into(),
        })?;
        generator.emit_literal(&Literal {
            kind: LiteralKind::Float,
            value: "3.4028235e38f".into(),
        })?;
        generator.emit_literal(&Literal {
            kind: LiteralKind::Double,
            value: "1.7976931348623157e308d".into(),
        })?;

        assert!(matches!(
            generator.emitter.code_items[0],
            CodeItem::Instruction(Instruction::Ldc(_))
        ));
        assert!(matches!(
            generator.emitter.code_items[1],
            CodeItem::Instruction(Instruction::Ldc2_w(_))
        ));
        assert!(matches!(
            generator.emitter.code_items[2],
            CodeItem::Instruction(Instruction::Ldc(_))
        ));
        assert!(matches!(
            generator.emitter.code_items[3],
            CodeItem::Instruction(Instruction::Ldc2_w(_))
        ));

        Ok(())
    }

    #[test]
    fn emit_literal_decodes_char_escape_sequences() -> RajacResult<()> {
        let arena = AstArena::new();
        let type_arena = TypeArena::new();
        let symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();
        let mut generator =
            CodeGenerator::new(&arena, &type_arena, &symbol_table, &mut constant_pool);

        generator.emit_literal(&Literal {
            kind: LiteralKind::Char,
            value: "'\\u0005'".into(),
        })?;
        generator.emit_literal(&Literal {
            kind: LiteralKind::Char,
            value: "'\\uffff'".into(),
        })?;

        assert!(matches!(
            generator.emitter.code_items[0],
            CodeItem::Instruction(Instruction::Iconst_5)
        ));
        assert!(matches!(
            generator.emitter.code_items[1],
            CodeItem::Instruction(Instruction::Ldc(_))
        ));

        Ok(())
    }
}
