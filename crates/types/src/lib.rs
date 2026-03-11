// Module declarations
mod array_type;
mod class_type;
mod ident;
mod primitive_type;
mod type_;
mod type_arena;
mod type_id;
mod type_variable;
mod wildcard;

// Public re-exports
pub use array_type::ArrayType;
pub use class_type::ClassType;
pub use ident::{Ident, TypeParam};
pub use primitive_type::PrimitiveType;
pub use type_::Type;
pub use type_arena::TypeArena;
pub use type_id::TypeId;
pub use type_variable::TypeVariable;
pub use wildcard::{WildcardBound, WildcardType};
