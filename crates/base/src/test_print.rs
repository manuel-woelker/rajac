use crate::result::FelicoResult;
use std::fmt::Write;

pub trait TestPrint {
    fn test_print(&self, write: &mut dyn Write, indent: usize) -> FelicoResult<()>;
    fn test_print_to_string(&self, indent: usize) -> FelicoResult<String> {
        let mut string = String::new();
        self.test_print(&mut string, indent)?;
        Ok(string)
    }
    fn indent(&self, write: &mut dyn Write, indent: usize) -> FelicoResult<()> {
        write!(write, "{:indent$}", "", indent = indent)?;
        Ok(())
    }
}
