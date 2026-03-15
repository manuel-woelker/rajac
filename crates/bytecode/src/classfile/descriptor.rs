use rajac_ast::{
    AstArena, AstType, Constructor as AstConstructor, Method as AstMethod, PrimitiveType,
};
use rajac_base::result::RajacResult;

pub(crate) fn method_to_descriptor(
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

pub(crate) fn constructor_to_descriptor(
    arena: &AstArena,
    constructor: &AstConstructor,
    type_arena: &rajac_types::TypeArena,
) -> RajacResult<String> {
    let mut s = String::new();
    s.push('(');
    for param_id in &constructor.params {
        let param = arena.param(*param_id);
        s.push_str(&type_to_descriptor(arena, param.ty, type_arena)?);
    }
    s.push(')');
    s.push('V');
    Ok(s)
}

pub(crate) fn type_to_descriptor(
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
            name, ty: type_id, ..
        } => {
            if *type_id != rajac_types::TypeId::INVALID {
                let type_entry = type_arena.get(*type_id);
                if let rajac_types::Type::Class(class_type) = type_entry {
                    return Ok(format!("L{};", class_type.internal_name()));
                }
            }
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

pub(crate) fn type_to_internal_class_name(
    arena: &AstArena,
    type_id: rajac_ast::AstTypeId,
    type_arena: &rajac_types::TypeArena,
) -> RajacResult<String> {
    let ty = arena.ty(type_id);
    Ok(match ty {
        AstType::Simple {
            name, ty: type_id, ..
        } => {
            if *type_id != rajac_types::TypeId::INVALID {
                let type_entry = type_arena.get(*type_id);
                if let rajac_types::Type::Class(class_type) = type_entry {
                    return Ok(class_type.internal_name());
                }
            }
            name.as_str().to_string()
        }
        AstType::Array { element_type, .. } => {
            type_to_internal_class_name(arena, *element_type, type_arena)?
        }
        _ => "java/lang/Object".to_string(),
    })
}
