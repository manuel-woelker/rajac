use rajac_ast::{Ast, AstArena, ClassKind, ClassMemberId, ExprId, StmtId};
use rajac_base::qualified_name::QualifiedName as ResolvedName;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_base::shared_string::SharedString;
use rajac_bytecode::classfile::generate_classfiles;
use rajac_classpath::Classpath;
use rajac_parser::parse;
use rajac_symbols::{Symbol, SymbolKind, SymbolTable};
use rajac_types::{Ident, Type, TypeId, WildcardBound};
use rayon::prelude::*;
use ristretto_classfile::attributes::Attribute;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct CompilationUnit {
    pub source_file: PathBuf,
    pub parse_result: ParseResult,
}

type ParseResult = rajac_parser::ParseResult;

#[allow(dead_code)]
pub struct Compiler {
    symbol_table: SymbolTable,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            symbol_table: SymbolTable::new(),
        }
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiler {
    pub fn compile_directory(&self, source_dir: &Path, target_dir: &Path) -> RajacResult<()> {
        fs::create_dir_all(target_dir).context("Failed to create target directory")?;

        let java_files = self.find_java_files(source_dir)?;

        if java_files.is_empty() {
            return Ok(());
        }

        let mut compilation_units: Vec<CompilationUnit> = java_files
            .par_iter()
            .map(|java_file| {
                let source = fs::read_to_string(java_file).context("Failed to read source file")?;
                let parse_result = parse(&source);
                Ok(CompilationUnit {
                    source_file: java_file.clone(),
                    parse_result,
                })
            })
            .collect::<RajacResult<Vec<_>>>()?;

        let mut symbol_table = SymbolTable::new();

        let rt_jar = PathBuf::from("/usr/lib/jvm/java-8-openjdk/jre/lib/rt.jar");
        if rt_jar.exists() {
            let mut classpath = Classpath::new();
            classpath.add_jar(rt_jar);
            classpath.add_to_symbol_table(&mut symbol_table)?;
        }

        for unit in &compilation_units {
            populate_symbol_table(
                &mut symbol_table,
                &unit.parse_result.ast,
                &unit.parse_result.arena,
            );
        }

        compilation_units.par_iter_mut().for_each(|unit| {
            resolve_identifiers(
                &unit.parse_result.ast,
                &mut unit.parse_result.arena,
                &symbol_table,
            );
        });

        let results: Vec<RajacResult<usize>> = compilation_units
            .par_iter()
            .map(|unit| {
                emit_classfiles(
                    &unit.parse_result.ast,
                    &unit.parse_result.arena,
                    &unit.source_file,
                    target_dir,
                )
            })
            .collect();

        let mut total_classfiles = 0;
        for result in results {
            total_classfiles += result?;
        }

        println!(
            "Compiled {} Java files -> {} class files",
            java_files.len(),
            total_classfiles
        );

        Ok(())
    }

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
}

fn populate_symbol_table(symbol_table: &mut SymbolTable, ast: &Ast, arena: &AstArena) {
    let package_name = ast
        .package
        .as_ref()
        .map(|p| {
            p.name
                .segments
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(".")
        })
        .unwrap_or_default();

    let package = symbol_table.package(&package_name);

    for class_id in &ast.classes {
        let class = arena.class_decl(*class_id);
        let name = class.name.name.clone();
        let kind = match class.kind {
            ClassKind::Class => SymbolKind::Class,
            ClassKind::Interface => SymbolKind::Interface,
            ClassKind::Enum | ClassKind::Record | ClassKind::Annotation => continue,
        };
        package.insert(name.to_string(), Symbol::new(name, kind));
    }
}

/// Context for resolving identifiers using the symbol table, package, and imports.
struct ResolveContext<'a> {
    symbol_table: &'a SymbolTable,
    current_package: String,
    single_type_imports: Vec<(String, String)>,
    on_demand_imports: Vec<String>,
}

impl<'a> ResolveContext<'a> {
    /// Builds a resolution context from the current AST and symbol table.
    fn new(ast: &Ast, symbol_table: &'a SymbolTable) -> Self {
        let current_package = ast
            .package
            .as_ref()
            .map(|p| package_name_from_segments(&p.name.segments))
            .unwrap_or_default();

        let mut single_type_imports = Vec::new();
        let mut on_demand_imports = Vec::new();

        for import in &ast.imports {
            if import.is_on_demand {
                on_demand_imports.push(package_name_from_segments(&import.name.segments));
            } else if let Some((package, name)) = split_import_name(&import.name.segments) {
                single_type_imports.push((package, name));
            }
        }

        Self {
            symbol_table,
            current_package,
            single_type_imports,
            on_demand_imports,
        }
    }
}

/// Resolves identifiers in the AST and updates their qualified names in-place.
fn resolve_identifiers(ast: &Ast, arena: &mut AstArena, symbol_table: &SymbolTable) {
    let context = ResolveContext::new(ast, symbol_table);

    for stmt_id in &ast.statements {
        resolve_stmt(*stmt_id, arena, &context);
    }

    for class_id in &ast.classes {
        resolve_class_decl(*class_id, arena, &context);
    }
}

/// Resolves a class declaration and all nested members.
fn resolve_class_decl(
    class_id: rajac_ast::ClassDeclId,
    arena: &mut AstArena,
    context: &ResolveContext,
) {
    let (members, extends, implements, permits) = {
        let class = &mut arena.class_decls[class_id.0 as usize];
        resolve_ident(&mut class.name, context);
        for param in &mut class.type_params {
            resolve_ident(&mut param.name, context);
        }
        (
            class.members.clone(),
            class.extends,
            class.implements.clone(),
            class.permits.clone(),
        )
    };

    if let Some(type_id) = extends {
        resolve_type(type_id, arena, context);
    }
    for type_id in implements {
        resolve_type(type_id, arena, context);
    }
    for type_id in permits {
        resolve_type(type_id, arena, context);
    }
    for member_id in members {
        resolve_class_member(member_id, arena, context);
    }
}

/// Resolves identifiers in a class member.
fn resolve_class_member(member_id: ClassMemberId, arena: &mut AstArena, context: &ResolveContext) {
    let mut member = arena.class_members[member_id.0 as usize].clone();

    match &mut member {
        rajac_ast::ClassMember::Field(field) => resolve_field(field, arena, context),
        rajac_ast::ClassMember::Method(method) => resolve_method(method, arena, context),
        rajac_ast::ClassMember::Constructor(constructor) => {
            resolve_constructor(constructor, arena, context)
        }
        rajac_ast::ClassMember::StaticBlock(stmt_id) => resolve_stmt(*stmt_id, arena, context),
        rajac_ast::ClassMember::NestedClass(class_id)
        | rajac_ast::ClassMember::NestedInterface(class_id)
        | rajac_ast::ClassMember::NestedRecord(class_id)
        | rajac_ast::ClassMember::NestedAnnotation(class_id) => {
            resolve_class_decl(*class_id, arena, context)
        }
        rajac_ast::ClassMember::NestedEnum(enum_decl) => {
            resolve_enum_decl(enum_decl, arena, context)
        }
    }

    arena.class_members[member_id.0 as usize] = member;
}

/// Resolves identifiers in an enum declaration.
fn resolve_enum_decl(
    enum_decl: &mut rajac_ast::EnumDecl,
    arena: &mut AstArena,
    context: &ResolveContext,
) {
    resolve_ident(&mut enum_decl.name, context);

    for type_id in enum_decl.implements.clone() {
        resolve_type(type_id, arena, context);
    }

    for entry in &mut enum_decl.entries {
        resolve_ident(&mut entry.name, context);
        for expr_id in entry.args.clone() {
            resolve_expr(expr_id, arena, context);
        }
        if let Some(members) = &entry.body {
            for member_id in members.clone() {
                resolve_class_member(member_id, arena, context);
            }
        }
    }

    for member_id in enum_decl.members.clone() {
        resolve_class_member(member_id, arena, context);
    }
}

/// Resolves identifiers in a field declaration.
fn resolve_field(field: &mut rajac_ast::Field, arena: &mut AstArena, context: &ResolveContext) {
    resolve_ident(&mut field.name, context);
    resolve_type(field.ty, arena, context);
    if let Some(expr_id) = field.initializer {
        resolve_expr(expr_id, arena, context);
    }
}

/// Resolves identifiers in a method declaration.
fn resolve_method(method: &mut rajac_ast::Method, arena: &mut AstArena, context: &ResolveContext) {
    resolve_ident(&mut method.name, context);
    for param_id in method.params.clone() {
        resolve_param(param_id, arena, context);
    }
    resolve_type(method.return_ty, arena, context);
    for throws_id in method.throws.clone() {
        resolve_type(throws_id, arena, context);
    }
    if let Some(body) = method.body {
        resolve_stmt(body, arena, context);
    }
}

/// Resolves identifiers in a constructor declaration.
fn resolve_constructor(
    constructor: &mut rajac_ast::Constructor,
    arena: &mut AstArena,
    context: &ResolveContext,
) {
    resolve_ident(&mut constructor.name, context);
    for param_id in constructor.params.clone() {
        resolve_param(param_id, arena, context);
    }
    for throws_id in constructor.throws.clone() {
        resolve_type(throws_id, arena, context);
    }
    if let Some(body) = constructor.body {
        resolve_stmt(body, arena, context);
    }
}

/// Resolves identifiers in a parameter.
fn resolve_param(param_id: rajac_ast::ParamId, arena: &mut AstArena, context: &ResolveContext) {
    let param = &mut arena.params[param_id.0 as usize];
    resolve_ident(&mut param.name, context);
    resolve_type(param.ty, arena, context);
}

/// Resolves identifiers in a statement.
fn resolve_stmt(stmt_id: StmtId, arena: &mut AstArena, context: &ResolveContext) {
    let (exprs, stmts, types, params) = {
        let stmt = &mut arena.stmts[stmt_id.0 as usize];
        let mut exprs = Vec::new();
        let mut stmts = Vec::new();
        let mut types = Vec::new();
        let mut params = Vec::new();

        match stmt {
            rajac_ast::Stmt::Empty => {}
            rajac_ast::Stmt::Block(items) => {
                stmts.extend(items.iter().copied());
            }
            rajac_ast::Stmt::Expr(expr_id) => exprs.push(*expr_id),
            rajac_ast::Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                exprs.push(*condition);
                stmts.push(*then_branch);
                if let Some(else_branch) = else_branch {
                    stmts.push(*else_branch);
                }
            }
            rajac_ast::Stmt::While { condition, body } => {
                exprs.push(*condition);
                stmts.push(*body);
            }
            rajac_ast::Stmt::DoWhile { body, condition } => {
                stmts.push(*body);
                exprs.push(*condition);
            }
            rajac_ast::Stmt::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init {
                    match init {
                        rajac_ast::ForInit::Expr(expr_id) => exprs.push(*expr_id),
                        rajac_ast::ForInit::LocalVar {
                            ty,
                            name,
                            initializer,
                        } => {
                            types.push(*ty);
                            resolve_ident(name, context);
                            if let Some(init) = initializer {
                                exprs.push(*init);
                            }
                        }
                    }
                }
                if let Some(condition) = condition {
                    exprs.push(*condition);
                }
                if let Some(update) = update {
                    exprs.push(*update);
                }
                stmts.push(*body);
            }
            rajac_ast::Stmt::Switch { expr, cases } => {
                exprs.push(*expr);
                for case in cases {
                    for label in &case.labels {
                        match label {
                            rajac_ast::SwitchLabel::Case(expr_id) => exprs.push(*expr_id),
                            rajac_ast::SwitchLabel::Default => {}
                        }
                    }
                    stmts.extend(case.body.iter().copied());
                }
            }
            rajac_ast::Stmt::Return(expr_id) => {
                if let Some(expr_id) = expr_id {
                    exprs.push(*expr_id);
                }
            }
            rajac_ast::Stmt::Break(name) | rajac_ast::Stmt::Continue(name) => {
                if let Some(name) = name {
                    resolve_ident(name, context);
                }
            }
            rajac_ast::Stmt::Label(name, body) => {
                resolve_ident(name, context);
                stmts.push(*body);
            }
            rajac_ast::Stmt::Try {
                try_block,
                catches,
                finally_block,
            } => {
                stmts.push(*try_block);
                for clause in catches {
                    params.push(clause.param);
                    stmts.push(clause.body);
                }
                if let Some(finally_block) = finally_block {
                    stmts.push(*finally_block);
                }
            }
            rajac_ast::Stmt::Throw(expr_id) => exprs.push(*expr_id),
            rajac_ast::Stmt::Synchronized { expr, block } => {
                if let Some(expr_id) = expr {
                    exprs.push(*expr_id);
                }
                stmts.push(*block);
            }
            rajac_ast::Stmt::LocalVar {
                ty,
                name,
                initializer,
            } => {
                types.push(*ty);
                resolve_ident(name, context);
                if let Some(init) = initializer {
                    exprs.push(*init);
                }
            }
        }

        (exprs, stmts, types, params)
    };

    for type_id in types {
        resolve_type(type_id, arena, context);
    }
    for param_id in params {
        resolve_param(param_id, arena, context);
    }
    for expr_id in exprs {
        resolve_expr(expr_id, arena, context);
    }
    for stmt_id in stmts {
        resolve_stmt(stmt_id, arena, context);
    }
}

/// Resolves identifiers in an expression.
fn resolve_expr(expr_id: ExprId, arena: &mut AstArena, context: &ResolveContext) {
    let (exprs, types) = {
        let expr = &mut arena.exprs[expr_id.0 as usize];
        let mut exprs = Vec::new();
        let mut types = Vec::new();

        match expr {
            rajac_ast::Expr::Error => {}
            rajac_ast::Expr::Ident(name) => resolve_ident(name, context),
            rajac_ast::Expr::Literal(_) => {}
            rajac_ast::Expr::Unary { expr, .. } => exprs.push(*expr),
            rajac_ast::Expr::Binary { lhs, rhs, .. } => {
                exprs.push(*lhs);
                exprs.push(*rhs);
            }
            rajac_ast::Expr::Assign { lhs, rhs, .. } => {
                exprs.push(*lhs);
                exprs.push(*rhs);
            }
            rajac_ast::Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                exprs.push(*condition);
                exprs.push(*then_expr);
                exprs.push(*else_expr);
            }
            rajac_ast::Expr::Cast { ty, expr } => {
                types.push(*ty);
                exprs.push(*expr);
            }
            rajac_ast::Expr::InstanceOf { expr, ty } => {
                exprs.push(*expr);
                types.push(*ty);
            }
            rajac_ast::Expr::FieldAccess { expr, name } => {
                exprs.push(*expr);
                resolve_ident(name, context);
            }
            rajac_ast::Expr::MethodCall {
                expr,
                name,
                type_args,
                args,
            } => {
                if let Some(expr_id) = expr {
                    exprs.push(*expr_id);
                }
                resolve_ident(name, context);
                if let Some(type_args) = type_args {
                    types.extend(type_args.iter().copied());
                }
                exprs.extend(args.iter().copied());
            }
            rajac_ast::Expr::New { ty, args } => {
                types.push(*ty);
                exprs.extend(args.iter().copied());
            }
            rajac_ast::Expr::NewArray { ty, dimensions } => {
                types.push(*ty);
                exprs.extend(dimensions.iter().copied());
            }
            rajac_ast::Expr::ArrayAccess { array, index } => {
                exprs.push(*array);
                exprs.push(*index);
            }
            rajac_ast::Expr::ArrayLength { array } => exprs.push(*array),
            rajac_ast::Expr::This(expr_id) => {
                if let Some(expr_id) = expr_id {
                    exprs.push(*expr_id);
                }
            }
            rajac_ast::Expr::Super => {}
            rajac_ast::Expr::SuperCall {
                name,
                type_args,
                args,
            } => {
                resolve_ident(name, context);
                if let Some(type_args) = type_args {
                    types.extend(type_args.iter().copied());
                }
                exprs.extend(args.iter().copied());
            }
        }

        (exprs, types)
    };

    for type_id in types {
        resolve_type(type_id, arena, context);
    }
    for expr_id in exprs {
        resolve_expr(expr_id, arena, context);
    }
}

/// Resolves identifiers in a type.
fn resolve_type(type_id: TypeId, arena: &mut AstArena, context: &ResolveContext) {
    let types = {
        let ty = arena.ty_mut(type_id);
        let mut types = Vec::new();

        match ty {
            Type::Error => {}
            Type::Primitive(_) => {}
            Type::Class(class_type) => {
                // Note: class_type.name is now a String, not Ident
                if !class_type.type_args.is_empty() {
                    types.extend(class_type.type_args.iter().copied());
                }
                // Try to resolve the class name if it's not already qualified
                if class_type.package.is_none()
                    && let Some(resolved_name) =
                        resolve_class_name(&SharedString::new(&class_type.name), context)
                {
                    // Update the class type with the resolved package
                    let package = resolved_name.package_name().as_str();
                    if !package.is_empty() {
                        *ty = Type::Class(class_type.clone().with_package(package.to_string()));
                    }
                }
            }
            Type::Array(array_type) => types.push(array_type.element_type),
            Type::TypeVariable(_type_variable) => {
                // Note: type_variable.name is now a String, not Ident
                // resolve_ident(&type_variable.name, context);
            }
            Type::Wildcard(wildcard_type) => {
                if let Some(ref bound) = wildcard_type.bound {
                    match bound {
                        WildcardBound::Extends(type_id) | WildcardBound::Super(type_id) => {
                            types.push(*type_id)
                        }
                    }
                }
            }
        }

        types
    };

    for type_id in types {
        resolve_type(type_id, arena, context);
    }
}

/// Resolves a single identifier if it maps to a known symbol.
fn resolve_ident(ident: &mut Ident, context: &ResolveContext) {
    if ident.qualified_name != ResolvedName::default() {
        return;
    }

    if let Some(resolved) = resolve_class_name(&ident.name, context) {
        ident.qualified_name = resolved;
    }
}

/// Resolves a class name using the current package and imports.
fn resolve_class_name(name: &SharedString, context: &ResolveContext) -> Option<ResolvedName> {
    let name_str = name.as_str();

    // Special case for common Java types that should be fully qualified
    if let Some(qualified_name) = resolve_common_java_type(name_str) {
        return Some(qualified_name);
    }

    for (package, import_name) in &context.single_type_imports {
        if import_name == name_str && package_has_symbol(context.symbol_table, package, name_str) {
            return Some(ResolvedName::new(SharedString::new(package), name.clone()));
        }
    }

    if package_has_symbol(context.symbol_table, &context.current_package, name_str) {
        return Some(ResolvedName::new(
            SharedString::new(&context.current_package),
            name.clone(),
        ));
    }

    for package in &context.on_demand_imports {
        if package_has_symbol(context.symbol_table, package, name_str) {
            return Some(ResolvedName::new(SharedString::new(package), name.clone()));
        }
    }

    None
}

/// Resolves common Java types to their fully qualified names.
fn resolve_common_java_type(name_str: &str) -> Option<ResolvedName> {
    match name_str {
        "String" => Some(ResolvedName::new(
            SharedString::new("java/lang"),
            SharedString::new("String"),
        )),
        "Object" => Some(ResolvedName::new(
            SharedString::new("java/lang"),
            SharedString::new("Object"),
        )),
        "System" => Some(ResolvedName::new(
            SharedString::new("java/lang"),
            SharedString::new("System"),
        )),
        "PrintStream" => Some(ResolvedName::new(
            SharedString::new("java/io"),
            SharedString::new("PrintStream"),
        )),
        _ => None,
    }
}

/// Returns true if the symbol table contains a class in the given package.
fn package_has_symbol(symbol_table: &SymbolTable, package: &str, name: &str) -> bool {
    symbol_table
        .get_package(package)
        .is_some_and(|pkg| pkg.contains(name))
}

/// Joins qualified name segments into a Java-style package name.
fn package_name_from_segments(segments: &[SharedString]) -> String {
    segments
        .iter()
        .map(|segment| segment.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

/// Splits import segments into (package, name).
fn split_import_name(segments: &[SharedString]) -> Option<(String, String)> {
    let (name, package) = segments.split_last()?;
    let package = package_name_from_segments(package);
    Some((package, name.as_str().to_string()))
}

fn emit_classfiles(
    ast: &Ast,
    arena: &AstArena,
    source_file: &Path,
    target_dir: &Path,
) -> RajacResult<usize> {
    let mut class_files = generate_classfiles(ast, arena)?;

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

    let classfile_count = class_files.len();

    for class_file in class_files {
        let class_name = class_file
            .constant_pool
            .try_get_class(class_file.this_class)
            .context("Failed to get class name from constant pool")?;

        let class_path = target_dir.join(format!("{}.class", class_name));

        if let Some(parent) = class_path.parent() {
            fs::create_dir_all(parent).context("Failed to create package directory")?;
        }

        let mut bytes = Vec::new();
        class_file.to_bytes(&mut bytes)?;
        fs::write(&class_path, &bytes).context(format!(
            "Failed to write class file: {}",
            class_path.display()
        ))?;
    }

    Ok(classfile_count)
}
