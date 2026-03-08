use rajac_ast::{ClassMember, Expr, Stmt};
use rajac_parser::parse;
use std::path::Path;
use walkdir::WalkDir;

/// Count all AST nodes in the parsed AST
fn count_ast_nodes(ast: &rajac_ast::Ast, arena: &rajac_ast::AstArena) -> usize {
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
            count += count_member_nodes(member, arena);
        }
    }

    count
}

fn count_member_nodes(member: &ClassMember, arena: &rajac_ast::AstArena) -> usize {
    let mut count = 1; // The member itself

    match member {
        ClassMember::Field(field) => {
            count += 1; // Field type
            if let Some(init_id) = field.initializer {
                count += count_expr_nodes(init_id, arena);
            }
        }
        ClassMember::Method(method) => {
            count += 1; // Return type
            count += method.params.len();
            if let Some(body_id) = method.body {
                count += count_stmt_nodes(body_id, arena);
            }
        }
        ClassMember::Constructor(constructor) => {
            count += constructor.params.len();
            if let Some(body_id) = constructor.body {
                count += count_stmt_nodes(body_id, arena);
            }
        }
        ClassMember::StaticBlock(stmt_id) => {
            count += count_stmt_nodes(*stmt_id, arena);
        }
        ClassMember::NestedClass(class_id) => {
            let nested_class = arena.class_decl(*class_id);
            count += nested_class.members.len();
            for nested_member_id in &nested_class.members {
                count += count_member_nodes(arena.class_member(*nested_member_id), arena);
            }
        }
        ClassMember::NestedInterface(class_id) => {
            let nested_class = arena.class_decl(*class_id);
            count += nested_class.members.len();
            for nested_member_id in &nested_class.members {
                count += count_member_nodes(arena.class_member(*nested_member_id), arena);
            }
        }
        ClassMember::NestedEnum(_enum_decl) => {
            count += 1; // Enum entries
        }
        ClassMember::NestedRecord(class_id) => {
            let nested_class = arena.class_decl(*class_id);
            count += nested_class.members.len();
            for nested_member_id in &nested_class.members {
                count += count_member_nodes(arena.class_member(*nested_member_id), arena);
            }
        }
        ClassMember::NestedAnnotation(class_id) => {
            let nested_class = arena.class_decl(*class_id);
            count += nested_class.members.len();
            for nested_member_id in &nested_class.members {
                count += count_member_nodes(arena.class_member(*nested_member_id), arena);
            }
        }
    }

    count
}

fn count_stmt_nodes(stmt_id: rajac_ast::StmtId, arena: &rajac_ast::AstArena) -> usize {
    let mut count = 1; // The statement itself

    let stmt = arena.stmt(stmt_id);
    match stmt {
        Stmt::Empty | Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(None) | Stmt::Throw(_) => {
            // These have minimal additional nodes
        }
        Stmt::Block(stmts) => {
            for stmt_id in stmts {
                count += count_stmt_nodes(*stmt_id, arena);
            }
        }
        Stmt::Expr(expr_id) => {
            count += count_expr_nodes(*expr_id, arena);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            count += count_expr_nodes(*condition, arena);
            count += count_stmt_nodes(*then_branch, arena);
            if let Some(else_stmt) = else_branch {
                count += count_stmt_nodes(*else_stmt, arena);
            }
        }
        Stmt::While { condition, body } => {
            count += count_expr_nodes(*condition, arena);
            count += count_stmt_nodes(*body, arena);
        }
        Stmt::DoWhile { body, condition } => {
            count += count_stmt_nodes(*body, arena);
            count += count_expr_nodes(*condition, arena);
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
                count += count_expr_nodes(*cond, arena);
            }
            if let Some(upd) = update {
                count += count_expr_nodes(*upd, arena);
            }
            count += count_stmt_nodes(*body, arena);
        }
        Stmt::Switch { expr, cases } => {
            count += count_expr_nodes(*expr, arena);
            for case in cases {
                count += case.labels.len();
                for stmt_id in &case.body {
                    count += count_stmt_nodes(*stmt_id, arena);
                }
            }
        }
        Stmt::Return(Some(expr_id)) => {
            count += count_expr_nodes(*expr_id, arena);
        }
        Stmt::Label(_, stmt_id) => {
            count += count_stmt_nodes(*stmt_id, arena);
        }
        Stmt::Try {
            try_block,
            catches,
            finally_block,
        } => {
            count += count_stmt_nodes(*try_block, arena);
            for catch_clause in catches {
                count += 1; // Catch parameter
                count += count_stmt_nodes(catch_clause.body, arena);
            }
            if let Some(finally_stmt) = finally_block {
                count += count_stmt_nodes(*finally_stmt, arena);
            }
        }
        Stmt::Synchronized { expr, block } => {
            if let Some(expr_id) = expr {
                count += count_expr_nodes(*expr_id, arena);
            }
            count += count_stmt_nodes(*block, arena);
        }
        Stmt::LocalVar {
            ty: _,
            name: _,
            initializer,
        } => {
            if let Some(init) = initializer {
                count += count_expr_nodes(*init, arena);
            }
        }
    }

    count
}

fn count_expr_nodes(expr_id: rajac_ast::ExprId, arena: &rajac_ast::AstArena) -> usize {
    let mut count = 1; // The expression itself

    let expr = arena.expr(expr_id);
    match expr {
        Expr::Error | Expr::Ident(_) | Expr::Literal(_) | Expr::This(_) | Expr::Super => {
            // These are leaf nodes
        }
        Expr::Unary { op: _, expr } => {
            count += count_expr_nodes(*expr, arena);
        }
        Expr::Binary { op: _, lhs, rhs } => {
            count += count_expr_nodes(*lhs, arena);
            count += count_expr_nodes(*rhs, arena);
        }
        Expr::Assign { op: _, lhs, rhs } => {
            count += count_expr_nodes(*lhs, arena);
            count += count_expr_nodes(*rhs, arena);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            count += count_expr_nodes(*condition, arena);
            count += count_expr_nodes(*then_expr, arena);
            count += count_expr_nodes(*else_expr, arena);
        }
        Expr::Cast { ty: _, expr } => {
            count += 1; // Type
            count += count_expr_nodes(*expr, arena);
        }
        Expr::InstanceOf { expr, ty: _ } => {
            count += 1; // Type
            count += count_expr_nodes(*expr, arena);
        }
        Expr::FieldAccess { expr, name: _ } => {
            count += count_expr_nodes(*expr, arena);
        }
        Expr::MethodCall {
            expr,
            name: _,
            type_args,
            args,
        } => {
            if let Some(receiver) = expr {
                count += count_expr_nodes(*receiver, arena);
            }
            if let Some(type_args) = type_args {
                count += type_args.len();
            }
            for arg in args {
                count += count_expr_nodes(*arg, arena);
            }
        }
        Expr::New { ty: _, args } => {
            count += 1; // Type
            for arg in args {
                count += count_expr_nodes(*arg, arena);
            }
        }
        Expr::NewArray { ty: _, dimensions } => {
            count += 1; // Type
            for dim in dimensions {
                count += count_expr_nodes(*dim, arena);
            }
        }
        Expr::ArrayAccess { array, index } => {
            count += count_expr_nodes(*array, arena);
            count += count_expr_nodes(*index, arena);
        }
        Expr::ArrayLength { array } => {
            count += count_expr_nodes(*array, arena);
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
                count += count_expr_nodes(*arg, arena);
            }
        }
    }

    count
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ballpit".to_string());
    let path = Path::new(&dir);

    let mut total_ast_nodes = 0;

    for entry in WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let file_path = entry.path();
        if file_path.extension().is_some_and(|ext| ext == "java") {
            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", file_path.display(), e);
                    continue;
                }
            };

            let parse_result = parse(&source);
            let ast_nodes = count_ast_nodes(&parse_result.ast, &parse_result.arena);
            total_ast_nodes += ast_nodes;
        }
    }

    println!("Total AST nodes: {}", total_ast_nodes);
}
