use std::error::Error as StdError;
use std::fmt::{Debug, Display, Formatter};

use crate::shared_string::SharedString;
use crate::unansi;

#[derive(Debug)]
pub enum ErrorKind {
    Message(SharedString),
    Std(Box<dyn StdError + Send + Sync + 'static>),
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::Std(error) => Display::fmt(error, f),
        }
    }
}

#[derive(Debug)]
pub struct RajacError {
    kind: ErrorKind,
    source: Option<Box<RajacError>>,
}

impl RajacError {
    pub fn new(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }

    pub fn message(s: impl Into<SharedString>) -> Self {
        Self::new(ErrorKind::Message(s.into()))
    }

    pub fn std(error: impl StdError + Send + Sync + 'static) -> Self {
        Self::new(ErrorKind::Std(Box::new(error)))
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn source(&self) -> Option<&RajacError> {
        self.source.as_deref()
    }

    pub fn with_source(mut self, source: impl Into<RajacError>) -> Self {
        self.source = Some(Box::new(source.into()));
        self
    }

    pub fn with_std_source(self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.with_source(RajacError::std(source))
    }

    pub fn write_to(&self, write: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(write, "Error: {}", self.kind)?;
        let mut source = self.source.as_deref();
        while let Some(error) = source {
            writeln!(write, "Caused by: {}", error.kind)?;
            source = error.source.as_deref();
        }
        Ok(())
    }

    pub fn to_test_string(&self) -> String {
        let mut test_string = String::new();
        self.write_to(&mut test_string).unwrap();
        unansi(&test_string)
    }
}

impl<T> From<T> for RajacError
where
    T: StdError + Send + Sync + 'static,
{
    fn from(value: T) -> Self {
        Self::std(value)
    }
}

#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::error::RajacError::message(format!($($arg)*))
    };
}
pub use err;

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::err!($($arg)*))
    };
}
pub use bail;

#[cfg(test)]
mod tests {
    use std::io;

    use expect_test::expect;

    use crate::error::RajacError;
    use crate::result::RajacResult;

    #[test]
    fn test_err() {
        let err = err!("test {}", 123);
        assert_eq!(err.to_test_string(), "Error: test 123\n");
    }

    #[test]
    fn test_bail() {
        let err = (|| -> RajacResult<()> {
            bail!("test {}", 123);
        })()
        .unwrap_err();
        assert_eq!(err.to_test_string(), "Error: test 123\n");
    }

    #[test]
    fn test_error_chaining() {
        let err = RajacError::message("failed to read file")
            .with_source(RajacError::message("missing file"));
        expect!([r#"
            Error: failed to read file
            Caused by: missing file
        "#])
        .assert_eq(&err.to_test_string());
    }

    #[test]
    fn test_with_std_source() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "missing config");
        let err = RajacError::message("cannot initialize").with_std_source(io_error);
        expect!([r#"
            Error: cannot initialize
            Caused by: missing config
        "#])
        .assert_eq(&err.to_test_string());
    }
}
