use crate::result::RajacResult;
use std::fmt::Write;

pub fn indent(write: &mut dyn Write, indent: usize) -> RajacResult<()> {
    write!(write, "{:indent$}", "", indent = indent)?;
    Ok(())
}
