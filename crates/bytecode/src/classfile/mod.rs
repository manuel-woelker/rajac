mod attributes;
mod builder;
mod descriptor;
mod generation_context;
mod member_lowering;
mod naming;

use rajac_ast::{Ast, AstArena, ClassDeclId};
use rajac_base::result::RajacResult;
use rajac_symbols::SymbolTable;
use ristretto_classfile::ClassFile;

pub use generation_context::GeneratedClassFiles;

use builder::{classfile_from_class_decl_with_context, emit_classfiles_for_class};
use generation_context::ClassfileGenerationContext;
use naming::internal_class_name;

pub(crate) use attributes::{
    build_inner_classes_attribute, class_access_flags, exceptions_attribute_from_ast_types,
    has_modifier,
};
pub(crate) use descriptor::{
    constructor_to_descriptor, method_to_descriptor, type_to_descriptor,
    type_to_internal_class_name,
};
pub(crate) use generation_context::NestedClassInfo;
pub(crate) use member_lowering::{
    constructor_from_ast, enum_constructor_from_ast, field_from_ast, method_from_ast,
};
pub(crate) use naming::collect_nested_class_infos;

pub fn generate_classfiles(
    ast: &Ast,
    arena: &AstArena,
    type_arena: &rajac_types::TypeArena,
    symbol_table: &SymbolTable,
) -> RajacResult<Vec<ClassFile>> {
    Ok(generate_classfiles_with_report(ast, arena, type_arena, symbol_table)?.class_files)
}

pub fn generate_classfiles_with_report(
    ast: &Ast,
    arena: &AstArena,
    type_arena: &rajac_types::TypeArena,
    symbol_table: &SymbolTable,
) -> RajacResult<GeneratedClassFiles> {
    let mut class_files = Vec::new();
    let mut unsupported_features = Vec::new();
    let mut generation_context = ClassfileGenerationContext {
        type_arena,
        symbol_table,
        unsupported_features: &mut unsupported_features,
    };
    for class_id in &ast.classes {
        let class = arena.class_decl(*class_id);
        let internal_name = internal_class_name(ast, &class.name, symbol_table);
        emit_classfiles_for_class(
            arena,
            *class_id,
            internal_name.into(),
            None,
            &mut class_files,
            &mut generation_context,
        )?;
    }
    Ok(GeneratedClassFiles {
        class_files,
        unsupported_features,
    })
}

pub fn classfile_from_class_decl(
    ast: &Ast,
    arena: &AstArena,
    class_id: ClassDeclId,
    type_arena: &rajac_types::TypeArena,
    symbol_table: &SymbolTable,
) -> RajacResult<ClassFile> {
    let class = arena.class_decl(class_id);
    let this_internal_name = internal_class_name(ast, &class.name, symbol_table);
    let mut unsupported_features = Vec::new();
    let mut generation_context = ClassfileGenerationContext {
        type_arena,
        symbol_table,
        unsupported_features: &mut unsupported_features,
    };
    classfile_from_class_decl_with_context(
        arena,
        class_id,
        &this_internal_name,
        None,
        &[],
        &mut generation_context,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rajac_ast::{
        Ast, AstArena, AstType, ClassDecl, ClassKind, ClassMember, Constructor as AstConstructor,
        EnumEntry, Method, Modifiers, PackageDecl, Param, PrimitiveType, QualifiedName,
    };
    use rajac_base::shared_string::SharedString;
    use rajac_types::Ident;
    use ristretto_classfile::attributes::{Attribute, NestedClassAccessFlags};

    #[test]
    fn generates_minimal_abstract_method_without_code_attribute() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));
        let type_arena = rajac_types::TypeArena::new();
        let symbol_table = SymbolTable::new();

        let void_ty = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Void,
            ty: rajac_types::TypeId::INVALID,
        });
        let int_ty = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Int,
            ty: rajac_types::TypeId::INVALID,
        });

        let param_id = arena.alloc_param(Param {
            ty: int_ty,
            name: Ident::new(SharedString::new("x")),
            modifiers: rajac_ast::Modifiers::default(),
            varargs: false,
        });

        let method = Method {
            name: Ident::new(SharedString::new("f")),
            params: vec![param_id],
            return_ty: void_ty,
            body: None,
            throws: vec![],
            modifiers: Modifiers(Modifiers::PUBLIC),
        };

        let member_id = arena.alloc_class_member(ClassMember::Method(method));

        let class_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Interface,
            name: Ident::new(SharedString::new("Foo")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![],
            members: vec![member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });

        ast.classes.push(class_id);

        let mut class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        assert_eq!(class_files.len(), 1);

        let class_file = class_files.pop().unwrap();
        class_file.verify()?;

        assert_eq!(class_file.methods.len(), 1);
        assert!(class_file.methods[0].attributes.is_empty());

        Ok(())
    }

    #[test]
    fn generates_bytecode_for_methods_with_bodies() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));
        let type_arena = rajac_types::TypeArena::new();
        let symbol_table = SymbolTable::new();

        let void_ty = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Void,
            ty: rajac_types::TypeId::INVALID,
        });
        let empty_block = arena.alloc_stmt(rajac_ast::Stmt::Block(vec![]));

        let method = Method {
            name: Ident::new(SharedString::new("g")),
            params: vec![],
            return_ty: void_ty,
            body: Some(empty_block),
            throws: vec![],
            modifiers: Modifiers(Modifiers::PUBLIC),
        };

        let member_id = arena.alloc_class_member(ClassMember::Method(method));

        let class_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Class,
            name: Ident::new(SharedString::new("Foo")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![],
            members: vec![member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });

        ast.classes.push(class_id);

        let mut class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        assert_eq!(class_files.len(), 1);

        let class_file = class_files.pop().unwrap();
        class_file.verify()?;
        assert_eq!(class_file.methods.len(), 2);

        let method_with_body = class_file
            .methods
            .iter()
            .find(|m| class_file.constant_pool.try_get_utf8(m.name_index).ok() == Some("g"))
            .expect("method 'g' should be present");

        assert!(!method_with_body.attributes.is_empty());
        let has_code = method_with_body
            .attributes
            .iter()
            .any(|attr| matches!(attr, Attribute::Code { .. }));
        assert!(has_code);

        Ok(())
    }

    #[test]
    fn emits_inner_class_files_and_attributes() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));
        let type_arena = rajac_types::TypeArena::new();
        let symbol_table = SymbolTable::new();
        ast.package = Some(PackageDecl {
            name: QualifiedName::new(vec![SharedString::new("p")]),
        });

        let inner_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Class,
            name: Ident::new(SharedString::new("Inner")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![],
            members: vec![],
            modifiers: Modifiers(Modifiers::PRIVATE),
        });

        let inner_member_id = arena.alloc_class_member(ClassMember::NestedClass(inner_id));

        let outer_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Class,
            name: Ident::new(SharedString::new("Outer")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![],
            members: vec![inner_member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });

        ast.classes.push(outer_id);

        let class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        assert_eq!(class_files.len(), 2);

        let mut outer = None;
        let mut inner = None;

        for class_file in &class_files {
            match class_file.class_name()? {
                "p/Outer" => outer = Some(class_file),
                "p/Outer$Inner" => inner = Some(class_file),
                other => panic!("unexpected class emitted: {other}"),
            }
        }

        let outer = outer.expect("outer class not emitted");
        let inner = inner.expect("inner class not emitted");

        let outer_inner_attr = outer
            .attributes
            .iter()
            .find_map(|attr| match attr {
                Attribute::InnerClasses { classes, .. } => Some(classes),
                _ => None,
            })
            .expect("outer class missing InnerClasses attribute");

        let outer_entry = outer_inner_attr
            .iter()
            .find(|entry| {
                outer
                    .constant_pool
                    .try_get_class(entry.class_info_index)
                    .ok()
                    == Some("p/Outer$Inner")
            })
            .expect("outer class missing inner class entry");

        assert_eq!(
            outer
                .constant_pool
                .try_get_class(outer_entry.outer_class_info_index)?,
            "p/Outer"
        );
        assert_eq!(
            outer.constant_pool.try_get_utf8(outer_entry.name_index)?,
            "Inner"
        );
        assert!(
            outer_entry
                .access_flags
                .contains(NestedClassAccessFlags::PRIVATE)
        );

        let inner_attr = inner
            .attributes
            .iter()
            .find_map(|attr| match attr {
                Attribute::InnerClasses { classes, .. } => Some(classes),
                _ => None,
            })
            .expect("inner class missing InnerClasses attribute");

        let inner_entry = inner_attr
            .iter()
            .find(|entry| {
                inner
                    .constant_pool
                    .try_get_class(entry.class_info_index)
                    .ok()
                    == Some("p/Outer$Inner")
            })
            .expect("inner class missing self entry");

        assert_eq!(
            inner
                .constant_pool
                .try_get_class(inner_entry.outer_class_info_index)?,
            "p/Outer"
        );
        assert_eq!(
            inner.constant_pool.try_get_utf8(inner_entry.name_index)?,
            "Inner"
        );

        Ok(())
    }

    #[test]
    fn emits_nested_enum_class_files() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));
        let type_arena = rajac_types::TypeArena::new();
        let symbol_table = SymbolTable::new();
        ast.package = Some(PackageDecl {
            name: QualifiedName::new(vec![SharedString::new("p")]),
        });

        let inner_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Enum,
            name: Ident::new(SharedString::new("Inner")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![EnumEntry {
                name: Ident::new(SharedString::new("VALUE")),
                args: vec![],
                body: None,
            }],
            members: vec![],
            modifiers: Modifiers(Modifiers::PRIVATE),
        });

        let inner_member_id = arena.alloc_class_member(ClassMember::NestedEnum(inner_id));

        let outer_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Class,
            name: Ident::new(SharedString::new("Outer")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![],
            members: vec![inner_member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });

        ast.classes.push(outer_id);

        let class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        assert_eq!(class_files.len(), 2);
        assert!(
            class_files
                .iter()
                .any(|class_file| class_file.class_name().ok() == Some("p/Outer"))
        );
        assert!(
            class_files
                .iter()
                .any(|class_file| class_file.class_name().ok() == Some("p/Outer$Inner"))
        );

        Ok(())
    }

    #[test]
    fn emits_explicit_constructors_as_init_methods() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));
        let type_arena = rajac_types::TypeArena::new();
        let symbol_table = SymbolTable::new();

        let int_ty = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Int,
            ty: rajac_types::TypeId::INVALID,
        });
        let param_id = arena.alloc_param(Param {
            ty: int_ty,
            name: Ident::new(SharedString::new("x")),
            modifiers: rajac_ast::Modifiers::default(),
            varargs: false,
        });
        let body = arena.alloc_stmt(rajac_ast::Stmt::Block(vec![]));
        let constructor = AstConstructor {
            name: Ident::new(SharedString::new("Foo")),
            params: vec![param_id],
            body: Some(body),
            throws: vec![],
            modifiers: Modifiers(Modifiers::PUBLIC),
        };
        let ctor_member_id = arena.alloc_class_member(ClassMember::Constructor(constructor));

        let class_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Class,
            name: Ident::new(SharedString::new("Foo")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![],
            members: vec![ctor_member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });
        ast.classes.push(class_id);

        let mut class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        let class_file = class_files.pop().unwrap();
        class_file.verify()?;

        assert_eq!(class_file.methods.len(), 1);
        let constructor = &class_file.methods[0];
        assert_eq!(
            class_file
                .constant_pool
                .try_get_utf8(constructor.name_index)?,
            "<init>"
        );
        assert_eq!(
            class_file
                .constant_pool
                .try_get_utf8(constructor.descriptor_index)?,
            "(I)V"
        );
        assert!(
            constructor
                .attributes
                .iter()
                .any(|attribute| matches!(attribute, Attribute::Code { .. }))
        );

        Ok(())
    }

    #[test]
    fn emits_enum_fields_and_synthetic_methods() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));
        let type_arena = rajac_types::TypeArena::new();
        let symbol_table = SymbolTable::new();

        let class_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Enum,
            name: Ident::new(SharedString::new("Color")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![
                EnumEntry {
                    name: Ident::new(SharedString::new("RED")),
                    args: vec![],
                    body: None,
                },
                EnumEntry {
                    name: Ident::new(SharedString::new("GREEN")),
                    args: vec![],
                    body: None,
                },
            ],
            members: vec![],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });
        ast.classes.push(class_id);

        let mut class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        let class_file = class_files.pop().unwrap();
        class_file.verify()?;

        let field_names = class_file
            .fields
            .iter()
            .map(|field| class_file.constant_pool.try_get_utf8(field.name_index))
            .collect::<Result<Vec<_>, _>>()?;
        assert!(field_names.contains(&"RED"));
        assert!(field_names.contains(&"GREEN"));
        assert!(field_names.contains(&"$VALUES"));

        let method_names = class_file
            .methods
            .iter()
            .map(|method| class_file.constant_pool.try_get_utf8(method.name_index))
            .collect::<Result<Vec<_>, _>>()?;
        assert!(method_names.contains(&"values"));
        assert!(method_names.contains(&"valueOf"));
        assert!(method_names.contains(&"<clinit>"));

        Ok(())
    }

    #[test]
    fn emits_exceptions_attribute_for_methods() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));
        let mut symbol_table = SymbolTable::new();

        let exception_ty_id = symbol_table.add_class(
            "java.lang",
            "Exception",
            rajac_types::Type::class(
                rajac_types::ClassType::new(SharedString::new("Exception"))
                    .with_package(SharedString::new("java.lang")),
            ),
            rajac_symbols::SymbolKind::Class,
        );
        let type_arena = symbol_table.type_arena().clone();
        let void_ty = arena.alloc_type(AstType::Primitive {
            kind: PrimitiveType::Void,
            ty: rajac_types::TypeId::INVALID,
        });
        let throws_ty = arena.alloc_type(AstType::Simple {
            name: SharedString::new("Exception"),
            type_args: vec![],
            ty: exception_ty_id,
        });
        let empty_block = arena.alloc_stmt(rajac_ast::Stmt::Block(vec![]));

        let method = Method {
            name: Ident::new(SharedString::new("g")),
            params: vec![],
            return_ty: void_ty,
            body: Some(empty_block),
            throws: vec![throws_ty],
            modifiers: Modifiers(Modifiers::PUBLIC),
        };

        let member_id = arena.alloc_class_member(ClassMember::Method(method));
        let class_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Class,
            name: Ident::new(SharedString::new("Foo")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![],
            members: vec![member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });
        ast.classes.push(class_id);

        let mut class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        let class_file = class_files.pop().unwrap();
        class_file.verify()?;

        let method = class_file
            .methods
            .iter()
            .find(|method| {
                class_file
                    .constant_pool
                    .try_get_utf8(method.name_index)
                    .ok()
                    == Some("g")
            })
            .expect("method 'g' should be present");

        let exceptions = method
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                Attribute::Exceptions {
                    exception_indexes, ..
                } => Some(exception_indexes),
                _ => None,
            })
            .expect("Exceptions attribute should be present");

        assert_eq!(exceptions.len(), 1);
        assert_eq!(
            class_file.constant_pool.try_get_class(exceptions[0])?,
            "java/lang/Exception"
        );
        Ok(())
    }

    #[test]
    fn emits_exceptions_attribute_for_constructors() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));
        let mut symbol_table = SymbolTable::new();

        let exception_ty_id = symbol_table.add_class(
            "java.lang",
            "Exception",
            rajac_types::Type::class(
                rajac_types::ClassType::new(SharedString::new("Exception"))
                    .with_package(SharedString::new("java.lang")),
            ),
            rajac_symbols::SymbolKind::Class,
        );
        let type_arena = symbol_table.type_arena().clone();
        let throws_ty = arena.alloc_type(AstType::Simple {
            name: SharedString::new("Exception"),
            type_args: vec![],
            ty: exception_ty_id,
        });
        let body = arena.alloc_stmt(rajac_ast::Stmt::Block(vec![]));
        let constructor = AstConstructor {
            name: Ident::new(SharedString::new("Foo")),
            params: vec![],
            body: Some(body),
            throws: vec![throws_ty],
            modifiers: Modifiers(Modifiers::PUBLIC),
        };
        let ctor_member_id = arena.alloc_class_member(ClassMember::Constructor(constructor));

        let class_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Class,
            name: Ident::new(SharedString::new("Foo")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            enum_entries: vec![],
            members: vec![ctor_member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });
        ast.classes.push(class_id);

        let mut class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        let class_file = class_files.pop().unwrap();
        class_file.verify()?;

        let constructor = class_file
            .methods
            .iter()
            .find(|method| {
                class_file
                    .constant_pool
                    .try_get_utf8(method.name_index)
                    .ok()
                    == Some("<init>")
            })
            .expect("constructor should be present");

        let exceptions = constructor
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                Attribute::Exceptions {
                    exception_indexes, ..
                } => Some(exception_indexes),
                _ => None,
            })
            .expect("Exceptions attribute should be present");

        assert_eq!(exceptions.len(), 1);
        assert_eq!(
            class_file.constant_pool.try_get_class(exceptions[0])?,
            "java/lang/Exception"
        );

        Ok(())
    }
}
