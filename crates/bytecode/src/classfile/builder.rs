use super::{
    ClassfileGenerationContext, NestedClassInfo, build_inner_classes_attribute, class_access_flags,
    collect_nested_class_infos, constructor_from_ast, enum_constructor_from_ast, field_from_ast,
    method_from_ast, type_to_descriptor, type_to_internal_class_name,
};
use rajac_ast::{
    AstArena, ClassDecl, ClassDeclId, ClassKind, ClassMember, Constructor as AstConstructor,
    EnumEntry, Modifiers,
};
use rajac_base::result::{RajacResult, ResultExt};
use rajac_base::shared_string::SharedString;
use ristretto_classfile::attributes::{Attribute, Instruction};
use ristretto_classfile::{
    ClassFile, ConstantPool, Field, FieldAccessFlags, FieldType, JAVA_21, Method, MethodAccessFlags,
};

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
        None if matches!(class.kind, ClassKind::Enum) => "java/lang/Enum".to_string(),
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

    if matches!(class.kind, ClassKind::Enum) {
        synthesize_enum_fields(&mut constant_pool, this_internal_name, class, &mut fields)?;
        methods.push(synthesize_enum_values_method(
            &mut constant_pool,
            this_internal_name,
        )?);
        methods.push(synthesize_enum_value_of_method(
            &mut constant_pool,
            this_internal_name,
        )?);
    }

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
                    this_internal_name,
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
                let method = if matches!(class.kind, ClassKind::Enum) {
                    enum_constructor_from_ast(
                        arena,
                        &mut constant_pool,
                        this_internal_name,
                        constructor,
                        &class.modifiers,
                        generation_context,
                    )?
                } else {
                    constructor_from_ast(
                        arena,
                        &mut constant_pool,
                        this_internal_name,
                        constructor,
                        &class.modifiers,
                        &super_internal_name,
                        generation_context,
                    )?
                };
                methods.push(method);
            }
            ClassMember::StaticBlock(_)
            | ClassMember::NestedClass(_)
            | ClassMember::NestedInterface(_)
            | ClassMember::NestedEnum(_)
            | ClassMember::NestedRecord(_)
            | ClassMember::NestedAnnotation(_) => {}
        }
    }

    if !has_constructor {
        match class.kind {
            ClassKind::Class => {
                methods.push(constructor_from_ast(
                    arena,
                    &mut constant_pool,
                    this_internal_name,
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
            ClassKind::Enum => {
                methods.push(enum_constructor_from_ast(
                    arena,
                    &mut constant_pool,
                    this_internal_name,
                    &AstConstructor {
                        name: class.name.clone(),
                        params: vec![],
                        body: None,
                        throws: vec![],
                        modifiers: Modifiers(Modifiers::PRIVATE),
                    },
                    &class.modifiers,
                    generation_context,
                )?);
            }
            _ => {}
        }
    }

    if matches!(class.kind, ClassKind::Enum) {
        methods.push(synthesize_enum_values_array_method(
            &mut constant_pool,
            this_internal_name,
            class,
        )?);
        methods.push(synthesize_enum_clinit(
            arena,
            &mut constant_pool,
            this_internal_name,
            class,
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
    if let Some(nest_attribute) = build_nest_attribute(
        &mut constant_pool,
        this_class,
        outer_internal_name,
        nested_classes,
    )? {
        attributes.push(nest_attribute);
    }
    if matches!(class.kind, ClassKind::Enum) {
        attributes.push(Attribute::Signature {
            name_index: constant_pool.add_utf8("Signature")?,
            signature_index: constant_pool
                .add_utf8(format!("Ljava/lang/Enum<L{this_internal_name};>;"))?,
        });
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

fn synthesize_enum_fields(
    constant_pool: &mut ConstantPool,
    this_internal_name: &str,
    class: &ClassDecl,
    fields: &mut Vec<Field>,
) -> RajacResult<()> {
    let enum_descriptor = format!("L{this_internal_name};");
    let enum_field_type =
        FieldType::parse(&enum_descriptor).context("failed to parse enum field descriptor")?;
    for entry in &class.enum_entries {
        let name_index = constant_pool.add_utf8(entry.name.as_str())?;
        let descriptor_index = constant_pool.add_utf8(&enum_descriptor)?;
        fields.push(Field {
            access_flags: FieldAccessFlags::PUBLIC
                | FieldAccessFlags::STATIC
                | FieldAccessFlags::FINAL
                | FieldAccessFlags::ENUM,
            name_index,
            descriptor_index,
            field_type: enum_field_type.clone(),
            attributes: vec![],
        });
    }

    let values_descriptor = format!("[L{this_internal_name};");
    let values_field_type =
        FieldType::parse(&values_descriptor).context("failed to parse $VALUES descriptor")?;
    fields.push(Field {
        access_flags: FieldAccessFlags::PRIVATE
            | FieldAccessFlags::STATIC
            | FieldAccessFlags::FINAL
            | FieldAccessFlags::SYNTHETIC,
        name_index: constant_pool.add_utf8("$VALUES")?,
        descriptor_index: constant_pool.add_utf8(&values_descriptor)?,
        field_type: values_field_type,
        attributes: vec![],
    });

    Ok(())
}

fn synthesize_enum_values_method(
    constant_pool: &mut ConstantPool,
    this_internal_name: &str,
) -> RajacResult<Method> {
    let array_descriptor = format!("[L{this_internal_name};");
    let this_class = constant_pool.add_class(this_internal_name)?;
    let array_class = constant_pool.add_class(&array_descriptor)?;
    let values_field = constant_pool.add_field_ref(this_class, "$VALUES", &array_descriptor)?;
    let clone_method =
        constant_pool.add_method_ref(array_class, "clone", "()Ljava/lang/Object;")?;
    let code_name = constant_pool.add_utf8("Code")?;

    Ok(Method {
        access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
        name_index: constant_pool.add_utf8("values")?,
        descriptor_index: constant_pool.add_utf8(format!("(){array_descriptor}"))?,
        attributes: vec![Attribute::Code {
            name_index: code_name,
            max_stack: 1,
            max_locals: 0,
            code: vec![
                Instruction::Getstatic(values_field),
                Instruction::Invokevirtual(clone_method),
                Instruction::Checkcast(array_class),
                Instruction::Areturn,
            ],
            exception_table: vec![],
            attributes: vec![],
        }],
    })
}

fn synthesize_enum_values_array_method(
    constant_pool: &mut ConstantPool,
    this_internal_name: &str,
    class: &ClassDecl,
) -> RajacResult<Method> {
    let this_class = constant_pool.add_class(this_internal_name)?;
    let array_descriptor = format!("[L{this_internal_name};");
    let code_name = constant_pool.add_utf8("Code")?;
    let mut code = Vec::new();
    push_small_int(&mut code, class.enum_entries.len() as i32);
    code.push(Instruction::Anewarray(this_class));
    for (ordinal, entry) in class.enum_entries.iter().enumerate() {
        code.push(Instruction::Dup);
        push_small_int(&mut code, ordinal as i32);
        let field_ref = constant_pool.add_field_ref(
            this_class,
            entry.name.as_str(),
            &format!("L{this_internal_name};"),
        )?;
        code.push(Instruction::Getstatic(field_ref));
        code.push(Instruction::Aastore);
    }
    code.push(Instruction::Areturn);

    Ok(Method {
        access_flags: MethodAccessFlags::PRIVATE
            | MethodAccessFlags::STATIC
            | MethodAccessFlags::SYNTHETIC,
        name_index: constant_pool.add_utf8("$values")?,
        descriptor_index: constant_pool.add_utf8(format!("(){array_descriptor}"))?,
        attributes: vec![Attribute::Code {
            name_index: code_name,
            max_stack: 4,
            max_locals: 0,
            code,
            exception_table: vec![],
            attributes: vec![],
        }],
    })
}

fn synthesize_enum_value_of_method(
    constant_pool: &mut ConstantPool,
    this_internal_name: &str,
) -> RajacResult<Method> {
    let this_class = constant_pool.add_class(this_internal_name)?;
    let enum_class = constant_pool.add_class("java/lang/Enum")?;
    let enum_value_of = constant_pool.add_method_ref(
        enum_class,
        "valueOf",
        "(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;",
    )?;
    let code_name = constant_pool.add_utf8("Code")?;

    Ok(Method {
        access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
        name_index: constant_pool.add_utf8("valueOf")?,
        descriptor_index: constant_pool
            .add_utf8(format!("(Ljava/lang/String;)L{this_internal_name};"))?,
        attributes: vec![Attribute::Code {
            name_index: code_name,
            max_stack: 2,
            max_locals: 1,
            code: vec![
                Instruction::Ldc_w(this_class),
                Instruction::Aload_0,
                Instruction::Invokestatic(enum_value_of),
                Instruction::Checkcast(this_class),
                Instruction::Areturn,
            ],
            exception_table: vec![],
            attributes: vec![],
        }],
    })
}

fn synthesize_enum_clinit(
    arena: &AstArena,
    constant_pool: &mut ConstantPool,
    this_internal_name: &str,
    class: &ClassDecl,
) -> RajacResult<Method> {
    let this_class = constant_pool.add_class(this_internal_name)?;
    let mut code = Vec::new();

    for (ordinal, entry) in class.enum_entries.iter().enumerate() {
        code.push(Instruction::New(this_class));
        code.push(Instruction::Dup);
        let string_index = constant_pool.add_string(entry.name.as_str())?;
        if string_index <= u8::MAX as u16 {
            code.push(Instruction::Ldc(string_index as u8));
        } else {
            code.push(Instruction::Ldc_w(string_index));
        }
        push_small_int(&mut code, ordinal as i32);
        for arg in &entry.args {
            emit_enum_literal_argument(code.as_mut(), constant_pool, arena, *arg)?;
        }
        let descriptor = enum_constructor_descriptor_for_entry(arena, class, entry)?;
        let ctor_ref = constant_pool.add_method_ref(this_class, "<init>", &descriptor)?;
        code.push(Instruction::Invokespecial(ctor_ref));
        let field_ref = constant_pool.add_field_ref(
            this_class,
            entry.name.as_str(),
            &format!("L{this_internal_name};"),
        )?;
        code.push(Instruction::Putstatic(field_ref));
    }

    let values_ref = constant_pool.add_method_ref(
        this_class,
        "$values",
        &format!("()[L{this_internal_name};"),
    )?;
    code.push(Instruction::Invokestatic(values_ref));
    let values_field_ref =
        constant_pool.add_field_ref(this_class, "$VALUES", &format!("[L{this_internal_name};"))?;
    code.push(Instruction::Putstatic(values_field_ref));
    code.push(Instruction::Return);

    Ok(Method {
        access_flags: MethodAccessFlags::STATIC,
        name_index: constant_pool.add_utf8("<clinit>")?,
        descriptor_index: constant_pool.add_utf8("()V")?,
        attributes: vec![Attribute::Code {
            name_index: constant_pool.add_utf8("Code")?,
            max_stack: enum_clinit_max_stack(class),
            max_locals: 0,
            code,
            exception_table: vec![],
            attributes: vec![],
        }],
    })
}

fn enum_constructor_descriptor_for_entry(
    arena: &AstArena,
    class: &ClassDecl,
    entry: &EnumEntry,
) -> RajacResult<String> {
    let matching_constructor = class
        .members
        .iter()
        .filter_map(|member_id| match arena.class_member(*member_id) {
            ClassMember::Constructor(constructor)
                if constructor.params.len() == entry.args.len() =>
            {
                Some(constructor)
            }
            _ => None,
        })
        .next();

    let params = matching_constructor
        .map(|constructor| constructor.params.clone())
        .unwrap_or_default();
    let mut descriptor = String::from("(Ljava/lang/String;I");
    for param_id in params {
        let param = arena.param(param_id);
        descriptor.push_str(&type_to_descriptor(
            arena,
            param.ty,
            &rajac_types::TypeArena::new(),
        )?);
    }
    descriptor.push_str(")V");
    Ok(descriptor)
}

fn build_nest_attribute(
    constant_pool: &mut ConstantPool,
    _this_class: u16,
    outer_internal_name: Option<&str>,
    nested_classes: &[NestedClassInfo],
) -> RajacResult<Option<Attribute>> {
    if let Some(outer_internal_name) = outer_internal_name {
        return Ok(Some(Attribute::NestHost {
            name_index: constant_pool.add_utf8("NestHost")?,
            host_class_index: constant_pool.add_class(outer_internal_name)?,
        }));
    }

    if nested_classes.is_empty() {
        return Ok(None);
    }

    Ok(Some(Attribute::NestMembers {
        name_index: constant_pool.add_utf8("NestMembers")?,
        class_indexes: nested_classes
            .iter()
            .map(|nested| constant_pool.add_class(&nested.internal_name))
            .collect::<Result<Vec<_>, _>>()?,
    }))
}

fn push_small_int(code: &mut Vec<Instruction>, value: i32) {
    match value {
        0 => code.push(Instruction::Iconst_0),
        1 => code.push(Instruction::Iconst_1),
        2 => code.push(Instruction::Iconst_2),
        3 => code.push(Instruction::Iconst_3),
        4 => code.push(Instruction::Iconst_4),
        5 => code.push(Instruction::Iconst_5),
        -128..=127 => code.push(Instruction::Bipush(value as i8)),
        _ => code.push(Instruction::Sipush(value as i16)),
    }
}

fn enum_clinit_max_stack(class: &ClassDecl) -> u16 {
    let max_entry_args = class
        .enum_entries
        .iter()
        .map(|entry| entry.args.len() as u16)
        .max()
        .unwrap_or(0);
    4 + max_entry_args
}

fn emit_enum_literal_argument(
    code: &mut Vec<Instruction>,
    constant_pool: &mut ConstantPool,
    arena: &AstArena,
    arg: rajac_ast::ExprId,
) -> RajacResult<()> {
    if let rajac_ast::Expr::Literal(literal) = arena.expr(arg) {
        match literal.kind {
            rajac_ast::LiteralKind::Int => {
                let value = literal.value.as_str().parse::<i32>().unwrap_or_default();
                push_small_int(code, value);
            }
            rajac_ast::LiteralKind::String => {
                let string_index = constant_pool.add_string(literal.value.as_str())?;
                if string_index <= u8::MAX as u16 {
                    code.push(Instruction::Ldc(string_index as u8));
                } else {
                    code.push(Instruction::Ldc_w(string_index));
                }
            }
            rajac_ast::LiteralKind::Bool => {
                push_small_int(
                    code,
                    if literal.value.as_str() == "true" {
                        1
                    } else {
                        0
                    },
                );
            }
            _ => {}
        }
    }
    Ok(())
}
