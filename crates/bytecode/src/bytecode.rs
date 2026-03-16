use rajac_ast::{
    AstArena, AstType, Expr as AstExpr, ExprId, Literal, LiteralKind, ParamId, PrimitiveType, Stmt,
    StmtId, SwitchCase, SwitchLabel,
};
use rajac_base::result::RajacResult;
use rajac_base::shared_string::SharedString;
use rajac_symbols::{SymbolKind, SymbolTable};
use rajac_types::{Ident, MethodId, Type, TypeArena, TypeId};
use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::{
    ArrayType as JvmArrayType, Instruction, LookupSwitch, TableSwitch,
};

#[derive(Clone, Debug)]
pub struct UnsupportedFeature {
    /// User-facing message shared between diagnostics and runtime stubs.
    pub message: SharedString,
    /// Source marker used to anchor the generation-stage diagnostic.
    pub marker: SharedString,
}

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

    fn emit_switch(&mut self, switch: SwitchItem) {
        self.code_items.push(CodeItem::Switch(switch));
    }

    fn is_empty(&self) -> bool {
        self.code_items
            .iter()
            .all(|item| matches!(item, CodeItem::Label(_)))
    }

    fn last_instruction(&self) -> Option<&Instruction> {
        self.code_items.iter().rev().find_map(|item| match item {
            CodeItem::Instruction(instruction) => Some(instruction),
            CodeItem::Branch { .. } | CodeItem::Switch(_) | CodeItem::Label(_) => None,
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
                CodeItem::Switch(_) => {
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
            CodeItem::Switch(switch) => switch
                .targets()
                .any(|target| labels.get(&target).copied() == Some(terminal_offset)),
            CodeItem::Instruction(_) | CodeItem::Label(_) => false,
        });

        for item in &self.code_items {
            match item {
                CodeItem::Instruction(instruction) => instructions.push(instruction.clone()),
                CodeItem::Branch { kind, target } => {
                    let offset = labels.get(target).copied().unwrap_or_default();
                    instructions.push(branch_instruction(*kind, offset));
                }
                CodeItem::Switch(switch) => {
                    instructions.push(switch_instruction(switch, &labels));
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
    control_flow_stack: Vec<ControlFlowFrame>,
    next_label_id: u32,
    unsupported_features: Vec<UnsupportedFeature>,
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
            control_flow_stack: Vec::new(),
            next_label_id: 0,
            unsupported_features: Vec::new(),
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
                        | Instruction::Athrow
                        | Instruction::Ireturn
                        | Instruction::Freturn
                        | Instruction::Dreturn
                        | Instruction::Lreturn
                )
            )
        {
            self.ensure_clean_stack("implicit return at end of method body")?;
            self.emit(Instruction::Return);
        }

        Ok((self.emitter.finalize(), self.max_stack, self.max_locals))
    }

    pub fn generate_constructor_body(
        &mut self,
        params: &[ParamId],
        body_id: Option<StmtId>,
        super_internal_name: &str,
    ) -> RajacResult<(Vec<Instruction>, u16, u16)> {
        self.initialize_method_locals(false, params);

        if let Some(body_id) = body_id {
            if let Some((super_args, remaining_stmts)) = self.constructor_body_parts(body_id) {
                self.emit_super_constructor_call(super_internal_name, &super_args)?;

                for stmt_id in remaining_stmts {
                    self.emit_statement(stmt_id)?;
                }
            } else {
                self.emit_super_constructor_call(super_internal_name, &[])?;
                self.emit_statement(body_id)?;
            }
        } else {
            self.emit_super_constructor_call(super_internal_name, &[])?;
        }

        if self.emulator().is_empty()
            || !matches!(
                self.emulator().last_instruction(),
                Some(
                    Instruction::Return
                        | Instruction::Areturn
                        | Instruction::Athrow
                        | Instruction::Ireturn
                        | Instruction::Freturn
                        | Instruction::Dreturn
                        | Instruction::Lreturn
                )
            )
        {
            self.ensure_clean_stack("implicit return at end of constructor body")?;
            self.emit(Instruction::Return);
        }

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

    pub fn take_unsupported_features(&mut self) -> Vec<UnsupportedFeature> {
        std::mem::take(&mut self.unsupported_features)
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

    fn emit_switch_instruction(&mut self, switch: SwitchItem) {
        self.current_stack = (self.current_stack - 1).max(0);
        self.emitter.emit_switch(switch);
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
                let initial_stack = self.current_stack;
                self.emit_expression(*expr_id)?;
                self.discard_statement_expression_result(initial_stack)?;
            }
            Stmt::Return(None) => {
                self.ensure_clean_stack("void return")?;
                self.emit(Instruction::Return);
            }
            Stmt::Return(Some(expr_id)) => {
                self.emit_expression(*expr_id)?;
                let expr_ty = self.expression_type_id(*expr_id);
                let expr_kind = self.kind_for_expr(*expr_id, expr_ty);
                self.emit(self.return_instruction_for_kind(expr_kind));
                self.ensure_clean_stack("value return")?;
            }
            Stmt::LocalVar {
                ty,
                name,
                initializer,
            } => self.emit_local_var_declaration(*ty, name, *initializer)?,
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
                    let end_label = self.new_label();

                    self.emit_false_branch_condition(*condition, end_label)?;
                    self.emit_statement(*then_branch)?;
                    self.bind_label(end_label);
                }
            }
            Stmt::While { condition, body } => {
                self.emit_while_statement(*condition, *body, None)?;
            }
            Stmt::DoWhile { body, condition } => {
                self.emit_do_while_statement(*body, *condition, None)?;
            }
            Stmt::For {
                init,
                condition,
                update,
                body,
            } => {
                self.emit_for_statement(init.clone(), *condition, *update, *body, None)?;
            }
            Stmt::Switch { expr, cases } => {
                self.emit_switch_statement(*expr, cases)?;
            }
            Stmt::Break(label) => {
                self.emit_break_statement(label.as_ref());
            }
            Stmt::Continue(label) => {
                self.emit_continue_statement(label.as_ref());
            }
            Stmt::Label(name, body) => match self.arena.stmt(*body).clone() {
                Stmt::While { condition, body } => {
                    self.emit_while_statement(condition, body, Some(name.clone()))?;
                }
                Stmt::DoWhile { body, condition } => {
                    self.emit_do_while_statement(body, condition, Some(name.clone()))?;
                }
                Stmt::For {
                    init,
                    condition,
                    update,
                    body,
                } => {
                    self.emit_for_statement(init, condition, update, body, Some(name.clone()))?;
                }
                _ => {
                    let end_label = self.new_label();
                    self.push_control_flow(ControlFlowFrame {
                        label_name: Some(name.clone()),
                        break_label: end_label,
                        continue_label: None,
                        supports_unlabeled_break: false,
                    });
                    self.emit_statement(*body)?;
                    self.pop_control_flow();
                    self.bind_label(end_label);
                }
            },
            Stmt::Throw(expr_id) => {
                self.emit_expression(*expr_id)?;
                self.emit(Instruction::Athrow);
            }
            Stmt::Try { .. } => {
                self.emit_unsupported_feature("try statements", "try")?;
            }
            Stmt::Synchronized { .. } => {
                self.emit_unsupported_feature("synchronized statements", "synchronized")?;
            }
        }
        Ok(())
    }

    fn emit_while_statement(
        &mut self,
        condition: ExprId,
        body: StmtId,
        label_name: Option<Ident>,
    ) -> RajacResult<()> {
        let loop_head = self.new_label();
        let loop_end = self.new_label();

        self.push_control_flow(ControlFlowFrame {
            label_name,
            break_label: loop_end,
            continue_label: Some(loop_head),
            supports_unlabeled_break: true,
        });

        self.bind_label(loop_head);
        self.emit_false_branch_condition(condition, loop_end)?;
        self.emit_statement(body)?;

        if !self.statement_terminates(body) {
            self.emit_branch(BranchKind::Goto, loop_head);
            self.current_stack = 0;
        }

        self.pop_control_flow();
        self.bind_label(loop_end);
        Ok(())
    }

    fn emit_do_while_statement(
        &mut self,
        body: StmtId,
        condition: ExprId,
        label_name: Option<Ident>,
    ) -> RajacResult<()> {
        let loop_body = self.new_label();
        let loop_continue = self.new_label();
        let loop_end = self.new_label();

        self.push_control_flow(ControlFlowFrame {
            label_name,
            break_label: loop_end,
            continue_label: Some(loop_continue),
            supports_unlabeled_break: true,
        });

        self.bind_label(loop_body);
        self.emit_statement(body)?;

        if !self.statement_terminates(body) {
            self.bind_label(loop_continue);
            self.emit_true_branch_condition(condition, loop_body)?;
        }

        self.pop_control_flow();
        self.bind_label(loop_end);
        Ok(())
    }

    fn emit_for_statement(
        &mut self,
        init: Option<rajac_ast::ForInit>,
        condition: Option<ExprId>,
        update: Option<ExprId>,
        body: StmtId,
        label_name: Option<Ident>,
    ) -> RajacResult<()> {
        if let Some(init) = init.as_ref() {
            self.emit_for_init(init)?;
        }

        let loop_head = self.new_label();
        let loop_continue = self.new_label();
        let loop_end = self.new_label();

        self.push_control_flow(ControlFlowFrame {
            label_name,
            break_label: loop_end,
            continue_label: Some(loop_continue),
            supports_unlabeled_break: true,
        });

        self.bind_label(loop_head);

        if let Some(condition) = condition {
            self.emit_false_branch_condition(condition, loop_end)?;
        }

        self.emit_statement(body)?;

        if !self.statement_terminates(body) {
            self.bind_label(loop_continue);
            if let Some(update) = update {
                self.emit_expression(update)?;
            }
            self.emit_branch(BranchKind::Goto, loop_head);
            self.current_stack = 0;
        }

        self.pop_control_flow();
        self.bind_label(loop_end);
        Ok(())
    }

    fn emit_switch_statement(&mut self, expr: ExprId, cases: &[SwitchCase]) -> RajacResult<()> {
        let end_label = self.new_label();
        let mut case_labels = Vec::with_capacity(cases.len());
        let mut default_label = end_label;
        let mut switch_pairs = Vec::new();

        for case in cases {
            let case_label = self.new_label();
            case_labels.push(case_label);

            for label in &case.labels {
                match label {
                    SwitchLabel::Case(expr_id) => {
                        if let Some(value) = self.switch_case_value(*expr_id) {
                            switch_pairs.push((value, case_label));
                        }
                    }
                    SwitchLabel::Default => {
                        default_label = case_label;
                    }
                }
            }
        }

        switch_pairs.sort_by_key(|(value, _)| *value);

        self.emit_expression(expr)?;
        self.emit_switch_instruction(choose_switch_item(default_label, &switch_pairs));
        self.push_control_flow(ControlFlowFrame {
            label_name: None,
            break_label: end_label,
            continue_label: None,
            supports_unlabeled_break: true,
        });

        for (case, case_label) in cases.iter().zip(case_labels.iter().copied()) {
            self.bind_label(case_label);
            for &stmt_id in &case.body {
                self.emit_statement(stmt_id)?;
            }
        }

        self.pop_control_flow();
        self.bind_label(end_label);
        Ok(())
    }

    fn emit_break_statement(&mut self, label: Option<&Ident>) {
        if let Some(target) = self.resolve_break_target(label) {
            self.emit_branch(BranchKind::Goto, target);
            self.current_stack = 0;
        }
    }

    fn emit_continue_statement(&mut self, label: Option<&Ident>) {
        if let Some(target) = self.resolve_continue_target(label) {
            self.emit_branch(BranchKind::Goto, target);
            self.current_stack = 0;
        }
    }

    fn push_control_flow(&mut self, frame: ControlFlowFrame) {
        self.control_flow_stack.push(frame);
    }

    fn pop_control_flow(&mut self) {
        let _ = self.control_flow_stack.pop();
    }

    fn resolve_break_target(&self, label: Option<&Ident>) -> Option<LabelId> {
        match label {
            Some(label) => self
                .control_flow_stack
                .iter()
                .rev()
                .find(|frame| {
                    frame
                        .label_name
                        .as_ref()
                        .is_some_and(|name| name.name == label.name)
                })
                .map(|frame| frame.break_label),
            None => self
                .control_flow_stack
                .iter()
                .rev()
                .find(|frame| frame.supports_unlabeled_break)
                .map(|frame| frame.break_label),
        }
    }

    fn resolve_continue_target(&self, label: Option<&Ident>) -> Option<LabelId> {
        match label {
            Some(label) => self
                .control_flow_stack
                .iter()
                .rev()
                .find(|frame| {
                    frame.continue_label.is_some()
                        && frame
                            .label_name
                            .as_ref()
                            .is_some_and(|name| name.name == label.name)
                })
                .and_then(|frame| frame.continue_label),
            None => self
                .control_flow_stack
                .iter()
                .rev()
                .find_map(|frame| frame.continue_label),
        }
    }

    fn switch_case_value(&self, expr_id: ExprId) -> Option<i32> {
        match self.arena.expr(expr_id) {
            AstExpr::Literal(literal) => match literal.kind {
                LiteralKind::Int => parse_int_literal(literal.value.as_str()),
                LiteralKind::Char => {
                    parse_char_literal(literal.value.as_str()).map(|value| value as i32)
                }
                _ => None,
            },
            _ => None,
        }
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
            AstExpr::Assign { lhs, rhs, .. } => {
                self.emit_assignment(*lhs, *rhs)?;
            }
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
                self.emit_instanceof_expression(*expr, *ty)?;
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
                let class_name = type_id_to_internal_name(self.arena.ty(*ty).ty(), self.type_arena);
                let class_index = self.constant_pool.add_class(&class_name)?;
                self.emit(Instruction::New(class_index));
                self.emit(Instruction::Dup);
                for &arg in args {
                    self.emit_expression(arg)?;
                }
                let descriptor = method_descriptor_from_parts(
                    args.iter()
                        .map(|arg| self.expression_type_id(*arg))
                        .collect(),
                    self.symbol_table
                        .primitive_type_id("void")
                        .unwrap_or(TypeId::INVALID),
                    self.type_arena,
                );
                let constructor_ref =
                    self.constant_pool
                        .add_method_ref(class_index, "<init>", &descriptor)?;
                self.emit(Instruction::Invokespecial(constructor_ref));
                self.adjust_method_call_stack(&descriptor, true);
            }
            AstExpr::NewArray {
                ty,
                dimensions,
                initializer,
            } => {
                self.emit_new_array_expression(*ty, dimensions, *initializer, expr_ty)?;
            }
            AstExpr::ArrayInitializer { .. } => {
                self.emit_unsupported_feature("array initializer expressions", "{")?;
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
            AstExpr::SuperCall {
                name,
                args,
                method_id,
                ..
            } => {
                self.emit_super_method_call(name, args, *method_id, expr_ty)?;
            }
        }
        Ok(())
    }

    fn emit_unsupported_feature(&mut self, feature: &str, marker: &str) -> RajacResult<()> {
        let message = format!("unsupported bytecode generation feature: {feature}");
        self.unsupported_features.push(UnsupportedFeature {
            message: SharedString::new(&message),
            marker: SharedString::new(marker),
        });

        let exception_class = self
            .constant_pool
            .add_class("java/lang/UnsupportedOperationException")?;
        let constructor_ref = self.constant_pool.add_method_ref(
            exception_class,
            "<init>",
            "(Ljava/lang/String;)V",
        )?;
        let message_index = self.constant_pool.add_string(&message)?;

        self.emit(Instruction::New(exception_class));
        self.emit(Instruction::Dup);
        self.emit_loadable_constant(message_index);
        self.emit(Instruction::Invokespecial(constructor_ref));
        self.adjust_method_call_stack("(Ljava/lang/String;)V", true);
        self.emit(Instruction::Athrow);
        Ok(())
    }

    fn emit_new_array_expression(
        &mut self,
        ast_type_id: rajac_ast::AstTypeId,
        dimensions: &[ExprId],
        initializer: Option<ExprId>,
        expr_ty: TypeId,
    ) -> RajacResult<()> {
        if let Some(initializer) = initializer {
            self.emit_array_initializer(expr_ty, initializer)?;
            return Ok(());
        }

        if dimensions.is_empty() || expr_ty == TypeId::INVALID {
            let _ = ast_type_id;
            self.emit_unsupported_feature("array creation expressions", "new")?;
            return Ok(());
        }

        for &dimension in dimensions {
            self.emit_expression(dimension)?;
        }

        if dimensions.len() > 1 {
            let array_descriptor = type_id_to_descriptor(expr_ty, self.type_arena);
            let class_index = self.constant_pool.add_class(array_descriptor)?;
            self.emit(Instruction::Multianewarray(
                class_index,
                dimensions.len() as u8,
            ));
            self.adjust_multianewarray_stack(dimensions.len() as i32);
            return Ok(());
        }

        let component_type = array_component_type(expr_ty, self.type_arena);
        if let Some(array_type) = primitive_array_type(component_type, self.type_arena) {
            self.emit(Instruction::Newarray(array_type));
            return Ok(());
        }

        let class_name = array_component_class_name(component_type, self.type_arena);
        let class_index = self.constant_pool.add_class(class_name)?;
        self.emit(Instruction::Anewarray(class_index));
        Ok(())
    }

    fn emit_instanceof_expression(
        &mut self,
        expr: ExprId,
        target_type: rajac_ast::AstTypeId,
    ) -> RajacResult<()> {
        let target_type_id = self.arena.ty(target_type).ty();
        if target_type_id == TypeId::INVALID {
            return self.emit_unsupported_feature("instanceof expressions", "instanceof");
        }

        self.emit_expression(expr)?;

        let class_name = instanceof_target_class_name(target_type_id, self.type_arena);
        let class_index = self.constant_pool.add_class(class_name)?;
        self.emit(Instruction::Instanceof(class_index));
        Ok(())
    }

    fn emit_array_initializer(&mut self, array_ty: TypeId, initializer: ExprId) -> RajacResult<()> {
        let AstExpr::ArrayInitializer { elements } = self.arena.expr(initializer).clone() else {
            self.emit_unsupported_feature("array initializer expressions", "{")?;
            return Ok(());
        };

        self.emit_int_constant(elements.len() as i32)?;
        self.emit_array_allocation(array_ty, 1)?;

        let element_ty = array_component_type(array_ty, self.type_arena);
        for (index, element_expr) in elements.into_iter().enumerate() {
            self.emit(Instruction::Dup);
            self.emit_int_constant(index as i32)?;
            self.emit_array_initializer_element(element_ty, element_expr)?;
            self.emit(array_store_instruction(element_ty, self.type_arena));
        }

        Ok(())
    }

    fn emit_array_initializer_element(
        &mut self,
        element_ty: TypeId,
        element_expr: ExprId,
    ) -> RajacResult<()> {
        match self.arena.expr(element_expr) {
            AstExpr::ArrayInitializer { .. } => {
                self.emit_array_initializer(element_ty, element_expr)
            }
            _ => self.emit_expression(element_expr),
        }
    }

    fn emit_array_allocation(&mut self, array_ty: TypeId, dimensions: usize) -> RajacResult<()> {
        if dimensions > 1 {
            let array_descriptor = type_id_to_descriptor(array_ty, self.type_arena);
            let class_index = self.constant_pool.add_class(array_descriptor)?;
            self.emit(Instruction::Multianewarray(class_index, dimensions as u8));
            self.adjust_multianewarray_stack(dimensions as i32);
            return Ok(());
        }

        let component_type = array_component_type(array_ty, self.type_arena);
        if let Some(array_type) = primitive_array_type(component_type, self.type_arena) {
            self.emit(Instruction::Newarray(array_type));
            return Ok(());
        }

        let class_name = array_component_class_name(component_type, self.type_arena);
        let class_index = self.constant_pool.add_class(class_name)?;
        self.emit(Instruction::Anewarray(class_index));
        Ok(())
    }

    fn emit_int_constant(&mut self, value: i32) -> RajacResult<()> {
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

    fn emit_super_constructor_call(
        &mut self,
        super_internal_name: &str,
        args: &[ExprId],
    ) -> RajacResult<()> {
        self.emit(Instruction::Aload_0);
        let super_class = self.constant_pool.add_class(super_internal_name)?;
        let descriptor = method_descriptor_from_parts(
            args.iter()
                .map(|arg| self.expression_type_id(*arg))
                .collect(),
            self.symbol_table
                .primitive_type_id("void")
                .unwrap_or(TypeId::INVALID),
            self.symbol_table.type_arena(),
        );
        let invocation = ResolvedInvocation::special(
            SharedString::new(super_internal_name),
            SharedString::new("<init>"),
            SharedString::new(descriptor),
        );
        self.emit_invocation(&invocation, super_class, args)?;
        Ok(())
    }

    fn emit_super_method_call(
        &mut self,
        name: &Ident,
        args: &[ExprId],
        method_id: Option<MethodId>,
        return_type: TypeId,
    ) -> RajacResult<()> {
        let Some(method_id) = method_id else {
            return self.emit_unsupported_feature("unresolved super method calls", "super");
        };
        let Some(mut invocation) =
            self.resolve_method_invocation(None, name, args, Some(method_id), return_type)?
        else {
            return self
                .emit_unsupported_feature("super method calls without resolved owners", "super");
        };
        invocation.kind = InvocationKind::Special;
        let owner_class = self
            .constant_pool
            .add_class(invocation.owner_internal_name.as_str())?;
        self.emit(Instruction::Aload_0);
        self.emit_invocation(&invocation, owner_class, args)?;
        Ok(())
    }

    fn discard_statement_expression_result(&mut self, initial_stack: i32) -> RajacResult<()> {
        let leftover = self.current_stack - initial_stack;
        match leftover {
            0 => Ok(()),
            1 => {
                self.emit(Instruction::Pop);
                Ok(())
            }
            2 => {
                self.emit(Instruction::Pop2);
                Ok(())
            }
            _ => Err(rajac_base::err!(
                "internal bytecode error: statement expression left operand stack delta {}",
                leftover
            )),
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

    fn emit_local_var_declaration(
        &mut self,
        ty: rajac_ast::AstTypeId,
        name: &Ident,
        initializer: Option<ExprId>,
    ) -> RajacResult<()> {
        let ty = self.arena.ty(ty);
        let kind = local_kind_from_ast_type(ty);
        let slot = self.allocate_local(kind);

        self.local_vars
            .insert(name.as_str().to_string(), LocalVar { slot, kind });

        if let Some(expr_id) = initializer {
            self.emit_expression(expr_id)?;
            self.emit_store(slot, kind);
        }

        Ok(())
    }

    fn emit_for_init(&mut self, init: &rajac_ast::ForInit) -> RajacResult<()> {
        match init {
            rajac_ast::ForInit::Expr(expr_id) => self.emit_expression(*expr_id),
            rajac_ast::ForInit::LocalVar {
                ty,
                name,
                initializer,
            } => self.emit_local_var_declaration(*ty, name, *initializer),
        }
    }

    fn emit_assignment(&mut self, lhs: ExprId, rhs: ExprId) -> RajacResult<()> {
        let AstExpr::Ident(ident) = self.arena.expr(lhs) else {
            return Ok(());
        };

        let Some(local) = self.local_vars.get(ident.as_str()).copied() else {
            return Ok(());
        };

        self.emit_expression(rhs)?;
        self.emit_store(local.slot, local.kind);
        Ok(())
    }

    fn ensure_clean_stack(&self, context: &str) -> RajacResult<()> {
        if self.current_stack == 0 {
            return Ok(());
        }

        Err(rajac_base::err!(
            "internal bytecode error: operand stack depth {} at {}",
            self.current_stack,
            context
        ))
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

    fn emit_true_branch_condition(
        &mut self,
        expr_id: ExprId,
        true_label: LabelId,
    ) -> RajacResult<()> {
        let typed_expr = self.arena.expr_typed(expr_id);
        let expr = &typed_expr.expr;

        match expr {
            AstExpr::Literal(literal) if matches!(literal.kind, LiteralKind::Bool) => {
                if literal.value.as_str() == "true" {
                    self.emit_branch(BranchKind::Goto, true_label);
                }
            }
            AstExpr::Unary {
                op: rajac_ast::UnaryOp::Bang,
                expr,
            } => {
                let false_label = self.new_label();
                self.emit_condition(*expr, false_label, true_label)?;
                self.bind_label(false_label);
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
                self.emit_expression(*lhs)?;
                self.emit_expression(*rhs)?;

                let lhs_kind = self.kind_for_expr(*lhs, self.arena.expr_typed(*lhs).ty);
                let rhs_kind = self.kind_for_expr(*rhs, self.arena.expr_typed(*rhs).ty);
                let comparison_kind = promote_numeric_kind(lhs_kind, rhs_kind);

                match comparison_kind {
                    LocalVarKind::Long => {
                        self.emit(Instruction::Lcmp);
                        self.emit_branch(branch_kind_for_zero_compare(op.clone()), true_label);
                    }
                    LocalVarKind::Float => {
                        self.emit(Instruction::Fcmpl);
                        self.emit_branch(branch_kind_for_zero_compare(op.clone()), true_label);
                    }
                    LocalVarKind::Double => {
                        self.emit(Instruction::Dcmpl);
                        self.emit_branch(branch_kind_for_zero_compare(op.clone()), true_label);
                    }
                    LocalVarKind::IntLike => {
                        self.emit_branch(branch_kind_for_int_compare(op.clone()), true_label);
                    }
                    LocalVarKind::Reference => {
                        self.emit_branch(reference_branch_kind(op.clone()), true_label);
                    }
                }
            }
            _ => {
                self.emit_expression(expr_id)?;
                self.emit_branch(BranchKind::IfNe, true_label);
            }
        }

        self.current_stack = 0;
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
            Stmt::Return(_) | Stmt::Throw(_) => true,
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
        method_id: Option<MethodId>,
        return_type: TypeId,
    ) -> RajacResult<()> {
        let Some(invocation) =
            self.resolve_method_invocation(target, name, args, method_id, return_type)?
        else {
            return self.emit_unsupported_feature("unresolved method calls", name.as_str());
        };

        if let Some(target_expr_id) = target {
            self.emit_expression(*target_expr_id)?;
        } else if invocation.has_receiver() {
            self.emit(Instruction::Aload_0);
        }

        let owner_class = self
            .constant_pool
            .add_class(invocation.owner_internal_name.as_str())?;
        self.emit_invocation(&invocation, owner_class, args)?;
        Ok(())
    }

    fn resolve_method_invocation(
        &self,
        target: Option<&ExprId>,
        name: &Ident,
        args: &[ExprId],
        method_id: Option<MethodId>,
        return_type: TypeId,
    ) -> RajacResult<Option<ResolvedInvocation>> {
        let Some(method_id) = method_id else {
            if let Some(descriptor) =
                infer_method_descriptor(name, args, self.symbol_table, self.type_arena)
            {
                return Ok(Some(ResolvedInvocation {
                    kind: InvocationKind::Virtual,
                    owner_internal_name: SharedString::new("java/lang/Object"),
                    name: SharedString::new(name.as_str()),
                    descriptor: SharedString::new(descriptor),
                    interface_arg_count: None,
                }));
            }
            return Ok(Some(ResolvedInvocation {
                kind: InvocationKind::Virtual,
                owner_internal_name: SharedString::new("java/lang/Object"),
                name: SharedString::new(name.as_str()),
                descriptor: SharedString::new(method_descriptor_from_parts(
                    args.iter()
                        .map(|arg| self.expression_type_id(*arg))
                        .collect(),
                    return_type,
                    self.type_arena,
                )),
                interface_arg_count: None,
            }));
        };

        let signature = self.symbol_table.method_arena().get(method_id);
        let descriptor =
            SharedString::new(method_descriptor_from_signature(signature, self.type_arena));
        let Some(owner) = self.resolve_method_owner(method_id) else {
            return Ok(None);
        };
        let is_super_receiver =
            target.is_some_and(|expr_id| matches!(self.arena.expr(*expr_id), AstExpr::Super));
        let kind = if signature.modifiers.is_static() {
            InvocationKind::Static
        } else if is_super_receiver {
            InvocationKind::Special
        } else if matches!(owner.kind, SymbolKind::Interface) {
            InvocationKind::Interface
        } else {
            InvocationKind::Virtual
        };

        let interface_arg_count = matches!(kind, InvocationKind::Interface)
            .then(|| interface_arg_count(signature, self.type_arena));

        Ok(Some(ResolvedInvocation {
            kind,
            owner_internal_name: owner.internal_name,
            name: SharedString::new(signature.name.as_str()),
            descriptor,
            interface_arg_count,
        }))
    }

    fn resolve_method_owner(&self, method_id: MethodId) -> Option<ResolvedMethodOwner> {
        let type_arena = self.symbol_table.type_arena();
        for index in 0..type_arena.len() {
            let type_id = TypeId(index as u32);
            let Type::Class(class_type) = type_arena.get(type_id) else {
                continue;
            };
            if !class_type
                .methods
                .values()
                .any(|method_ids| method_ids.contains(&method_id))
            {
                continue;
            }

            let internal_name = SharedString::new(class_type.internal_name());
            let kind = self.lookup_symbol_kind_for_type(type_id)?;
            return Some(ResolvedMethodOwner {
                internal_name,
                kind,
            });
        }
        None
    }

    fn lookup_symbol_kind_for_type(&self, type_id: TypeId) -> Option<SymbolKind> {
        for (_package_name, package) in self.symbol_table.iter() {
            for (_name, symbol) in package.iter() {
                if symbol.ty == type_id {
                    return Some(symbol.kind);
                }
            }
        }
        None
    }

    fn emit_invocation(
        &mut self,
        invocation: &ResolvedInvocation,
        owner_class: u16,
        args: &[ExprId],
    ) -> RajacResult<()> {
        for &arg in args {
            self.emit_expression(arg)?;
        }

        let method_ref = match invocation.kind {
            InvocationKind::Interface => self.constant_pool.add_interface_method_ref(
                owner_class,
                invocation.name.as_str(),
                invocation.descriptor.as_str(),
            )?,
            InvocationKind::Static | InvocationKind::Virtual | InvocationKind::Special => {
                self.constant_pool.add_method_ref(
                    owner_class,
                    invocation.name.as_str(),
                    invocation.descriptor.as_str(),
                )?
            }
        };

        match invocation.kind {
            InvocationKind::Static => {
                self.emit(Instruction::Invokestatic(method_ref));
            }
            InvocationKind::Virtual => {
                self.emit(Instruction::Invokevirtual(method_ref));
            }
            InvocationKind::Special => {
                self.emit(Instruction::Invokespecial(method_ref));
            }
            InvocationKind::Interface => {
                self.emit(Instruction::Invokeinterface(
                    method_ref,
                    invocation.interface_arg_count.unwrap_or(1),
                ));
            }
        }
        self.adjust_method_call_stack(invocation.descriptor.as_str(), invocation.has_receiver());
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
            self.local_vars
                .insert(param.name.as_str().to_string(), LocalVar { slot, kind });
        }
    }

    fn constructor_body_parts(&self, body_id: StmtId) -> Option<(Vec<ExprId>, Vec<StmtId>)> {
        let Stmt::Block(statements) = self.arena.stmt(body_id) else {
            return None;
        };
        let first_stmt = statements.first()?;
        let Stmt::Expr(expr_id) = self.arena.stmt(*first_stmt) else {
            return None;
        };
        let AstExpr::SuperCall { args, .. } = self.arena.expr(*expr_id) else {
            return None;
        };

        Some((args.clone(), statements.iter().skip(1).copied().collect()))
    }

    fn adjust_method_call_stack(&mut self, descriptor: &str, has_receiver: bool) {
        let actual_delta =
            method_call_stack_delta(descriptor, has_receiver).unwrap_or(-i32::from(has_receiver));
        let generic_delta = -i32::from(has_receiver);
        self.current_stack += actual_delta - generic_delta;
        self.max_stack = self.max_stack.max(self.current_stack.max(0) as u16);
    }

    fn adjust_multianewarray_stack(&mut self, dimensions: i32) {
        let actual_delta = 1 - dimensions;
        self.current_stack = (self.current_stack + actual_delta).max(0);
        self.max_stack = self.max_stack.max(self.current_stack as u16);
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
            | AstExpr::ArrayInitializer { .. }
            | AstExpr::ArrayAccess { .. }
            | AstExpr::ArrayLength { .. }
            | AstExpr::This(_)
            | AstExpr::Super
            | AstExpr::FieldAccess { .. }
            | AstExpr::SuperCall { .. } => LocalVarKind::Reference,
            AstExpr::InstanceOf { .. } => LocalVarKind::IntLike,
            AstExpr::Assign { .. } | AstExpr::Error => LocalVarKind::Reference,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LocalVar {
    slot: u16,
    kind: LocalVarKind,
}

#[derive(Clone, Debug)]
struct ResolvedMethodOwner {
    internal_name: SharedString,
    kind: SymbolKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InvocationKind {
    Static,
    Virtual,
    Special,
    Interface,
}

#[derive(Clone, Debug)]
struct ResolvedInvocation {
    kind: InvocationKind,
    owner_internal_name: SharedString,
    name: SharedString,
    descriptor: SharedString,
    interface_arg_count: Option<u8>,
}

impl ResolvedInvocation {
    fn special(
        owner_internal_name: SharedString,
        name: SharedString,
        descriptor: SharedString,
    ) -> Self {
        Self {
            kind: InvocationKind::Special,
            owner_internal_name,
            name,
            descriptor,
            interface_arg_count: None,
        }
    }

    fn has_receiver(&self) -> bool {
        !matches!(self.kind, InvocationKind::Static)
    }
}

#[derive(Clone, Debug)]
struct ControlFlowFrame {
    label_name: Option<Ident>,
    break_label: LabelId,
    continue_label: Option<LabelId>,
    supports_unlabeled_break: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct LabelId(u32);

#[derive(Clone, Debug)]
enum CodeItem {
    Instruction(Instruction),
    Branch { kind: BranchKind, target: LabelId },
    Switch(SwitchItem),
    Label(LabelId),
}

#[derive(Clone, Debug)]
enum SwitchItem {
    Table {
        default: LabelId,
        low: i32,
        high: i32,
        offsets: Vec<LabelId>,
    },
    Lookup {
        default: LabelId,
        pairs: Vec<(i32, LabelId)>,
    },
}

impl SwitchItem {
    fn targets(&self) -> impl Iterator<Item = LabelId> + '_ {
        let mut targets = Vec::new();
        match self {
            SwitchItem::Table {
                default, offsets, ..
            } => {
                targets.push(*default);
                targets.extend(offsets.iter().copied());
            }
            SwitchItem::Lookup { default, pairs } => {
                targets.push(*default);
                targets.extend(pairs.iter().map(|(_, label)| *label));
            }
        }
        targets.into_iter()
    }
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

fn choose_switch_item(default: LabelId, pairs: &[(i32, LabelId)]) -> SwitchItem {
    if pairs.is_empty() {
        return SwitchItem::Lookup {
            default,
            pairs: Vec::new(),
        };
    }

    let low = pairs.first().map(|(value, _)| *value).unwrap_or_default();
    let high = pairs.last().map(|(value, _)| *value).unwrap_or_default();
    let table_space_cost = 4 + (high - low + 1);
    let table_time_cost = 3;
    let lookup_space_cost = 3 + 2 * pairs.len() as i32;
    let lookup_time_cost = pairs.len() as i32;

    if table_space_cost + 3 * table_time_cost <= lookup_space_cost + 3 * lookup_time_cost {
        let mut offsets = vec![default; (high - low + 1) as usize];
        for (value, label) in pairs {
            offsets[(value - low) as usize] = *label;
        }
        SwitchItem::Table {
            default,
            low,
            high,
            offsets,
        }
    } else {
        SwitchItem::Lookup {
            default,
            pairs: pairs.to_vec(),
        }
    }
}

fn switch_instruction(
    switch: &SwitchItem,
    labels: &std::collections::HashMap<LabelId, u16>,
) -> Instruction {
    match switch {
        SwitchItem::Table {
            default,
            low,
            high,
            offsets,
        } => Instruction::Tableswitch(TableSwitch {
            default: labels.get(default).copied().unwrap_or_default() as i32,
            low: *low,
            high: *high,
            offsets: offsets
                .iter()
                .map(|label| labels.get(label).copied().unwrap_or_default() as i32)
                .collect(),
        }),
        SwitchItem::Lookup { default, pairs } => Instruction::Lookupswitch(LookupSwitch {
            default: labels.get(default).copied().unwrap_or_default() as i32,
            pairs: pairs
                .iter()
                .map(|(value, label)| {
                    (
                        *value,
                        labels.get(label).copied().unwrap_or_default() as i32,
                    )
                })
                .collect(),
        }),
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

fn reference_branch_kind(op: rajac_ast::BinaryOp) -> BranchKind {
    match op {
        rajac_ast::BinaryOp::EqEq => BranchKind::IfAcmpEq,
        rajac_ast::BinaryOp::BangEq => BranchKind::IfAcmpNe,
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

fn instanceof_target_class_name(type_id: TypeId, type_arena: &TypeArena) -> String {
    match type_arena.get(type_id) {
        Type::Class(class_type) => class_type.internal_name(),
        Type::Array(_) => type_id_to_descriptor(type_id, type_arena),
        Type::Primitive(_) | Type::TypeVariable(_) | Type::Wildcard(_) | Type::Error => {
            "java/lang/Object".to_string()
        }
    }
}

fn array_component_type(array_type_id: TypeId, type_arena: &TypeArena) -> TypeId {
    match type_arena.get(array_type_id) {
        Type::Array(array_type) => array_type.element_type,
        _ => TypeId::INVALID,
    }
}

fn primitive_array_type(type_id: TypeId, type_arena: &TypeArena) -> Option<JvmArrayType> {
    match type_arena.get(type_id) {
        Type::Primitive(rajac_types::PrimitiveType::Boolean) => Some(JvmArrayType::Boolean),
        Type::Primitive(rajac_types::PrimitiveType::Byte) => Some(JvmArrayType::Byte),
        Type::Primitive(rajac_types::PrimitiveType::Char) => Some(JvmArrayType::Char),
        Type::Primitive(rajac_types::PrimitiveType::Short) => Some(JvmArrayType::Short),
        Type::Primitive(rajac_types::PrimitiveType::Int) => Some(JvmArrayType::Int),
        Type::Primitive(rajac_types::PrimitiveType::Long) => Some(JvmArrayType::Long),
        Type::Primitive(rajac_types::PrimitiveType::Float) => Some(JvmArrayType::Float),
        Type::Primitive(rajac_types::PrimitiveType::Double) => Some(JvmArrayType::Double),
        Type::Primitive(rajac_types::PrimitiveType::Void)
        | Type::Class(_)
        | Type::Array(_)
        | Type::TypeVariable(_)
        | Type::Wildcard(_)
        | Type::Error => None,
    }
}

fn array_component_class_name(type_id: TypeId, type_arena: &TypeArena) -> String {
    match type_arena.get(type_id) {
        Type::Class(class_type) => class_type.internal_name(),
        Type::Array(_) => type_id_to_descriptor(type_id, type_arena),
        Type::Primitive(_) | Type::TypeVariable(_) | Type::Wildcard(_) | Type::Error => {
            "java/lang/Object".to_string()
        }
    }
}

fn array_store_instruction(type_id: TypeId, type_arena: &TypeArena) -> Instruction {
    match type_arena.get(type_id) {
        Type::Primitive(rajac_types::PrimitiveType::Boolean)
        | Type::Primitive(rajac_types::PrimitiveType::Byte) => Instruction::Bastore,
        Type::Primitive(rajac_types::PrimitiveType::Char) => Instruction::Castore,
        Type::Primitive(rajac_types::PrimitiveType::Short) => Instruction::Sastore,
        Type::Primitive(rajac_types::PrimitiveType::Int) => Instruction::Iastore,
        Type::Primitive(rajac_types::PrimitiveType::Long) => Instruction::Lastore,
        Type::Primitive(rajac_types::PrimitiveType::Float) => Instruction::Fastore,
        Type::Primitive(rajac_types::PrimitiveType::Double) => Instruction::Dastore,
        Type::Primitive(rajac_types::PrimitiveType::Void)
        | Type::Class(_)
        | Type::Array(_)
        | Type::TypeVariable(_)
        | Type::Wildcard(_)
        | Type::Error => Instruction::Aastore,
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
        Invokeinterface(_, _) => -1,
        Invokespecial(_) => -1,
        Invokestatic(_) => 0,
        New(_) => 1,
        Newarray(_) => 0,
        Anewarray(_) => 0,
        Multianewarray(_, _) => 0,
        Iastore | Fastore | Aastore | Bastore | Castore | Sastore => -3,
        Lastore | Dastore => -4,
        Arraylength => 0,
        Athrow => -1,
        Checkcast(_) => 0,
        Instanceof(_) => 0,
        Ifeq(_) | Ifne(_) | Iflt(_) | Ifge(_) | Ifgt(_) | Ifle(_) | Ifnull(_) | Ifnonnull(_) => -1,
        If_icmpeq(_) | If_icmpne(_) | If_icmplt(_) | If_icmpge(_) | If_icmpgt(_) | If_icmple(_)
        | If_acmpeq(_) | If_acmpne(_) => -2,
        Tableswitch(_) | Lookupswitch(_) => -1,
        _ => 0,
    }
}

fn method_call_stack_delta(descriptor: &str, has_receiver: bool) -> Option<i32> {
    let mut chars = descriptor.chars().peekable();
    if chars.next()? != '(' {
        return None;
    }

    let mut arg_slots = if has_receiver { 1 } else { 0 };
    loop {
        match chars.peek().copied()? {
            ')' => {
                chars.next();
                break;
            }
            _ => {
                arg_slots += parse_descriptor_type_slots(&mut chars)?;
            }
        }
    }

    let return_slots = parse_return_descriptor_slots(&mut chars)?;
    if chars.next().is_some() {
        return None;
    }

    Some(return_slots - arg_slots)
}

fn interface_arg_count(signature: &rajac_types::MethodSignature, type_arena: &TypeArena) -> u8 {
    let parameter_slots: usize = signature
        .params
        .iter()
        .map(|param_ty| local_kind_from_type_id(*param_ty, type_arena).slot_size() as usize)
        .sum();
    u8::try_from(parameter_slots + 1).unwrap_or(1)
}

fn parse_return_descriptor_slots<I>(chars: &mut std::iter::Peekable<I>) -> Option<i32>
where
    I: Iterator<Item = char>,
{
    match chars.peek().copied()? {
        'V' => {
            chars.next();
            Some(0)
        }
        _ => parse_descriptor_type_slots(chars),
    }
}

fn parse_descriptor_type_slots<I>(chars: &mut std::iter::Peekable<I>) -> Option<i32>
where
    I: Iterator<Item = char>,
{
    match chars.next()? {
        'B' | 'C' | 'F' | 'I' | 'S' | 'Z' => Some(1),
        'D' | 'J' => Some(2),
        'L' => {
            while chars.next()? != ';' {}
            Some(1)
        }
        '[' => {
            while matches!(chars.peek().copied(), Some('[')) {
                chars.next();
            }
            match chars.peek().copied()? {
                'L' => {
                    chars.next();
                    while chars.next()? != ';' {}
                }
                _ => {
                    chars.next();
                }
            }
            Some(1)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rajac_types::{ClassType, MethodModifiers, MethodSignature, Type};
    use ristretto_classfile::ConstantPool;

    fn add_owner_method(
        symbol_table: &mut SymbolTable,
        package_name: &str,
        class_name: &str,
        kind: SymbolKind,
        signature: MethodSignature,
    ) -> MethodId {
        let type_id = symbol_table.add_class(
            package_name,
            class_name,
            Type::class(
                ClassType::new(SharedString::new(class_name))
                    .with_package(SharedString::new(package_name)),
            ),
            kind,
        );
        let method_id = symbol_table.method_arena_mut().alloc(signature);
        let method_name = symbol_table.method_arena().get(method_id).name.clone();
        if let Type::Class(class_type) = symbol_table.type_arena_mut().get_mut(type_id) {
            class_type.add_method(method_name, method_id);
        }
        method_id
    }

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

    #[test]
    fn expression_statements_discard_their_result_before_return() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let type_arena = TypeArena::new();
        let symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let expr_id = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "7".into(),
        }));
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(expr_id));
        let body = arena.alloc_stmt(Stmt::Block(vec![expr_stmt]));

        let mut generator =
            CodeGenerator::new(&arena, &type_arena, &symbol_table, &mut constant_pool);
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;

        assert_eq!(
            instructions,
            vec![
                Instruction::Bipush(7),
                Instruction::Pop,
                Instruction::Return
            ]
        );

        Ok(())
    }

    #[test]
    fn assignment_expression_statements_do_not_emit_extra_pop() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let type_arena = TypeArena::new();
        let symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let int_ty = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Int,
            ty: TypeId::INVALID,
        });
        let initializer = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "0".into(),
        }));
        let local_var = arena.alloc_stmt(Stmt::LocalVar {
            ty: int_ty,
            name: Ident::new("value".into()),
            initializer: Some(initializer),
        });
        let assigned = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "1".into(),
        }));
        let lhs = arena.alloc_expr(AstExpr::Ident(Ident::new("value".into())));
        let assign_expr = arena.alloc_expr(AstExpr::Assign {
            op: rajac_ast::AssignOp::Eq,
            lhs,
            rhs: assigned,
        });
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(assign_expr));
        let body = arena.alloc_stmt(Stmt::Block(vec![local_var, expr_stmt]));

        let mut generator =
            CodeGenerator::new(&arena, &type_arena, &symbol_table, &mut constant_pool);
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;

        assert_eq!(
            instructions,
            vec![
                Instruction::Iconst_0,
                Instruction::Istore_1,
                Instruction::Iconst_1,
                Instruction::Istore_1,
                Instruction::Return
            ]
        );

        Ok(())
    }

    #[test]
    fn throw_statements_emit_athrow_without_implicit_return() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let type_arena = TypeArena::new();
        let symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let null_expr = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Null,
            value: "null".into(),
        }));
        let throw_stmt = arena.alloc_stmt(Stmt::Throw(null_expr));
        let body = arena.alloc_stmt(Stmt::Block(vec![throw_stmt]));

        let mut generator =
            CodeGenerator::new(&arena, &type_arena, &symbol_table, &mut constant_pool);
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;

        assert_eq!(
            instructions,
            vec![Instruction::Aconst_null, Instruction::Athrow]
        );

        Ok(())
    }

    #[test]
    fn constructor_throw_statements_emit_athrow_without_implicit_return() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let type_arena = TypeArena::new();
        let symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let null_expr = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Null,
            value: "null".into(),
        }));
        let throw_stmt = arena.alloc_stmt(Stmt::Throw(null_expr));
        let body = arena.alloc_stmt(Stmt::Block(vec![throw_stmt]));

        let mut generator =
            CodeGenerator::new(&arena, &type_arena, &symbol_table, &mut constant_pool);
        let (instructions, _max_stack, _max_locals) =
            generator.generate_constructor_body(&[], Some(body), "java/lang/Object")?;

        assert_eq!(instructions.len(), 4);
        assert!(matches!(instructions[0], Instruction::Aload_0));
        assert!(matches!(instructions[1], Instruction::Invokespecial(_)));
        assert!(matches!(instructions[2], Instruction::Aconst_null));
        assert!(matches!(instructions[3], Instruction::Athrow));

        Ok(())
    }

    #[test]
    fn unsupported_try_statements_emit_runtime_exception_and_report() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let type_arena = TypeArena::new();
        let symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let try_block = arena.alloc_stmt(Stmt::Block(vec![]));
        let try_stmt = arena.alloc_stmt(Stmt::Try {
            try_block,
            catches: vec![],
            finally_block: None,
        });
        let body = arena.alloc_stmt(Stmt::Block(vec![try_stmt]));

        let mut generator =
            CodeGenerator::new(&arena, &type_arena, &symbol_table, &mut constant_pool);
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;
        let unsupported_features = generator.take_unsupported_features();

        assert_eq!(unsupported_features.len(), 1);
        assert_eq!(
            unsupported_features[0].message.as_str(),
            "unsupported bytecode generation feature: try statements"
        );
        assert_eq!(unsupported_features[0].marker.as_str(), "try");
        assert!(matches!(instructions[0], Instruction::New(_)));
        assert!(matches!(instructions[1], Instruction::Dup));
        assert!(matches!(
            instructions[2],
            Instruction::Ldc(_) | Instruction::Ldc_w(_)
        ));
        assert!(matches!(instructions[3], Instruction::Invokespecial(_)));
        assert!(matches!(instructions[4], Instruction::Athrow));

        Ok(())
    }

    #[test]
    fn instanceof_expressions_emit_instanceof_for_class_targets() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let null_expr = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Null,
            value: "null".into(),
        }));
        let object_type = symbol_table.type_arena_mut().alloc(Type::class(
            ClassType::new(SharedString::new("Object"))
                .with_package(SharedString::new("java.lang")),
        ));
        let object_ty = arena.alloc_type(AstType::Simple {
            name: SharedString::new("Object"),
            ty: object_type,
            type_args: vec![],
        });
        let instanceof_expr = arena.alloc_expr(AstExpr::InstanceOf {
            expr: null_expr,
            ty: object_ty,
        });
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(instanceof_expr));
        let body = arena.alloc_stmt(Stmt::Block(vec![expr_stmt]));

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;
        let unsupported_features = generator.take_unsupported_features();

        assert!(unsupported_features.is_empty());
        assert_eq!(instructions[0], Instruction::Aconst_null);
        let Instruction::Instanceof(class_index) = instructions[1] else {
            panic!("expected instanceof");
        };
        assert_eq!(
            constant_pool
                .try_get_class(class_index)
                .expect("class constant"),
            "java/lang/Object"
        );
        assert_eq!(instructions[2], Instruction::Pop);
        assert_eq!(instructions[3], Instruction::Return);

        Ok(())
    }

    #[test]
    fn instanceof_expressions_emit_instanceof_for_array_targets() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let null_expr = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Null,
            value: "null".into(),
        }));
        let int_type = symbol_table
            .primitive_type_id("int")
            .expect("missing int type");
        let int_array_type = symbol_table.type_arena_mut().alloc(Type::array(int_type));
        let ast_int_type = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Int,
            ty: int_type,
        });
        let ast_array_type = arena.alloc_type(AstType::Array {
            element_type: ast_int_type,
            dimensions: 1,
            ty: int_array_type,
        });
        let instanceof_expr = arena.alloc_expr(AstExpr::InstanceOf {
            expr: null_expr,
            ty: ast_array_type,
        });
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(instanceof_expr));
        let body = arena.alloc_stmt(Stmt::Block(vec![expr_stmt]));

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;
        let unsupported_features = generator.take_unsupported_features();

        assert!(unsupported_features.is_empty());
        assert_eq!(instructions[0], Instruction::Aconst_null);
        let Instruction::Instanceof(class_index) = instructions[1] else {
            panic!("expected instanceof");
        };
        assert_eq!(
            constant_pool
                .try_get_class(class_index)
                .expect("class constant"),
            "[I"
        );
        assert_eq!(instructions[2], Instruction::Pop);
        assert_eq!(instructions[3], Instruction::Return);

        Ok(())
    }

    #[test]
    fn primitive_new_array_expressions_emit_newarray() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let int_type = symbol_table
            .primitive_type_id("int")
            .expect("missing int type");
        let array_type = symbol_table.type_arena_mut().alloc(Type::array(int_type));
        let ast_int_type = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Int,
            ty: int_type,
        });
        let dimension = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "3".into(),
        }));
        let new_array = arena.alloc_expr(AstExpr::NewArray {
            ty: ast_int_type,
            dimensions: vec![dimension],
            initializer: None,
        });
        arena.expr_typed_mut(new_array).ty = array_type;
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(new_array));
        let body = arena.alloc_stmt(Stmt::Block(vec![expr_stmt]));

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;

        assert_eq!(
            instructions,
            vec![
                Instruction::Iconst_3,
                Instruction::Newarray(JvmArrayType::Int),
                Instruction::Pop,
                Instruction::Return,
            ]
        );

        Ok(())
    }

    #[test]
    fn reference_new_array_expressions_emit_anewarray() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let string_type = symbol_table.type_arena_mut().alloc(Type::class(
            ClassType::new(SharedString::new("String"))
                .with_package(SharedString::new("java.lang")),
        ));
        let array_type = symbol_table
            .type_arena_mut()
            .alloc(Type::array(string_type));
        let ast_string_type = arena.alloc_type(AstType::Simple {
            name: SharedString::new("String"),
            ty: string_type,
            type_args: vec![],
        });
        let dimension = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "4".into(),
        }));
        let new_array = arena.alloc_expr(AstExpr::NewArray {
            ty: ast_string_type,
            dimensions: vec![dimension],
            initializer: None,
        });
        arena.expr_typed_mut(new_array).ty = array_type;
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(new_array));
        let body = arena.alloc_stmt(Stmt::Block(vec![expr_stmt]));

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;

        assert!(matches!(instructions[0], Instruction::Iconst_4));
        let Instruction::Anewarray(class_index) = instructions[1] else {
            panic!("expected anewarray");
        };
        assert_eq!(
            constant_pool
                .try_get_class(class_index)
                .expect("class constant"),
            "java/lang/String"
        );
        assert_eq!(instructions[2], Instruction::Pop);
        assert_eq!(instructions[3], Instruction::Return);

        Ok(())
    }

    #[test]
    fn multidimensional_new_array_expressions_emit_multianewarray() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let int_type = symbol_table
            .primitive_type_id("int")
            .expect("missing int type");
        let nested_array = symbol_table.type_arena_mut().alloc(Type::array(int_type));
        let array_type = symbol_table
            .type_arena_mut()
            .alloc(Type::array(nested_array));
        let ast_int_type = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Int,
            ty: int_type,
        });
        let rows = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "2".into(),
        }));
        let cols = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "5".into(),
        }));
        let new_array = arena.alloc_expr(AstExpr::NewArray {
            ty: ast_int_type,
            dimensions: vec![rows, cols],
            initializer: None,
        });
        arena.expr_typed_mut(new_array).ty = array_type;
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(new_array));
        let body = arena.alloc_stmt(Stmt::Block(vec![expr_stmt]));

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        let (instructions, _max_stack, _max_locals) =
            generator.generate_method_body(false, &[], body)?;

        assert!(matches!(instructions[0], Instruction::Iconst_2));
        assert!(matches!(instructions[1], Instruction::Iconst_5));
        let Instruction::Multianewarray(class_index, dimensions) = instructions[2] else {
            panic!("expected multianewarray");
        };
        assert_eq!(dimensions, 2);
        assert_eq!(
            constant_pool
                .try_get_class(class_index)
                .expect("class constant"),
            "[[I"
        );
        assert_eq!(instructions[3], Instruction::Pop);
        assert_eq!(instructions[4], Instruction::Return);

        Ok(())
    }

    #[test]
    fn primitive_array_initializer_expressions_emit_stores() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let int_type = symbol_table
            .primitive_type_id("int")
            .expect("missing int type");
        let array_type = symbol_table.type_arena_mut().alloc(Type::array(int_type));
        let ast_element_type = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Int,
            ty: int_type,
        });
        let ast_int_type = arena.alloc_type(AstType::array(ast_element_type, 1));
        let first = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "1".into(),
        }));
        let second = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "2".into(),
        }));
        let initializer = arena.alloc_expr(AstExpr::ArrayInitializer {
            elements: vec![first, second],
        });
        let new_array = arena.alloc_expr(AstExpr::NewArray {
            ty: ast_int_type,
            dimensions: vec![],
            initializer: Some(initializer),
        });
        arena.expr_typed_mut(new_array).ty = array_type;
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(new_array));
        let body = arena.alloc_stmt(Stmt::Block(vec![expr_stmt]));

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        let (instructions, _, _) = generator.generate_method_body(false, &[], body)?;

        assert_eq!(
            instructions,
            vec![
                Instruction::Iconst_2,
                Instruction::Newarray(JvmArrayType::Int),
                Instruction::Dup,
                Instruction::Iconst_0,
                Instruction::Iconst_1,
                Instruction::Iastore,
                Instruction::Dup,
                Instruction::Iconst_1,
                Instruction::Iconst_2,
                Instruction::Iastore,
                Instruction::Pop,
                Instruction::Return,
            ]
        );

        Ok(())
    }

    #[test]
    fn nested_array_initializer_expressions_emit_recursive_array_construction() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();

        let int_type = symbol_table
            .primitive_type_id("int")
            .expect("missing int type");
        let inner_array_type = symbol_table.type_arena_mut().alloc(Type::array(int_type));
        let outer_array_type = symbol_table
            .type_arena_mut()
            .alloc(Type::array(inner_array_type));
        let ast_int_type = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Int,
            ty: int_type,
        });
        let ast_array_type = arena.alloc_type(AstType::array(ast_int_type, 2));
        let first_value = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "1".into(),
        }));
        let first_inner = arena.alloc_expr(AstExpr::ArrayInitializer {
            elements: vec![first_value],
        });
        let second_value = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "2".into(),
        }));
        let third_value = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Int,
            value: "3".into(),
        }));
        let second_inner = arena.alloc_expr(AstExpr::ArrayInitializer {
            elements: vec![second_value, third_value],
        });
        let outer_initializer = arena.alloc_expr(AstExpr::ArrayInitializer {
            elements: vec![first_inner, second_inner],
        });
        let new_array = arena.alloc_expr(AstExpr::NewArray {
            ty: ast_array_type,
            dimensions: vec![],
            initializer: Some(outer_initializer),
        });
        arena.expr_typed_mut(new_array).ty = outer_array_type;
        let expr_stmt = arena.alloc_stmt(Stmt::Expr(new_array));
        let body = arena.alloc_stmt(Stmt::Block(vec![expr_stmt]));

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        let (instructions, _, _) = generator.generate_method_body(false, &[], body)?;

        assert!(matches!(instructions[0], Instruction::Iconst_2));
        let Instruction::Anewarray(class_index) = instructions[1] else {
            panic!("expected outer anewarray");
        };
        assert_eq!(
            constant_pool
                .try_get_class(class_index)
                .expect("class constant"),
            "[I"
        );
        assert!(instructions.contains(&Instruction::Iastore));
        assert!(instructions.contains(&Instruction::Aastore));
        assert_eq!(instructions.last(), Some(&Instruction::Return));

        Ok(())
    }

    #[test]
    fn method_call_stack_delta_handles_void_and_wide_descriptors() {
        assert_eq!(
            method_call_stack_delta("(Ljava/lang/String;)V", true),
            Some(-2)
        );
        assert_eq!(method_call_stack_delta("()I", true), Some(0));
        assert_eq!(method_call_stack_delta("(JD)J", true), Some(-3));
        assert_eq!(method_call_stack_delta("(I)V", false), Some(-1));
    }

    #[test]
    fn resolved_static_method_calls_emit_invokestatic() -> RajacResult<()> {
        let arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();
        let void_type = symbol_table
            .primitive_type_id_by_kind(rajac_types::PrimitiveType::Void)
            .expect("void type");
        let method_id = add_owner_method(
            &mut symbol_table,
            "example",
            "Util",
            SymbolKind::Class,
            MethodSignature::new(
                SharedString::new("ping"),
                vec![],
                void_type,
                MethodModifiers(MethodModifiers::STATIC | MethodModifiers::PUBLIC),
            ),
        );

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        generator.emit_method_call(
            None,
            &Ident::new("ping".into()),
            &[],
            Some(method_id),
            void_type,
        )?;

        assert!(matches!(
            generator.emitter.code_items.as_slice(),
            [CodeItem::Instruction(Instruction::Invokestatic(_))]
        ));
        Ok(())
    }

    #[test]
    fn resolved_interface_method_calls_emit_invokeinterface() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();
        let void_type = symbol_table
            .primitive_type_id_by_kind(rajac_types::PrimitiveType::Void)
            .expect("void type");
        let method_id = add_owner_method(
            &mut symbol_table,
            "example",
            "RunnableLike",
            SymbolKind::Interface,
            MethodSignature::new(
                SharedString::new("run"),
                vec![],
                void_type,
                MethodModifiers(MethodModifiers::PUBLIC),
            ),
        );
        let receiver_expr = arena.alloc_expr(AstExpr::Literal(Literal {
            kind: LiteralKind::Null,
            value: "null".into(),
        }));

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        generator.emit_method_call(
            Some(&receiver_expr),
            &Ident::new("run".into()),
            &[],
            Some(method_id),
            void_type,
        )?;

        assert!(matches!(
            generator.emitter.code_items.as_slice(),
            [
                CodeItem::Instruction(Instruction::Aconst_null),
                CodeItem::Instruction(Instruction::Invokeinterface(_, 1))
            ]
        ));
        Ok(())
    }

    #[test]
    fn implicit_private_method_calls_emit_invokevirtual() -> RajacResult<()> {
        let arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();
        let void_type = symbol_table
            .primitive_type_id_by_kind(rajac_types::PrimitiveType::Void)
            .expect("void type");
        let method_id = add_owner_method(
            &mut symbol_table,
            "example",
            "Widget",
            SymbolKind::Class,
            MethodSignature::new(
                SharedString::new("tick"),
                vec![],
                void_type,
                MethodModifiers(MethodModifiers::PRIVATE),
            ),
        );

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        generator.emit_method_call(
            None,
            &Ident::new("tick".into()),
            &[],
            Some(method_id),
            void_type,
        )?;

        assert!(matches!(
            generator.emitter.code_items.as_slice(),
            [
                CodeItem::Instruction(Instruction::Aload_0),
                CodeItem::Instruction(Instruction::Invokevirtual(_))
            ]
        ));
        Ok(())
    }

    #[test]
    fn explicit_super_method_calls_emit_invokespecial() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut symbol_table = SymbolTable::new();
        let mut constant_pool = ConstantPool::new();
        let void_type = symbol_table
            .primitive_type_id_by_kind(rajac_types::PrimitiveType::Void)
            .expect("void type");
        let method_id = add_owner_method(
            &mut symbol_table,
            "example",
            "Base",
            SymbolKind::Class,
            MethodSignature::new(
                SharedString::new("tick"),
                vec![],
                void_type,
                MethodModifiers(MethodModifiers::PUBLIC),
            ),
        );
        let super_expr = arena.alloc_expr(AstExpr::Super);

        let mut generator = CodeGenerator::new(
            &arena,
            symbol_table.type_arena(),
            &symbol_table,
            &mut constant_pool,
        );
        generator.emit_method_call(
            Some(&super_expr),
            &Ident::new("tick".into()),
            &[],
            Some(method_id),
            void_type,
        )?;

        assert!(matches!(
            generator.emitter.code_items.as_slice(),
            [
                CodeItem::Instruction(Instruction::Aload_0),
                CodeItem::Instruction(Instruction::Invokespecial(_))
            ]
        ));
        Ok(())
    }
}
