use crate::error::RajacError;
use crate::shared_string::SharedString;
use std::error::Error as StdError;

pub type RajacResult<T> = Result<T, RajacError>;

pub trait ResultExt<T> {
    fn context(self, context: impl Into<SharedString>) -> RajacResult<T>;

    fn with_context<C, S>(self, context: C) -> RajacResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>;
}

pub trait OptionExt<T> {
    fn context(self, context: impl Into<SharedString>) -> RajacResult<T>;

    fn with_context<C, S>(self, context: C) -> RajacResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: StdError + Send + Sync + 'static,
{
    fn context(self, context: impl Into<SharedString>) -> RajacResult<T> {
        self.map_err(|error| RajacError::message(context.into()).with_std_source(error))
    }

    fn with_context<C, S>(self, context: C) -> RajacResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>,
    {
        self.map_err(|error| RajacError::message(context()).with_std_source(error))
    }
}

impl<T> OptionExt<T> for Option<T> {
    fn context(self, context: impl Into<SharedString>) -> RajacResult<T> {
        self.ok_or_else(|| RajacError::message(context.into()))
    }

    fn with_context<C, S>(self, context: C) -> RajacResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>,
    {
        self.ok_or_else(|| RajacError::message(context()))
    }
}

#[cfg(test)]
mod tests {
    use crate::result::{OptionExt, ResultExt};
    use expect_test::expect;
    use std::io;

    #[test]
    fn test_with_context_converts_to_rajac_error() {
        let result: Result<(), io::Error> =
            Err(io::Error::new(io::ErrorKind::NotFound, "config missing"));
        let error = result.with_context(|| "failed to load config").unwrap_err();
        expect!([r#"
            Error: failed to load config
            Caused by: config missing
        "#])
        .assert_eq(&error.to_test_string());
    }

    #[test]
    fn test_context_converts_to_rajac_error() {
        let result: Result<(), io::Error> =
            Err(io::Error::new(io::ErrorKind::NotFound, "config missing"));
        let error = result.context("failed to load config").unwrap_err();
        expect!([r#"
            Error: failed to load config
            Caused by: config missing
        "#])
        .assert_eq(&error.to_test_string());
    }

    #[test]
    fn test_with_context_is_lazy_for_ok_results() {
        use std::cell::Cell;

        let context_called = Cell::new(false);
        let result: Result<i32, io::Error> = Ok(123);
        let value = result
            .with_context(|| {
                context_called.set(true);
                "should not be used"
            })
            .unwrap();
        assert_eq!(value, 123);
        assert!(!context_called.get());
    }

    #[test]
    fn test_option_context_converts_none_to_rajac_error() {
        let value: Option<i32> = None;
        let error = value.context("missing value").unwrap_err();
        expect!([r#"
            Error: missing value
        "#])
        .assert_eq(&error.to_test_string());
    }

    #[test]
    fn test_option_with_context_is_lazy_for_some_results() {
        use std::cell::Cell;

        let context_called = Cell::new(false);
        let value = Some(123)
            .with_context(|| {
                context_called.set(true);
                "should not be used"
            })
            .unwrap();
        assert_eq!(value, 123);
        assert!(!context_called.get());
    }
}
