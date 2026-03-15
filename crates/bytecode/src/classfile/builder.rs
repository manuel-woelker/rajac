use super::{
    ClassfileGenerationContext, NestedClassInfo, build_inner_classes_attribute, class_access_flags,
    collect_nested_class_infos, constructor_from_ast, field_from_ast, method_from_ast,
    type_to_internal_class_name,
};
use rajac_ast::{AstArena, ClassDeclId, ClassKind, ClassMember, Constructor as AstConstructor};
use rajac_base::result::{RajacResult, ResultExt};
use rajac_base::shared_string::SharedString;
use ristretto_classfile::{ClassFile, ConstantPool, JAVA_21};

pub(crate) fn emit_classfiles_for_class(
    arena: &AstArena,
    class_id: ClassDeclId,
    this_internal_name: SharedString,
    outer_internal_name: Option<SharedString>,
    class_files: &mut Vec<ClassFile>,
    generation_context: &mut ClassfileGenerationContext<'_>,
) -> RajacResult<()> {
    let class = arena.class_decl(class_id);
    let nested_classes = collect_nested_class_infos(arena, class, &this_internal_name);

    let class_file = classfile_from_class_decl_with_context(
        arena,
        class_id,
        &this_internal_name,
        outer_internal_name.as_deref(),
        &nested_classes,
        generation_context,
    )
    .with_context(|| format!("failed to build classfile for class '{}'", class.name))?;
    class_files.push(class_file);

    for nested in nested_classes {
        let nested_decl = arena.class_decl(nested.class_id);
        emit_classfiles_for_class(
            arena,
            nested.class_id,
            nested.internal_name,
            Some(this_internal_name.clone()),
            class_files,
            generation_context,
        )
        .with_context(|| {
            format!(
                "failed to generate bytecode for nested class '{}' inside '{}'",
                nested_decl.name, class.name
            )
        })?;
    }

    Ok(())
}

pub(crate) fn classfile_from_class_decl_with_context(
    arena: &AstArena,
    class_id: ClassDeclId,
    this_internal_name: &str,
    outer_internal_name: Option<&str>,
    nested_classes: &[NestedClassInfo],
    generation_context: &mut ClassfileGenerationContext<'_>,
) -> RajacResult<ClassFile> {
    let class = arena.class_decl(class_id);

    let mut constant_pool = ConstantPool::default();

    let this_class = constant_pool
        .add_class(this_internal_name)
        .with_context(|| {
            format!("failed to add class name '{this_internal_name}' to constant pool")
        })?;

    let super_internal_name = match class.extends {
        Some(type_id) => {
            type_to_internal_class_name(arena, type_id, generation_context.type_arena)?
        }
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
                if let Some(field_info) = field_from_ast(
                    arena,
                    &mut constant_pool,
                    field,
                    generation_context.type_arena,
                )? {
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
                    generation_context,
                )? {
                    methods.push(method_info);
                }
            }
            ClassMember::Constructor(constructor) => {
                has_constructor = true;
                methods.push(constructor_from_ast(
                    arena,
                    &mut constant_pool,
                    constructor,
                    &class.modifiers,
                    &super_internal_name,
                    generation_context,
                )?);
            }
            ClassMember::StaticBlock(_)
            | ClassMember::NestedClass(_)
            | ClassMember::NestedInterface(_)
            | ClassMember::NestedEnum(_)
            | ClassMember::NestedRecord(_)
            | ClassMember::NestedAnnotation(_) => {}
        }
    }

    if !has_constructor && matches!(class.kind, ClassKind::Class) {
        methods.push(constructor_from_ast(
            arena,
            &mut constant_pool,
            &AstConstructor {
                name: class.name.clone(),
                params: vec![],
                body: None,
                throws: vec![],
                modifiers: class.modifiers.clone(),
            },
            &class.modifiers,
            &super_internal_name,
            generation_context,
        )?);
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
