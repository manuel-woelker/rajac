use std::error::Error as StdError;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use tracing_error::{SpanTrace, SpanTraceStatus};

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
    location: &'static Location<'static>,
    span_trace: SpanTrace,
}

impl RajacError {
    #[track_caller]
    pub fn new(kind: ErrorKind) -> Self {
        Self::at_location(kind, Location::caller())
    }

    pub fn at_location(kind: ErrorKind, location: &'static Location<'static>) -> Self {
        Self {
            kind,
            source: None,
            location,
            span_trace: SpanTrace::capture(),
        }
    }

    #[track_caller]
    pub fn message(s: impl Into<SharedString>) -> Self {
        Self::message_at_location(s, Location::caller())
    }

    pub fn message_at_location(
        s: impl Into<SharedString>,
        location: &'static Location<'static>,
    ) -> Self {
        Self::at_location(ErrorKind::Message(s.into()), location)
    }

    #[track_caller]
    pub fn std(error: impl StdError + Send + Sync + 'static) -> Self {
        Self::std_at_location(error, Location::caller())
    }

    pub fn std_at_location(
        error: impl StdError + Send + Sync + 'static,
        location: &'static Location<'static>,
    ) -> Self {
        Self::at_location(ErrorKind::Std(Box::new(error)), location)
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn source(&self) -> Option<&RajacError> {
        self.source.as_deref()
    }

    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }

    pub fn span_trace(&self) -> &SpanTrace {
        &self.span_trace
    }

    pub fn with_source(mut self, source: impl Into<RajacError>) -> Self {
        self.source = Some(Box::new(source.into()));
        self
    }

    #[track_caller]
    pub fn with_std_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(RajacError::std_at_location(
            source,
            Location::caller(),
        )));
        self
    }

    pub fn with_std_source_at_location(
        mut self,
        source: impl StdError + Send + Sync + 'static,
        location: &'static Location<'static>,
    ) -> Self {
        self.source = Some(Box::new(RajacError::std_at_location(source, location)));
        self
    }

    pub fn write_to(&self, write: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(write, "Error: {}", self.kind)?;
        writeln!(
            write,
            "At: {}:{}:{}",
            self.location.file(),
            self.location.line(),
            self.location.column()
        )?;
        if self.span_trace.status() == SpanTraceStatus::CAPTURED {
            writeln!(write, "Span trace:")?;
            writeln!(write, "{}", self.span_trace)?;
        }
        let mut source = self.source.as_deref();
        while let Some(error) = source {
            writeln!(write, "Caused by: {}", error.kind)?;
            writeln!(
                write,
                "At: {}:{}:{}",
                error.location.file(),
                error.location.line(),
                error.location.column()
            )?;
            if error.span_trace.status() == SpanTraceStatus::CAPTURED {
                writeln!(write, "Span trace:")?;
                writeln!(write, "{}", error.span_trace)?;
            }
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
    #[track_caller]
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

    use crate::error::RajacError;
    use crate::logging::{info_span, init_logging};
    use crate::result::RajacResult;

    #[test]
    fn test_err() {
        let err = err!("test {}", 123);
        let rendered = err.to_test_string();
        assert!(rendered.contains("Error: test 123\n"));
        assert!(rendered.contains("At: crates/base/src/error.rs:"));
    }

    #[test]
    fn test_bail() {
        let err = (|| -> RajacResult<()> {
            bail!("test {}", 123);
        })()
        .unwrap_err();
        let rendered = err.to_test_string();
        assert!(rendered.contains("Error: test 123\n"));
        assert!(rendered.contains("At: crates/base/src/error.rs:"));
    }

    #[test]
    fn test_error_chaining() {
        let err = RajacError::message("failed to read file")
            .with_source(RajacError::message("missing file"));
        let rendered = err.to_test_string();
        assert!(rendered.contains("Error: failed to read file\n"));
        assert!(rendered.contains("Caused by: missing file\n"));
        assert_eq!(rendered.matches("At: crates/base/src/error.rs:").count(), 2);
    }

    #[test]
    fn test_with_std_source() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "missing config");
        let err = RajacError::message("cannot initialize").with_std_source(io_error);
        let rendered = err.to_test_string();
        assert!(rendered.contains("Error: cannot initialize\n"));
        assert!(rendered.contains("Caused by: missing config\n"));
        assert_eq!(rendered.matches("At: crates/base/src/error.rs:").count(), 2);
    }

    #[test]
    fn test_error_renders_span_trace_when_inside_span() {
        init_logging();
        let span = info_span!("error_test_span");
        let _guard = span.enter();

        let err = RajacError::message("failed inside span");
        let rendered = err.to_test_string();

        assert!(rendered.contains("Span trace:\n"));
        assert!(rendered.contains("error_test_span"));
    }
}
