pub mod cli;
pub mod error;
pub mod file_path;
pub mod hash;
pub mod indent;
pub mod logging;
pub mod qualified_name;
pub mod result;
pub mod shared_string;
pub mod span;
pub mod test_print;
pub mod timestamp;
pub mod value;

pub use parking_lot::{Mutex, RwLock};
pub use qualified_name::FullyQualifiedClassName;

pub fn unansi(string: &str) -> String {
    anstream::adapter::strip_str(string).to_string()
}
