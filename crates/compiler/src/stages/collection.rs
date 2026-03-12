/* 📖 # Why separate collection into its own stage?
The collection phase builds symbol tables from parsed ASTs.
This is where we discover all classes, methods, and fields
and populate the symbol table that will be used during resolution.
Separating this stage allows for focused testing of symbol table
construction and potential optimization of symbol discovery.
*/

use crate::CompilationUnit;
use rajac_ast::{Ast, AstArena, ClassKind};
use rajac_base::result::RajacResult;
use rajac_classpath::Classpath;
use rajac_symbols::{Symbol, SymbolKind, SymbolTable};
use std::path::Path;

/// Populates the symbol table with symbols from compilation units and runtime library.
pub fn collect_symbols(
    symbol_table: &mut SymbolTable,
    compilation_units: &[CompilationUnit],
) -> RajacResult<()> {
    // Add runtime library symbols (rt.jar)
    let rt_jar = Path::new("/usr/lib/jvm/java-8-openjdk/jre/lib/rt.jar");
    if rt_jar.exists() {
        let mut classpath = Classpath::new();
        classpath.add_jar(rt_jar);
        classpath.add_to_symbol_table(symbol_table)?;
    }

    // Add symbols from compilation units
    for unit in compilation_units {
        populate_symbol_table(symbol_table, &unit.ast, &unit.arena);
    }

    Ok(())
}

/// Populates the symbol table with symbols from a single AST.
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
