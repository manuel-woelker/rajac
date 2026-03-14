use crate::bytecode::CodeGenerator;
use rajac_ast::{
    Ast, AstArena, AstType, ClassDecl, ClassDeclId, ClassKind, ClassMember, Field as AstField,
    Method as AstMethod, Modifiers, PrimitiveType,
};
use rajac_base::result::{RajacResult, ResultExt};
use rajac_base::shared_string::SharedString;
use rajac_symbols::SymbolTable;
use rajac_types::Ident;
use ristretto_classfile::attributes::{Attribute, InnerClass, NestedClassAccessFlags};
use ristretto_classfile::{
    ClassAccessFlags, ClassFile, ConstantPool, Field, FieldAccessFlags, FieldType, JAVA_21, Method,
    MethodAccessFlags,
};

#[derive(Clone, Debug)]
struct NestedClassInfo {
    class_id: ClassDeclId,
    internal_name: SharedString,
    simple_name: SharedString,
    modifiers: Modifiers,
    kind: ClassKind,
}

pub fn generate_classfiles(
    ast: &Ast,
    arena: &AstArena,
    type_arena: &rajac_types::TypeArena,
    symbol_table: &SymbolTable,
) -> RajacResult<Vec<ClassFile>> {
    let mut class_files = Vec::new();
    for class_id in &ast.classes {
        let class = arena.class_decl(*class_id);
        let internal_name = internal_class_name(ast, &class.name, symbol_table);
        emit_classfiles_for_class(
            arena,
            *class_id,
            internal_name.into(),
            None,
            &mut class_files,
            type_arena,
            symbol_table,
        )?;
    }
    Ok(class_files)
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
    classfile_from_class_decl_with_context(
        arena,
        class_id,
        &this_internal_name,
        None,
        &[],
        type_arena,
        symbol_table,
    )
}

fn emit_classfiles_for_class(
    arena: &AstArena,
    class_id: ClassDeclId,
    this_internal_name: SharedString,
    outer_internal_name: Option<SharedString>,
    class_files: &mut Vec<ClassFile>,
    type_arena: &rajac_types::TypeArena,
    _symbol_table: &SymbolTable,
) -> RajacResult<()> {
    let class = arena.class_decl(class_id);
    let nested_classes = collect_nested_class_infos(arena, class, &this_internal_name);

    let class_file = classfile_from_class_decl_with_context(
        arena,
        class_id,
        &this_internal_name,
        outer_internal_name.as_deref(),
        &nested_classes,
        type_arena,
        _symbol_table,
    )?;
    class_files.push(class_file);

    for nested in nested_classes {
        emit_classfiles_for_class(
            arena,
            nested.class_id,
            nested.internal_name,
            Some(this_internal_name.clone()),
            class_files,
            type_arena,
            _symbol_table,
        )?;
    }

    Ok(())
}

fn classfile_from_class_decl_with_context(
    arena: &AstArena,
    class_id: ClassDeclId,
    this_internal_name: &str,
    outer_internal_name: Option<&str>,
    nested_classes: &[NestedClassInfo],
    type_arena: &rajac_types::TypeArena,
    symbol_table: &SymbolTable,
) -> RajacResult<ClassFile> {
    let class = arena.class_decl(class_id);

    let mut constant_pool = ConstantPool::default();

    let this_class = constant_pool
        .add_class(this_internal_name)
        .with_context(|| {
            format!("failed to add class name '{this_internal_name}' to constant pool")
        })?;

    let super_internal_name = match class.extends {
        Some(type_id) => type_to_internal_class_name(arena, type_id, type_arena)?,
        None => "java/lang/Object".to_string(),
    };

    let super_class = constant_pool
        .add_class(&super_internal_name)
        .with_context(|| {
            format!("failed to add super class name '{super_internal_name}' to constant pool")
        })?;

    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut has_constructor = false;

    for member_id in &class.members {
        let member = arena.class_member(*member_id);
        match member {
            ClassMember::Field(field) => {
                if let Some(field_info) =
                    field_from_ast(arena, &mut constant_pool, field, type_arena)?
                {
                    fields.push(field_info);
                }
            }
            ClassMember::Method(method) => {
                if let Some(method_info) = method_from_ast(
                    arena,
                    &mut constant_pool,
                    class.kind.clone(),
                    &class.modifiers,
                    method,
                    type_arena,
                    symbol_table,
                )? {
                    methods.push(method_info);
                }
            }
            ClassMember::Constructor(_) => {
                has_constructor = true;
                // TODO: Process actual constructors
            }
            ClassMember::StaticBlock(_)
            | ClassMember::NestedClass(_)
            | ClassMember::NestedInterface(_)
            | ClassMember::NestedEnum(_)
            | ClassMember::NestedRecord(_)
            | ClassMember::NestedAnnotation(_) => {
                // Omitted for now.
            }
        }
    }

    // Add default constructor if no constructors are defined and this is a class (not an interface)
    if !has_constructor
        && matches!(class.kind, ClassKind::Class)
        && let Some(default_constructor) =
            create_default_constructor(&mut constant_pool, &class.modifiers, &super_internal_name)?
    {
        methods.push(default_constructor);
    }

    let mut attributes = Vec::new();
    if let Some(inner_classes) = build_inner_classes_attribute(
        &mut constant_pool,
        this_class,
        outer_internal_name,
        class,
        nested_classes,
    )? {
        attributes.push(inner_classes);
    }

    let access_flags = class_access_flags(class.kind.clone(), &class.modifiers);

    Ok(ClassFile {
        version: JAVA_21,
        access_flags,
        constant_pool,
        this_class,
        super_class,
        fields,
        methods,
        attributes,
        ..Default::default()
    })
}

fn collect_nested_class_infos(
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

fn internal_class_name(ast: &Ast, class_name: &Ident, symbol_table: &SymbolTable) -> String {
    // Try to find the class in the symbol table first
    let class_name_str = class_name.as_str();

    // Check current package first
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

    // Check java.lang (implicitly imported)
    if let Some(pkg_table) = symbol_table.get_package("java.lang")
        && pkg_table.contains(class_name_str)
    {
        return format!("java/lang/{}", class_name_str);
    }

    // Fallback to package-based naming
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

fn build_inner_classes_attribute(
    constant_pool: &mut ConstantPool,
    this_class: u16,
    outer_internal_name: Option<&str>,
    class: &ClassDecl,
    nested_classes: &[NestedClassInfo],
) -> RajacResult<Option<Attribute>> {
    if outer_internal_name.is_none() && nested_classes.is_empty() {
        return Ok(None);
    }

    let mut classes = Vec::new();

    if let Some(outer_internal_name) = outer_internal_name {
        let outer_class = constant_pool
            .add_class(outer_internal_name)
            .with_context(|| {
                format!("failed to add outer class '{outer_internal_name}' to constant pool")
            })?;
        let name_index = constant_pool.add_utf8(class.name.as_str())?;
        classes.push(InnerClass {
            class_info_index: this_class,
            outer_class_info_index: outer_class,
            name_index,
            access_flags: nested_class_access_flags(class.kind.clone(), &class.modifiers),
        });
    }

    for nested in nested_classes {
        let class_info_index = constant_pool
            .add_class(&nested.internal_name)
            .with_context(|| {
                format!(
                    "failed to add nested class '{}' to constant pool",
                    nested.internal_name
                )
            })?;
        let name_index = constant_pool.add_utf8(&nested.simple_name)?;
        classes.push(InnerClass {
            class_info_index,
            outer_class_info_index: this_class,
            name_index,
            access_flags: nested_class_access_flags(nested.kind.clone(), &nested.modifiers),
        });
    }

    let name_index = constant_pool.add_utf8("InnerClasses")?;
    Ok(Some(Attribute::InnerClasses {
        name_index,
        classes,
    }))
}

fn class_access_flags(kind: ClassKind, modifiers: &Modifiers) -> ClassAccessFlags {
    let mut flags = ClassAccessFlags::empty();

    if has_modifier(modifiers, Modifiers::PUBLIC) {
        flags |= ClassAccessFlags::PUBLIC;
    }

    if has_modifier(modifiers, Modifiers::FINAL) {
        flags |= ClassAccessFlags::FINAL;
    }

    if has_modifier(modifiers, Modifiers::ABSTRACT) {
        flags |= ClassAccessFlags::ABSTRACT;
    }

    match kind {
        ClassKind::Interface => {
            flags |= ClassAccessFlags::INTERFACE;
            flags |= ClassAccessFlags::ABSTRACT;
        }
        ClassKind::Enum => {
            flags |= ClassAccessFlags::ENUM;
        }
        ClassKind::Annotation => {
            flags |= ClassAccessFlags::ANNOTATION;
            flags |= ClassAccessFlags::INTERFACE;
            flags |= ClassAccessFlags::ABSTRACT;
        }
        ClassKind::Record => {
            // No dedicated access flag; marker attribute omitted for now.
        }
        ClassKind::Class => {}
    }

    flags
}

fn nested_class_access_flags(kind: ClassKind, modifiers: &Modifiers) -> NestedClassAccessFlags {
    let mut flags = NestedClassAccessFlags::empty();

    if has_modifier(modifiers, Modifiers::PUBLIC) {
        flags |= NestedClassAccessFlags::PUBLIC;
    }
    if has_modifier(modifiers, Modifiers::PRIVATE) {
        flags |= NestedClassAccessFlags::PRIVATE;
    }
    if has_modifier(modifiers, Modifiers::PROTECTED) {
        flags |= NestedClassAccessFlags::PROTECTED;
    }
    if has_modifier(modifiers, Modifiers::STATIC) {
        flags |= NestedClassAccessFlags::STATIC;
    }
    if has_modifier(modifiers, Modifiers::FINAL) {
        flags |= NestedClassAccessFlags::FINAL;
    }
    if has_modifier(modifiers, Modifiers::ABSTRACT) {
        flags |= NestedClassAccessFlags::ABSTRACT;
    }
    if has_modifier(modifiers, Modifiers::SYNTHETIC) {
        flags |= NestedClassAccessFlags::SYNTHETIC;
    }

    match kind {
        ClassKind::Interface => {
            flags |= NestedClassAccessFlags::INTERFACE;
            flags |= NestedClassAccessFlags::ABSTRACT;
        }
        ClassKind::Enum => {
            flags |= NestedClassAccessFlags::ENUM;
        }
        ClassKind::Annotation => {
            flags |= NestedClassAccessFlags::ANNOTATION;
            flags |= NestedClassAccessFlags::INTERFACE;
            flags |= NestedClassAccessFlags::ABSTRACT;
        }
        ClassKind::Record | ClassKind::Class => {}
    }

    flags
}

fn has_modifier(modifiers: &Modifiers, mask: u32) -> bool {
    modifiers.0 & mask != 0
}

fn field_from_ast(
    arena: &AstArena,
    constant_pool: &mut ConstantPool,
    field: &AstField,
    type_arena: &rajac_types::TypeArena,
) -> RajacResult<Option<Field>> {
    let is_static = field.modifiers.0 & Modifiers::STATIC != 0;
    let has_initializer = field.initializer.is_some();

    if has_initializer && !is_static {
        return Ok(None);
    }

    let name_index = constant_pool.add_utf8(field.name.as_str())?;
    let descriptor = type_to_descriptor(arena, field.ty, type_arena)?;
    let descriptor_index = constant_pool.add_utf8(&descriptor)?;
    let field_type =
        FieldType::parse(&descriptor).context("failed to parse field descriptor for classfile")?;

    let access_flags = field_access_flags(&field.modifiers);

    Ok(Some(Field {
        access_flags,
        name_index,
        descriptor_index,
        field_type,
        attributes: vec![],
    }))
}

fn field_access_flags(modifiers: &Modifiers) -> FieldAccessFlags {
    let mut flags = FieldAccessFlags::empty();

    if has_modifier(modifiers, Modifiers::PUBLIC) {
        flags |= FieldAccessFlags::PUBLIC;
    }
    if has_modifier(modifiers, Modifiers::PRIVATE) {
        flags |= FieldAccessFlags::PRIVATE;
    }
    if has_modifier(modifiers, Modifiers::PROTECTED) {
        flags |= FieldAccessFlags::PROTECTED;
    }
    if has_modifier(modifiers, Modifiers::STATIC) {
        flags |= FieldAccessFlags::STATIC;
    }
    if has_modifier(modifiers, Modifiers::FINAL) {
        flags |= FieldAccessFlags::FINAL;
    }

    flags
}

fn method_from_ast(
    arena: &AstArena,
    constant_pool: &mut ConstantPool,
    class_kind: ClassKind,
    class_modifiers: &Modifiers,
    method: &AstMethod,
    type_arena: &rajac_types::TypeArena,
    symbol_table: &SymbolTable,
) -> RajacResult<Option<Method>> {
    let name_index = constant_pool.add_utf8(method.name.as_str())?;
    let descriptor = method_to_descriptor(arena, method, type_arena)?;
    let descriptor_index = constant_pool.add_utf8(&descriptor)?;

    let mut access_flags = method_access_flags(&method.modifiers);

    let attributes = if let Some(body_id) = method.body {
        // Generate bytecode for method with body
        generate_method_bytecode(
            arena,
            type_arena,
            symbol_table,
            constant_pool,
            method,
            body_id,
        )?
    } else {
        // Handle abstract methods
        let method_can_be_bodyless = match class_kind {
            ClassKind::Interface | ClassKind::Annotation => true,
            ClassKind::Class | ClassKind::Enum | ClassKind::Record => {
                has_modifier(&method.modifiers, Modifiers::ABSTRACT)
                    || has_modifier(class_modifiers, Modifiers::ABSTRACT)
            }
        };

        if !method_can_be_bodyless {
            return Ok(None);
        }

        access_flags |= MethodAccessFlags::ABSTRACT;
        vec![]
    };

    Ok(Some(Method {
        access_flags,
        name_index,
        descriptor_index,
        attributes,
    }))
}

fn generate_method_bytecode(
    arena: &AstArena,
    type_arena: &rajac_types::TypeArena,
    symbol_table: &SymbolTable,
    constant_pool: &mut ConstantPool,
    method: &AstMethod,
    body_id: rajac_ast::StmtId,
) -> RajacResult<Vec<ristretto_classfile::attributes::Attribute>> {
    let is_static = method.modifiers.0 & Modifiers::STATIC != 0;

    let mut code_gen = CodeGenerator::new(arena, type_arena, symbol_table, constant_pool);
    let (instructions, max_stack, max_locals) =
        code_gen.generate_method_body(is_static, &method.params, body_id)?;

    let code_name = constant_pool.add_utf8("Code")?;

    Ok(vec![ristretto_classfile::attributes::Attribute::Code {
        name_index: code_name,
        max_stack,
        max_locals,
        code: instructions,
        exception_table: vec![],
        attributes: vec![],
    }])
}

fn method_access_flags(modifiers: &Modifiers) -> MethodAccessFlags {
    let mut flags = MethodAccessFlags::empty();

    if has_modifier(modifiers, Modifiers::PUBLIC) {
        flags |= MethodAccessFlags::PUBLIC;
    }
    if has_modifier(modifiers, Modifiers::PRIVATE) {
        flags |= MethodAccessFlags::PRIVATE;
    }
    if has_modifier(modifiers, Modifiers::PROTECTED) {
        flags |= MethodAccessFlags::PROTECTED;
    }
    if has_modifier(modifiers, Modifiers::STATIC) {
        flags |= MethodAccessFlags::STATIC;
    }
    if has_modifier(modifiers, Modifiers::FINAL) {
        flags |= MethodAccessFlags::FINAL;
    }
    if has_modifier(modifiers, Modifiers::ABSTRACT) {
        flags |= MethodAccessFlags::ABSTRACT;
    }

    flags
}

fn method_to_descriptor(
    arena: &AstArena,
    method: &AstMethod,
    type_arena: &rajac_types::TypeArena,
) -> RajacResult<String> {
    let mut s = String::new();
    s.push('(');
    for param_id in &method.params {
        let param = arena.param(*param_id);
        s.push_str(&type_to_descriptor(arena, param.ty, type_arena)?);
    }
    s.push(')');
    s.push_str(&type_to_descriptor(arena, method.return_ty, type_arena)?);
    Ok(s)
}

fn type_to_descriptor(
    arena: &AstArena,
    type_id: rajac_ast::AstTypeId,
    type_arena: &rajac_types::TypeArena,
) -> RajacResult<String> {
    let ty = arena.ty(type_id);
    Ok(match ty {
        AstType::Error => "Ljava/lang/Object;".to_string(),
        AstType::Primitive { kind: p, ty: _ } => match p {
            PrimitiveType::Boolean => "Z".to_string(),
            PrimitiveType::Byte => "B".to_string(),
            PrimitiveType::Char => "C".to_string(),
            PrimitiveType::Short => "S".to_string(),
            PrimitiveType::Int => "I".to_string(),
            PrimitiveType::Long => "J".to_string(),
            PrimitiveType::Float => "F".to_string(),
            PrimitiveType::Double => "D".to_string(),
            PrimitiveType::Void => "V".to_string(),
        },
        AstType::Simple {
            name,
            ty: type_id,
            type_args: _,
        } => {
            // Use the TypeId to look up the fully qualified name from TypeArena
            if *type_id != rajac_types::TypeId::INVALID {
                let type_entry = type_arena.get(*type_id);
                if let rajac_types::Type::Class(class_type) = type_entry {
                    return Ok(format!("L{};", class_type.internal_name()));
                }
            }
            // Fallback to simple name if TypeId is invalid or not a class type
            format!("L{};", name)
        }
        AstType::Array {
            element_type,
            dimensions,
            ty: _,
        } => {
            let mut result = String::new();
            for _ in 0..*dimensions {
                result.push('[');
            }
            result.push_str(&type_to_descriptor(arena, *element_type, type_arena)?);
            result
        }
        AstType::Wildcard { .. } => "Ljava/lang/Object;".to_string(),
    })
}

fn type_to_internal_class_name(
    arena: &AstArena,
    type_id: rajac_ast::AstTypeId,
    type_arena: &rajac_types::TypeArena,
) -> RajacResult<String> {
    let ty = arena.ty(type_id);
    Ok(match ty {
        AstType::Simple {
            name,
            ty: type_id,
            type_args: _,
        } => {
            // Use the TypeId to look up the fully qualified name from TypeArena
            if *type_id != rajac_types::TypeId::INVALID {
                let type_entry = type_arena.get(*type_id);
                if let rajac_types::Type::Class(class_type) = type_entry {
                    return Ok(class_type.internal_name());
                }
            }
            // Fallback to simple name if TypeId is invalid or not a class type
            name.as_str().to_string()
        }
        AstType::Array { element_type, .. } => {
            type_to_internal_class_name(arena, *element_type, type_arena)?
        }
        _ => "java/lang/Object".to_string(),
    })
}

fn create_default_constructor(
    constant_pool: &mut ConstantPool,
    modifiers: &Modifiers,
    super_internal_name: &str,
) -> RajacResult<Option<Method>> {
    let name_index = constant_pool.add_utf8("<init>")?;
    let descriptor_index = constant_pool.add_utf8("()V")?;

    let mut access_flags = MethodAccessFlags::default();
    if modifiers.is_public() {
        access_flags |= MethodAccessFlags::PUBLIC;
    }
    if modifiers.is_protected() {
        access_flags |= MethodAccessFlags::PROTECTED;
    }

    // Create Code attribute for default constructor
    let code_name = constant_pool.add_utf8("Code")?;

    // Add superclass class reference for invokespecial
    let super_class = constant_pool.add_class(super_internal_name)?;
    let super_init = constant_pool.add_method_ref(super_class, "<init>", "()V")?;

    // Generate bytecode: aload_0, invokespecial Object.<init>, return
    let code = vec![
        ristretto_classfile::attributes::Instruction::Aload_0,
        ristretto_classfile::attributes::Instruction::Invokespecial(super_init),
        ristretto_classfile::attributes::Instruction::Return,
    ];

    let code_attribute = ristretto_classfile::attributes::Attribute::Code {
        name_index: code_name,
        max_stack: 1,  // Need stack for aload_0 and invokespecial
        max_locals: 1, // Need local variable for 'this'
        code,
        exception_table: vec![],
        attributes: vec![],
    };

    Ok(Some(Method {
        access_flags,
        name_index,
        descriptor_index,
        attributes: vec![code_attribute],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rajac_ast::{
        Ast, AstArena, AstType, ClassDecl, ClassKind, ClassMember, Method, Modifiers, PackageDecl,
        Param, PrimitiveType, QualifiedName,
    };
    use rajac_base::shared_string::SharedString;
    use rajac_types::Ident;

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
            kind: ClassKind::Class, // Changed from Interface to Class
            name: Ident::new(SharedString::new("Foo")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            members: vec![member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });

        ast.classes.push(class_id);

        let mut class_files = generate_classfiles(&ast, &arena, &type_arena, &symbol_table)?;
        assert_eq!(class_files.len(), 1);

        let class_file = class_files.pop().unwrap();
        class_file.verify()?;

        // Now methods with bodies should be processed and have Code attributes
        // We should have 2 methods: the method with body + default constructor
        assert_eq!(class_file.methods.len(), 2);

        // Find our method with body
        let method_with_body = class_file
            .methods
            .iter()
            .find(|m| class_file.constant_pool.try_get_utf8(m.name_index).ok() == Some("g"))
            .expect("method 'g' should be present");

        assert!(!method_with_body.attributes.is_empty());

        // Check that it has a Code attribute
        let has_code = method_with_body.attributes.iter().any(|attr| {
            matches!(
                attr,
                ristretto_classfile::attributes::Attribute::Code { .. }
            )
        });
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
}
