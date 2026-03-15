use super::NestedClassInfo;
use rajac_ast::{Ast, AstArena, ClassDecl, ClassMember};
use rajac_base::shared_string::SharedString;
use rajac_symbols::SymbolTable;
use rajac_types::Ident;

pub(crate) fn collect_nested_class_infos(
    arena: &AstArena,
    class: &ClassDecl,
    this_internal_name: &str,
) -> Vec<NestedClassInfo> {
    let mut nested = Vec::new();

    for member_id in &class.members {
        let member = arena.class_member(*member_id);
        let class_id = match member {
            ClassMember::NestedClass(class_id)
            | ClassMember::NestedInterface(class_id)
            | ClassMember::NestedRecord(class_id)
            | ClassMember::NestedAnnotation(class_id) => *class_id,
            ClassMember::NestedEnum(_) => continue,
            ClassMember::Field(_)
            | ClassMember::Method(_)
            | ClassMember::Constructor(_)
            | ClassMember::StaticBlock(_) => continue,
        };

        let nested_decl = arena.class_decl(class_id);
        let simple_name = SharedString::new(nested_decl.name.as_str());
        let internal_name = SharedString::new(format!("{this_internal_name}${simple_name}"));

        nested.push(NestedClassInfo {
            class_id,
            internal_name,
            simple_name,
            modifiers: nested_decl.modifiers.clone(),
            kind: nested_decl.kind.clone(),
        });
    }

    nested
}

pub(crate) fn internal_class_name(
    ast: &Ast,
    class_name: &Ident,
    symbol_table: &SymbolTable,
) -> String {
    let class_name_str = class_name.as_str();

    let current_package = ast
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

    if let Some(pkg_table) = symbol_table.get_package(&current_package)
        && pkg_table.contains(class_name_str)
    {
        let mut result = current_package.replace('.', "/");
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str(class_name_str);
        return result;
    }

    if let Some(pkg_table) = symbol_table.get_package("java.lang")
        && pkg_table.contains(class_name_str)
    {
        return format!("java/lang/{}", class_name_str);
    }

    match &ast.package {
        Some(pkg) => {
            let mut s = pkg.name.segments.join("/");
            if !s.is_empty() {
                s.push('/');
            }
            s.push_str(class_name_str);
            s
        }
        None => class_name_str.to_string(),
    }
}
