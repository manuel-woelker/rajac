// Module declarations
mod array_type;
mod class_type;
mod field_arena;
mod field_id;
mod field_signature;
mod ident;
mod method_arena;
mod method_id;
mod method_signature;
mod primitive_type;
mod type_;
mod type_arena;
mod type_id;
mod type_variable;
mod wildcard;

// Public re-exports
pub use array_type::ArrayType;
pub use class_type::ClassType;
pub use field_arena::FieldArena;
pub use field_id::FieldId;
pub use field_signature::{FieldModifiers, FieldSignature};
pub use ident::{Ident, TypeParam};
pub use method_arena::MethodArena;
pub use method_id::MethodId;
pub use method_signature::{MethodModifiers, MethodSignature};
pub use primitive_type::PrimitiveType;
pub use type_::Type;
pub use type_arena::TypeArena;
pub use type_id::TypeId;
pub use type_variable::TypeVariable;
pub use wildcard::{WildcardBound, WildcardType};
