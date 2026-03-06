use std::sync::Once;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_logging() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer(), /*.with_span_events(FmtSpan::ENTER)*/
            )
            .with(EnvFilter::builder().parse("INFO").unwrap())
            .init();
    });
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::error!($($arg)*);
    };
}

pub use log_error;
pub use tracing::{debug, error, info, trace, warn};
