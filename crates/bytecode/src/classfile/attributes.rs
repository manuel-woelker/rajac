use super::{NestedClassInfo, type_to_internal_class_name};
use rajac_ast::{AstArena, ClassDecl, ClassKind, Modifiers};
use rajac_base::result::{RajacResult, ResultExt};
use ristretto_classfile::attributes::{Attribute, InnerClass, NestedClassAccessFlags};
use ristretto_classfile::{ClassAccessFlags, ConstantPool};

pub(crate) fn build_inner_classes_attribute(
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

pub(crate) fn class_access_flags(kind: ClassKind, modifiers: &Modifiers) -> ClassAccessFlags {
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
            flags |= ClassAccessFlags::FINAL;
        }
        ClassKind::Annotation => {
            flags |= ClassAccessFlags::ANNOTATION;
            flags |= ClassAccessFlags::INTERFACE;
            flags |= ClassAccessFlags::ABSTRACT;
        }
        ClassKind::Record => {}
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
            flags |= NestedClassAccessFlags::STATIC;
            flags |= NestedClassAccessFlags::FINAL;
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

pub(crate) fn has_modifier(modifiers: &Modifiers, mask: u32) -> bool {
    modifiers.0 & mask != 0
}

pub(crate) fn exceptions_attribute_from_ast_types(
    constant_pool: &mut ConstantPool,
    throws: &[rajac_ast::AstTypeId],
    arena: &AstArena,
    type_arena: &rajac_types::TypeArena,
) -> RajacResult<Option<Attribute>> {
    if throws.is_empty() {
        return Ok(None);
    }

    let name_index = constant_pool.add_utf8("Exceptions")?;
    let mut exception_indexes = Vec::with_capacity(throws.len());
    for thrown_type in throws {
        let internal_name = type_to_internal_class_name(arena, *thrown_type, type_arena)?;
        exception_indexes.push(constant_pool.add_class(&internal_name)?);
    }

    Ok(Some(Attribute::Exceptions {
        name_index,
        exception_indexes,
    }))
}
