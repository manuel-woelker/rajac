use rajac_ast::{
    Ast, AstArena, ClassDeclId, ClassKind, ClassMember, Field as AstField, Ident,
    Method as AstMethod, Modifiers, Type,
};
use rajac_base::result::{RajacResult, ResultExt};
use ristretto_classfile::{
    ClassAccessFlags, ClassFile, ConstantPool, Field, FieldAccessFlags, FieldType, JAVA_21, Method,
    MethodAccessFlags,
};

pub fn generate_classfiles(ast: &Ast, arena: &AstArena) -> RajacResult<Vec<ClassFile>> {
    let mut class_files = Vec::with_capacity(ast.classes.len());
    for class_id in &ast.classes {
        class_files.push(classfile_from_class_decl(ast, arena, *class_id)?);
    }
    Ok(class_files)
}

pub fn classfile_from_class_decl(
    ast: &Ast,
    arena: &AstArena,
    class_id: ClassDeclId,
) -> RajacResult<ClassFile> {
    let class = arena.class_decl(class_id);

    let mut constant_pool = ConstantPool::default();

    let this_internal_name = internal_class_name(ast, &class.name);
    let this_class = constant_pool
        .add_class(&this_internal_name)
        .with_context(|| {
            format!("failed to add class name '{this_internal_name}' to constant pool")
        })?;

    let super_internal_name = match class.extends {
        Some(type_id) => type_to_internal_class_name(arena, type_id)?,
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
                if let Some(field_info) = field_from_ast(arena, &mut constant_pool, field)? {
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
            create_default_constructor(&mut constant_pool, &class.modifiers)?
    {
        methods.push(default_constructor);
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
        ..Default::default()
    })
}

fn internal_class_name(ast: &Ast, class_name: &Ident) -> String {
    match &ast.package {
        Some(pkg) => {
            let mut s = pkg.name.segments.join("/");
            if !s.is_empty() {
                s.push('/');
            }
            s.push_str(class_name.0.as_str());
            s
        }
        None => class_name.0.as_str().to_string(),
    }
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

fn has_modifier(modifiers: &Modifiers, mask: u32) -> bool {
    modifiers.0 & mask != 0
}

fn field_from_ast(
    arena: &AstArena,
    constant_pool: &mut ConstantPool,
    field: &AstField,
) -> RajacResult<Option<Field>> {
    let is_static = field.modifiers.0 & Modifiers::STATIC != 0;
    let has_initializer = field.initializer.is_some();

    if has_initializer && !is_static {
        return Ok(None);
    }

    let name_index = constant_pool.add_utf8(field.name.0.as_str())?;
    let descriptor = type_to_descriptor(arena, field.ty)?;
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
) -> RajacResult<Option<Method>> {
    if method.body.is_some() {
        return Ok(None);
    }

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

    let name_index = constant_pool.add_utf8(method.name.0.as_str())?;
    let descriptor = method_to_descriptor(arena, method)?;
    let descriptor_index = constant_pool.add_utf8(&descriptor)?;

    let mut access_flags = method_access_flags(&method.modifiers);
    if method.body.is_none() {
        access_flags |= MethodAccessFlags::ABSTRACT;
    }

    Ok(Some(Method {
        access_flags,
        name_index,
        descriptor_index,
        attributes: vec![],
    }))
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

fn method_to_descriptor(arena: &AstArena, method: &AstMethod) -> RajacResult<String> {
    let mut s = String::new();
    s.push('(');
    for param_id in &method.params {
        let param = arena.param(*param_id);
        s.push_str(&type_to_descriptor(arena, param.ty)?);
    }
    s.push(')');
    s.push_str(&type_to_descriptor(arena, method.return_ty)?);
    Ok(s)
}

fn type_to_descriptor(arena: &AstArena, type_id: rajac_ast::TypeId) -> RajacResult<String> {
    let ty = arena.ty(type_id);
    Ok(match ty {
        Type::Error | Type::NonCanonical => "Ljava/lang/Object;".to_string(),
        Type::Primitive(p) => match p {
            rajac_ast::PrimitiveType::Boolean => "Z".to_string(),
            rajac_ast::PrimitiveType::Byte => "B".to_string(),
            rajac_ast::PrimitiveType::Char => "C".to_string(),
            rajac_ast::PrimitiveType::Short => "S".to_string(),
            rajac_ast::PrimitiveType::Int => "I".to_string(),
            rajac_ast::PrimitiveType::Long => "J".to_string(),
            rajac_ast::PrimitiveType::Float => "F".to_string(),
            rajac_ast::PrimitiveType::Double => "D".to_string(),
            rajac_ast::PrimitiveType::Void => "V".to_string(),
        },
        Type::Class { name, .. } => format!("L{};", name.0.as_str().replace('.', "/")),
        Type::Array { ty } => format!("[{}", type_to_descriptor(arena, *ty)?),
        Type::TypeVariable { .. } | Type::Wildcard { .. } => "Ljava/lang/Object;".to_string(),
    })
}

fn type_to_internal_class_name(
    arena: &AstArena,
    type_id: rajac_ast::TypeId,
) -> RajacResult<String> {
    let ty = arena.ty(type_id);
    Ok(match ty {
        Type::Class { name, .. } => name.0.as_str().replace('.', "/"),
        _ => "java/lang/Object".to_string(),
    })
}

fn create_default_constructor(
    constant_pool: &mut ConstantPool,
    modifiers: &Modifiers,
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

    Ok(Some(Method {
        access_flags,
        name_index,
        descriptor_index,
        attributes: vec![],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rajac_ast::{Ast, AstArena, ClassDecl, ClassKind, Ident, Method, Modifiers, Param, Type};
    use rajac_base::shared_string::SharedString;

    #[test]
    fn generates_minimal_abstract_method_without_code_attribute() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));

        let void_ty = arena.alloc_type(Type::Primitive(rajac_ast::PrimitiveType::Void));
        let int_ty = arena.alloc_type(Type::Primitive(rajac_ast::PrimitiveType::Int));

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

        let mut class_files = generate_classfiles(&ast, &arena)?;
        assert_eq!(class_files.len(), 1);

        let class_file = class_files.pop().unwrap();
        class_file.verify()?;

        assert_eq!(class_file.methods.len(), 1);
        assert!(class_file.methods[0].attributes.is_empty());

        Ok(())
    }

    #[test]
    fn skips_methods_with_bodies_for_now() -> RajacResult<()> {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));

        let void_ty = arena.alloc_type(Type::Primitive(rajac_ast::PrimitiveType::Void));
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

        let mut class_files = generate_classfiles(&ast, &arena)?;
        assert_eq!(class_files.len(), 1);

        let class_file = class_files.pop().unwrap();
        class_file.verify()?;

        assert!(class_file.methods.is_empty());

        Ok(())
    }
}
