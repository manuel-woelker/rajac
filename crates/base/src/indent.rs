use crate::result::FelicoResult;
use std::fmt::Write;

pub fn indent(write: &mut dyn Write, indent: usize) -> FelicoResult<()> {
    write!(write, "{:indent$}", "", indent = indent)?;
    Ok(())
}
