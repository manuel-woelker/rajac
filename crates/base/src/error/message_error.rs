use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

pub struct MessageError {
    message: String,
}

impl<T: Into<String>> From<T> for MessageError {
    fn from(value: T) -> Self {
        Self {
            message: value.into(),
        }
    }
}

impl Debug for MessageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageError")
            .field("message", &self.message)
            .finish()
    }
}

impl Display for MessageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for MessageError {}
