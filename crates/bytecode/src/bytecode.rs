use rajac_ast::{
    AstArena, AstType, Expr as AstExpr, ExprId, Literal, LiteralKind, PrimitiveType, Stmt, StmtId,
};
use rajac_base::result::RajacResult;
use rajac_types::Ident;
use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::Instruction;

pub struct BytecodeEmitter {
    pub instructions: Vec<Instruction>,
}

impl BytecodeEmitter {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    pub fn emit(&mut self, instruction: Instruction) {
        self.instructions.push(instruction);
    }
}

impl Default for BytecodeEmitter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CodeGenerator<'arena> {
    arena: &'arena AstArena,
    constant_pool: &'arena mut ConstantPool,
    emitter: BytecodeEmitter,
    max_stack: u16,
    current_stack: i32,
    max_locals: u16,
    next_local_slot: u16,
    local_vars: std::collections::HashMap<String, u16>,
}

impl<'arena> CodeGenerator<'arena> {
    pub fn new(arena: &'arena AstArena, constant_pool: &'arena mut ConstantPool) -> Self {
        Self {
            arena,
            constant_pool,
            emitter: BytecodeEmitter::new(),
            max_stack: 2,
            current_stack: 0,
            max_locals: 1,
            next_local_slot: 1, // slot 0 is for 'this'
            local_vars: std::collections::HashMap::new(),
        }
    }

    pub fn generate_method_body(
        &mut self,
        is_static: bool,
        body_id: StmtId,
    ) -> RajacResult<(Vec<Instruction>, u16, u16)> {
        if !is_static {
            self.max_locals = 1;
        }

        self.emit_statement(body_id)?;

        if self.emulator().instructions.is_empty()
            || !matches!(
                self.emulator().instructions.last(),
                Some(Instruction::Return | Instruction::Areturn | Instruction::Ireturn)
            )
        {
            self.emit(Instruction::Return);
        }

        Ok((
            self.emitter.instructions.clone(),
            self.max_stack,
            self.max_locals,
        ))
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

        Ok((
            self.emitter.instructions.clone(),
            self.max_stack,
            self.max_locals,
        ))
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
                // For now, treat all returns as object references
                self.emit(Instruction::Areturn);
            }
            Stmt::LocalVar {
                ty,
                name,
                initializer,
            } => {
                if let Some(expr_id) = initializer {
                    self.emit_expression(*expr_id)?;
                    let slot = self.next_local_slot;
                    self.next_local_slot += 1;
                    self.max_locals = self.max_locals.max(self.next_local_slot);
                    self.local_vars.insert(name.as_str().to_string(), slot);

                    let ty = self.arena.ty(*ty);
                    match ty {
                        AstType::Primitive {
                            kind: PrimitiveType::Int,
                            ty: _,
                        }
                        | AstType::Primitive {
                            kind: PrimitiveType::Boolean,
                            ty: _,
                        }
                        | AstType::Primitive {
                            kind: PrimitiveType::Byte,
                            ty: _,
                        }
                        | AstType::Primitive {
                            kind: PrimitiveType::Short,
                            ty: _,
                        }
                        | AstType::Primitive {
                            kind: PrimitiveType::Char,
                            ty: _,
                        } => match slot {
                            0 => self.emit(Instruction::Istore_0),
                            1 => self.emit(Instruction::Istore_1),
                            2 => self.emit(Instruction::Istore_2),
                            3 => self.emit(Instruction::Istore_3),
                            _ => self.emit(Instruction::Istore(slot as u8)),
                        },
                        _ => match slot {
                            0 => self.emit(Instruction::Astore_0),
                            1 => self.emit(Instruction::Astore_1),
                            2 => self.emit(Instruction::Astore_2),
                            3 => self.emit(Instruction::Astore_3),
                            _ => self.emit(Instruction::Astore(slot as u8)),
                        },
                    }
                }
            }
            Stmt::If { .. } | Stmt::While { .. } | Stmt::For { .. } | Stmt::DoWhile { .. } => {}
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Label(_, _) | Stmt::Switch { .. } => {}
            Stmt::Throw(_) => {}
            Stmt::Try { .. } | Stmt::Synchronized { .. } => {}
        }
        Ok(())
    }

    pub fn emit_expression(&mut self, expr_id: ExprId) -> RajacResult<()> {
        let expr = self.arena.expr(expr_id);
        match expr {
            AstExpr::Error => {}
            AstExpr::Ident(ident) => {
                if let Some(&slot) = self.local_vars.get(ident.as_str()) {
                    if slot <= 3 {
                        self.emit(match slot {
                            0 => Instruction::Iload_0,
                            1 => Instruction::Iload_1,
                            2 => Instruction::Iload_2,
                            _ => Instruction::Iload_3,
                        });
                    } else {
                        self.emit(Instruction::Iload(slot as u8));
                    }
                }
            }
            AstExpr::Literal(literal) => {
                self.emit_literal(literal)?;
            }
            AstExpr::Unary { op, expr } => {
                self.emit_expression(*expr)?;
                if matches!(op, rajac_ast::UnaryOp::Minus) {
                    self.emit(Instruction::Ineg);
                }
            }
            AstExpr::Binary { op, lhs, rhs } => {
                self.emit_binary_op(op, *lhs, *rhs)?;
            }
            AstExpr::Assign { .. } => {}
            AstExpr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.emit_expression(*condition)?;
                self.emit_expression(*then_expr)?;
                self.emit_expression(*else_expr)?;
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
                ..
            } => {
                self.emit_method_call(expr.as_ref(), name, args)?;
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
                if let Ok(value) = literal.value.parse::<i32>() {
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
                        _ => {}
                    }
                }
            }
            LiteralKind::Long => {
                if let Ok(value) = literal.value.parse::<i64>() {
                    match value {
                        0 => self.emit(Instruction::Lconst_0),
                        1 => self.emit(Instruction::Lconst_1),
                        _ => {}
                    }
                }
            }
            LiteralKind::Float => {
                if let Ok(value) = literal.value.parse::<f32>() {
                    match value {
                        0.0 => self.emit(Instruction::Fconst_0),
                        1.0 => self.emit(Instruction::Fconst_1),
                        2.0 => self.emit(Instruction::Fconst_2),
                        _ => {}
                    }
                }
            }
            LiteralKind::Double => {
                if let Ok(value) = literal.value.parse::<f64>() {
                    match value {
                        0.0 => self.emit(Instruction::Dconst_0),
                        1.0 => self.emit(Instruction::Dconst_1),
                        _ => {}
                    }
                }
            }
            LiteralKind::Char => {
                if let Ok(value) = literal.value.parse::<char>() {
                    let code = value as i32;
                    match code {
                        0..=5 => self.emit(Instruction::Iconst_0),
                        -128..=127 => self.emit(Instruction::Bipush(code as i8)),
                        -32768..=32767 => self.emit(Instruction::Sipush(code as i16)),
                        _ => {}
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

    fn emit_binary_op(
        &mut self,
        op: &rajac_ast::BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
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
                self.emit(Instruction::Iadd);
            }
            BinaryOp::Sub => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Isub);
            }
            BinaryOp::Mul => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Imul);
            }
            BinaryOp::Div => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Idiv);
            }
            BinaryOp::Mod => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Irem);
            }
            BinaryOp::BitAnd => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Iand);
            }
            BinaryOp::BitOr => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Ior);
            }
            BinaryOp::BitXor => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Ixor);
            }
            BinaryOp::LShift => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Ishl);
            }
            BinaryOp::RShift => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Ishr);
            }
            BinaryOp::ARShift => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Iushr);
            }
            BinaryOp::Lt
            | BinaryOp::LtEq
            | BinaryOp::Gt
            | BinaryOp::GtEq
            | BinaryOp::EqEq
            | BinaryOp::BangEq => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
            }
            BinaryOp::And | BinaryOp::Or => {
                self.emit_expression(lhs)?;
                self.emit_expression(rhs)?;
                self.emit(Instruction::Iand);
            }
        }
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
    ) -> RajacResult<()> {
        if let Some(target_expr_id) = target {
            let target_expr = self.arena.expr(*target_expr_id);

            let is_println_call = match target_expr {
                AstExpr::FieldAccess {
                    expr: inner_target,
                    name: field_name,
                    ..
                } => {
                    if field_name.as_str() == "out" && name.as_str() == "println" {
                        let inner = self.arena.expr(*inner_target);
                        matches!(inner, AstExpr::Ident(ident) if ident.as_str() == "System")
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if is_println_call {
                return self.emit_println_call(args);
            }
        }

        Ok(())
    }

    fn emit_println_call(&mut self, args: &[ExprId]) -> RajacResult<()> {
        let system_class = self.constant_pool.add_class("java/lang/System")?;
        let printstream_class = self.constant_pool.add_class("java/io/PrintStream")?;
        let system_out =
            self.constant_pool
                .add_field_ref(system_class, "out", "Ljava/io/PrintStream;")?;
        self.emit(Instruction::Getstatic(system_out));

        if !args.is_empty() {
            self.emit_expression(args[0])?;
        } else {
            self.emit(Instruction::Aconst_null);
        }

        let println_method = self.constant_pool.add_method_ref(
            printstream_class,
            "println",
            "(Ljava/lang/String;)V",
        )?;
        self.emit(Instruction::Invokevirtual(println_method));

        Ok(())
    }
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
        Ineg => 0,
        Lneg => 0,
        Fneg | Dneg => 0,
        Ishl | Lshl | Ishr | Lshr | Iushr | Lushr => -1,
        Ireturn | Freturn | Areturn | Return => 0,
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
        _ => 0,
    }
}
