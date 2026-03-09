use rajac_ast::{ClassMember, Expr, Stmt};
use rajac_base::result::{RajacResult, ResultExt};
use rajac_bytecode::classfile::generate_classfiles;
use rajac_parser::parse;
use ristretto_classfile::attributes::Attribute;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Compiler struct that handles compilation of Java source files
pub struct Compiler {
    // Configuration and state can be added here
}

impl Compiler {
    /// Create a new Compiler instance
    pub fn new() -> Self {
        Compiler {
            // Initialize any state here
        }
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiler {
    /// Compile all Java files in a source directory to a target directory
    pub fn compile_directory(&self, source_dir: &Path, target_dir: &Path) -> RajacResult<()> {
        // Create target directory if it doesn't exist
        fs::create_dir_all(target_dir).context("Failed to create target directory")?;

        // Find all Java files in source directory
        let java_files = self.find_java_files(source_dir)?;

        if java_files.is_empty() {
            println!("No Java files found in {}", source_dir.display());
            return Ok(());
        }

        println!("Found {} Java files to compile", java_files.len());

        // Compile each file
        for java_file in &java_files {
            self.compile_file(java_file, target_dir)?;
        }

        Ok(())
    }

    /// Find all Java files in a directory
    fn find_java_files(&self, dir: &Path) -> RajacResult<Vec<PathBuf>> {
        let mut java_files = Vec::new();

        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "java") {
                java_files.push(path.to_path_buf());
            }
        }

        Ok(java_files)
    }

    /// Compile a single Java file
    fn compile_file(&self, source_file: &Path, target_dir: &Path) -> RajacResult<()> {
        println!("Compiling {}...", source_file.display());

        // Read source file
        let source = fs::read_to_string(source_file).context("Failed to read source file")?;

        // Parse the source
        let parse_result = parse(&source);

        // Generate class files
        let mut class_files = generate_classfiles(&parse_result.ast, &parse_result.arena)?;

        for class_file in &mut class_files {
            let source_file_attribute_index = class_file.constant_pool.add_utf8("SourceFile")?;
            let source_file_index = class_file
                .constant_pool
                .add_utf8(source_file.file_name().unwrap().display().to_string())?;
            class_file.attributes.push(Attribute::SourceFile {
                name_index: source_file_attribute_index,
                source_file_index,
            })
        }

        // Write class files to target directory
        for class_file in class_files {
            let class_name = class_file
                .constant_pool
                .try_get_class(class_file.this_class)
                .context("Failed to get class name from constant pool")?;
            let class_path = target_dir.join(format!("{}.class", class_name));

            let mut bytes = Vec::new();
            class_file.to_bytes(&mut bytes)?;
            fs::write(&class_path, &bytes).context(format!(
                "Failed to write class file: {}",
                class_path.display()
            ))?;

            println!("  Generated {}", class_path.display());
        }

        // Count AST nodes for statistics
        let ast_node_count = self.count_ast_nodes(&parse_result.ast, &parse_result.arena);
        println!("  Parsed {} AST nodes", ast_node_count);

        Ok(())
    }

    /// Count all AST nodes in the parsed AST (helper function)
    fn count_ast_nodes(&self, ast: &rajac_ast::Ast, arena: &rajac_ast::AstArena) -> usize {
        let mut count = 1; // Count the compilation unit itself

        // Count classes
        count += ast.classes.len();

        // Count all members in classes
        for class_id in &ast.classes {
            let class = arena.class_decl(*class_id);
            count += class.members.len();

            // Count statements and expressions in members
            for member_id in &class.members {
                let member = arena.class_member(*member_id);
                count += self.count_member_nodes(member, arena);
            }
        }

        count
    }

    fn count_member_nodes(&self, member: &ClassMember, arena: &rajac_ast::AstArena) -> usize {
        let mut count = 1; // The member itself

        match member {
            ClassMember::Field(field) => {
                count += 1; // Field type
                if let Some(init_id) = field.initializer {
                    count += self.count_expr_nodes(init_id, arena);
                }
            }
            ClassMember::Method(method) => {
                count += 1; // Return type
                count += method.params.len();
                if let Some(body_id) = method.body {
                    count += self.count_stmt_nodes(body_id, arena);
                }
            }
            ClassMember::Constructor(constructor) => {
                count += constructor.params.len();
                if let Some(body_id) = constructor.body {
                    count += self.count_stmt_nodes(body_id, arena);
                }
            }
            ClassMember::StaticBlock(stmt_id) => {
                count += self.count_stmt_nodes(*stmt_id, arena);
            }
            ClassMember::NestedClass(class_id) => {
                let nested_class = arena.class_decl(*class_id);
                count += nested_class.members.len();
                for nested_member_id in &nested_class.members {
                    count += self.count_member_nodes(arena.class_member(*nested_member_id), arena);
                }
            }
            ClassMember::NestedInterface(class_id) => {
                let nested_class = arena.class_decl(*class_id);
                count += nested_class.members.len();
                for nested_member_id in &nested_class.members {
                    count += self.count_member_nodes(arena.class_member(*nested_member_id), arena);
                }
            }
            ClassMember::NestedEnum(_enum_decl) => {
                count += 1; // Enum entries
            }
            ClassMember::NestedRecord(class_id) => {
                let nested_class = arena.class_decl(*class_id);
                count += nested_class.members.len();
                for nested_member_id in &nested_class.members {
                    count += self.count_member_nodes(arena.class_member(*nested_member_id), arena);
                }
            }
            ClassMember::NestedAnnotation(class_id) => {
                let nested_class = arena.class_decl(*class_id);
                count += nested_class.members.len();
                for nested_member_id in &nested_class.members {
                    count += self.count_member_nodes(arena.class_member(*nested_member_id), arena);
                }
            }
        }

        count
    }

    fn count_stmt_nodes(&self, stmt_id: rajac_ast::StmtId, arena: &rajac_ast::AstArena) -> usize {
        let mut count = 1; // The statement itself

        let stmt = arena.stmt(stmt_id);
        match stmt {
            Stmt::Empty
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Return(None)
            | Stmt::Throw(_) => {
                // These have minimal additional nodes
            }
            Stmt::Block(stmts) => {
                for stmt_id in stmts {
                    count += self.count_stmt_nodes(*stmt_id, arena);
                }
            }
            Stmt::Expr(expr_id) => {
                count += self.count_expr_nodes(*expr_id, arena);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                count += self.count_expr_nodes(*condition, arena);
                count += self.count_stmt_nodes(*then_branch, arena);
                if let Some(else_stmt) = else_branch {
                    count += self.count_stmt_nodes(*else_stmt, arena);
                }
            }
            Stmt::While { condition, body } => {
                count += self.count_expr_nodes(*condition, arena);
                count += self.count_stmt_nodes(*body, arena);
            }
            Stmt::DoWhile { body, condition } => {
                count += self.count_stmt_nodes(*body, arena);
                count += self.count_expr_nodes(*condition, arena);
            }
            Stmt::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(_init) = init {
                    count += 1; // Init expression/var
                }
                if let Some(cond) = condition {
                    count += self.count_expr_nodes(*cond, arena);
                }
                if let Some(upd) = update {
                    count += self.count_expr_nodes(*upd, arena);
                }
                count += self.count_stmt_nodes(*body, arena);
            }
            Stmt::Switch { expr, cases } => {
                count += self.count_expr_nodes(*expr, arena);
                for case in cases {
                    count += case.labels.len();
                    for stmt_id in &case.body {
                        count += self.count_stmt_nodes(*stmt_id, arena);
                    }
                }
            }
            Stmt::Return(Some(expr_id)) => {
                count += self.count_expr_nodes(*expr_id, arena);
            }
            Stmt::Label(_, stmt_id) => {
                count += self.count_stmt_nodes(*stmt_id, arena);
            }
            Stmt::Try {
                try_block,
                catches,
                finally_block,
            } => {
                count += self.count_stmt_nodes(*try_block, arena);
                for catch_clause in catches {
                    count += 1; // Catch parameter
                    count += self.count_stmt_nodes(catch_clause.body, arena);
                }
                if let Some(finally_stmt) = finally_block {
                    count += self.count_stmt_nodes(*finally_stmt, arena);
                }
            }
            Stmt::Synchronized { expr, block } => {
                if let Some(expr_id) = expr {
                    count += self.count_expr_nodes(*expr_id, arena);
                }
                count += self.count_stmt_nodes(*block, arena);
            }
            Stmt::LocalVar {
                ty: _,
                name: _,
                initializer,
            } => {
                if let Some(init) = initializer {
                    count += self.count_expr_nodes(*init, arena);
                }
            }
        }

        count
    }

    fn count_expr_nodes(&self, expr_id: rajac_ast::ExprId, arena: &rajac_ast::AstArena) -> usize {
        let mut count = 1; // The expression itself

        let expr = arena.expr(expr_id);
        match expr {
            Expr::Error | Expr::Ident(_) | Expr::Literal(_) | Expr::This(_) | Expr::Super => {
                // These are leaf nodes
            }
            Expr::Unary { op: _, expr } => {
                count += self.count_expr_nodes(*expr, arena);
            }
            Expr::Binary { op: _, lhs, rhs } => {
                count += self.count_expr_nodes(*lhs, arena);
                count += self.count_expr_nodes(*rhs, arena);
            }
            Expr::Assign { op: _, lhs, rhs } => {
                count += self.count_expr_nodes(*lhs, arena);
                count += self.count_expr_nodes(*rhs, arena);
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                count += self.count_expr_nodes(*condition, arena);
                count += self.count_expr_nodes(*then_expr, arena);
                count += self.count_expr_nodes(*else_expr, arena);
            }
            Expr::Cast { ty: _, expr } => {
                count += 1; // Type
                count += self.count_expr_nodes(*expr, arena);
            }
            Expr::InstanceOf { expr, ty: _ } => {
                count += 1; // Type
                count += self.count_expr_nodes(*expr, arena);
            }
            Expr::FieldAccess { expr, name: _ } => {
                count += self.count_expr_nodes(*expr, arena);
            }
            Expr::MethodCall {
                expr,
                name: _,
                type_args,
                args,
            } => {
                if let Some(receiver) = expr {
                    count += self.count_expr_nodes(*receiver, arena);
                }
                if let Some(type_args) = type_args {
                    count += type_args.len();
                }
                for arg in args {
                    count += self.count_expr_nodes(*arg, arena);
                }
            }
            Expr::New { ty: _, args } => {
                count += 1; // Type
                for arg in args {
                    count += self.count_expr_nodes(*arg, arena);
                }
            }
            Expr::NewArray { ty: _, dimensions } => {
                count += 1; // Type
                for dim in dimensions {
                    count += self.count_expr_nodes(*dim, arena);
                }
            }
            Expr::ArrayAccess { array, index } => {
                count += self.count_expr_nodes(*array, arena);
                count += self.count_expr_nodes(*index, arena);
            }
            Expr::ArrayLength { array } => {
                count += self.count_expr_nodes(*array, arena);
            }
            Expr::SuperCall {
                name: _,
                type_args,
                args,
            } => {
                if let Some(type_args) = type_args {
                    count += type_args.len();
                }
                for arg in args {
                    count += self.count_expr_nodes(*arg, arena);
                }
            }
        }

        count
    }
}
