use super::{
    ClassfileGenerationContext, constructor_to_descriptor, exceptions_attribute_from_ast_types,
    has_modifier, method_to_descriptor, type_to_descriptor,
};
use crate::bytecode::CodeGenerator;
use rajac_ast::{
    AstArena, ClassKind, Constructor as AstConstructor, Field as AstField, Method as AstMethod,
    Modifiers,
};
use rajac_base::result::{RajacResult, ResultExt};
use ristretto_classfile::attributes::Attribute;
use ristretto_classfile::{
    ConstantPool, Field, FieldAccessFlags, FieldType, Method, MethodAccessFlags,
};

pub(crate) fn field_from_ast(
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

    Ok(Some(Field {
        access_flags: field_access_flags(&field.modifiers),
        name_index,
        descriptor_index,
        field_type,
        attributes: vec![],
    }))
}

pub(crate) fn field_access_flags(modifiers: &Modifiers) -> FieldAccessFlags {
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
    if has_modifier(modifiers, Modifiers::SYNTHETIC) {
        flags |= FieldAccessFlags::SYNTHETIC;
    }

    flags
}

pub(crate) fn method_from_ast(
    arena: &AstArena,
    constant_pool: &mut ConstantPool,
    this_internal_name: &str,
    class_kind: ClassKind,
    class_modifiers: &Modifiers,
    method: &AstMethod,
    generation_context: &mut ClassfileGenerationContext<'_>,
) -> RajacResult<Option<Method>> {
    let name_index = constant_pool.add_utf8(method.name.as_str())?;
    let descriptor = method_to_descriptor(arena, method, generation_context.type_arena)?;
    let descriptor_index = constant_pool.add_utf8(&descriptor)?;

    let mut access_flags = method_access_flags(&method.modifiers);

    let mut attributes = if let Some(body_id) = method.body {
        generate_method_bytecode(
            arena,
            constant_pool,
            this_internal_name,
            method,
            body_id,
            generation_context,
        )?
    } else {
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

    if let Some(exceptions_attribute) = exceptions_attribute_from_ast_types(
        constant_pool,
        &method.throws,
        arena,
        generation_context.type_arena,
    )? {
        attributes.push(exceptions_attribute);
    }

    Ok(Some(Method {
        access_flags,
        name_index,
        descriptor_index,
        attributes,
    }))
}

pub(crate) fn generate_method_bytecode(
    arena: &AstArena,
    constant_pool: &mut ConstantPool,
    _this_internal_name: &str,
    method: &AstMethod,
    body_id: rajac_ast::StmtId,
    generation_context: &mut ClassfileGenerationContext<'_>,
) -> RajacResult<Vec<Attribute>> {
    let is_static = method.modifiers.0 & Modifiers::STATIC != 0;

    let mut code_gen = CodeGenerator::new(
        arena,
        generation_context.type_arena,
        generation_context.symbol_table,
        constant_pool,
    );
    let (instructions, max_stack, max_locals) =
        code_gen.generate_method_body(is_static, &method.params, body_id)?;
    generation_context
        .unsupported_features
        .extend(code_gen.take_unsupported_features());

    let code_name = constant_pool.add_utf8("Code")?;

    Ok(vec![Attribute::Code {
        name_index: code_name,
        max_stack,
        max_locals,
        code: instructions,
        exception_table: vec![],
        attributes: vec![],
    }])
}

pub(crate) fn method_access_flags(modifiers: &Modifiers) -> MethodAccessFlags {
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
    if has_modifier(modifiers, Modifiers::SYNTHETIC) {
        flags |= MethodAccessFlags::SYNTHETIC;
    }

    flags
}

pub(crate) fn constructor_from_ast(
    arena: &AstArena,
    constant_pool: &mut ConstantPool,
    _this_internal_name: &str,
    constructor: &AstConstructor,
    class_modifiers: &Modifiers,
    super_internal_name: &str,
    generation_context: &mut ClassfileGenerationContext<'_>,
) -> RajacResult<Method> {
    let name_index = constant_pool.add_utf8("<init>")?;
    let descriptor = constructor_to_descriptor(arena, constructor, generation_context.type_arena)?;
    let descriptor_index = constant_pool.add_utf8(&descriptor)?;

    let mut access_flags = MethodAccessFlags::default();
    let has_explicit_visibility = constructor.modifiers.is_public()
        || constructor.modifiers.is_protected()
        || constructor.modifiers.is_private();
    if constructor.modifiers.is_public()
        || (!has_explicit_visibility && class_modifiers.is_public())
    {
        access_flags |= MethodAccessFlags::PUBLIC;
    }
    if constructor.modifiers.is_protected() {
        access_flags |= MethodAccessFlags::PROTECTED;
    }
    if constructor.modifiers.is_private() {
        access_flags |= MethodAccessFlags::PRIVATE;
    }

    let mut code_gen = CodeGenerator::new(
        arena,
        generation_context.type_arena,
        generation_context.symbol_table,
        constant_pool,
    );
    let (instructions, max_stack, max_locals) = code_gen.generate_constructor_body(
        &constructor.params,
        constructor.body,
        super_internal_name,
    )?;
    generation_context
        .unsupported_features
        .extend(code_gen.take_unsupported_features());

    let code_name = constant_pool.add_utf8("Code")?;
    let code_attribute = Attribute::Code {
        name_index: code_name,
        max_stack,
        max_locals,
        code: instructions,
        exception_table: vec![],
        attributes: vec![],
    };

    let mut attributes = vec![code_attribute];
    if let Some(exceptions_attribute) = exceptions_attribute_from_ast_types(
        constant_pool,
        &constructor.throws,
        arena,
        generation_context.type_arena,
    )? {
        attributes.push(exceptions_attribute);
    }

    Ok(Method {
        access_flags,
        name_index,
        descriptor_index,
        attributes,
    })
}

pub(crate) fn enum_constructor_from_ast(
    arena: &AstArena,
    constant_pool: &mut ConstantPool,
    constructor: &AstConstructor,
    class_modifiers: &Modifiers,
    generation_context: &mut ClassfileGenerationContext<'_>,
) -> RajacResult<Method> {
    let name_index = constant_pool.add_utf8("<init>")?;

    let mut descriptor = String::from("(Ljava/lang/String;I");
    for param_id in &constructor.params {
        let param = arena.param(*param_id);
        descriptor.push_str(&type_to_descriptor(
            arena,
            param.ty,
            generation_context.type_arena,
        )?);
    }
    descriptor.push_str(")V");
    let descriptor_index = constant_pool.add_utf8(&descriptor)?;

    let mut access_flags = MethodAccessFlags::default();
    let has_explicit_visibility = constructor.modifiers.is_public()
        || constructor.modifiers.is_protected()
        || constructor.modifiers.is_private();
    if constructor.modifiers.is_public()
        || (!has_explicit_visibility && class_modifiers.is_public())
    {
        access_flags |= MethodAccessFlags::PUBLIC;
    }
    if constructor.modifiers.is_protected() {
        access_flags |= MethodAccessFlags::PROTECTED;
    }
    if constructor.modifiers.is_private() {
        access_flags |= MethodAccessFlags::PRIVATE;
    }

    let mut code_gen = CodeGenerator::new(
        arena,
        generation_context.type_arena,
        generation_context.symbol_table,
        constant_pool,
    );
    let (instructions, max_stack, max_locals) = code_gen.generate_enum_constructor_body(
        &constructor.params,
        constructor.body,
        "java/lang/Enum",
    )?;
    generation_context
        .unsupported_features
        .extend(code_gen.take_unsupported_features());

    let code_name = constant_pool.add_utf8("Code")?;
    let code_attribute = Attribute::Code {
        name_index: code_name,
        max_stack,
        max_locals,
        code: instructions,
        exception_table: vec![],
        attributes: vec![],
    };

    let mut attributes = vec![code_attribute];
    if let Some(exceptions_attribute) = exceptions_attribute_from_ast_types(
        constant_pool,
        &constructor.throws,
        arena,
        generation_context.type_arena,
    )? {
        attributes.push(exceptions_attribute);
    }

    Ok(Method {
        access_flags,
        name_index,
        descriptor_index,
        attributes,
    })
}
