pub struct ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        println!("{}: {}: {}", record.target(), record.level(), record.args());
    }

    fn flush(&self) {}
}

#[allow(private_interfaces)]
pub static LOGGER: ConsoleLogger = ConsoleLogger;

/// Unwraps the `Result` or panics with the provided message.
///
/// # Examples
/// ```
/// use tg::ok_or_panic;
/// let n = ok_or_panic!(std::thread::spawn(|| 5).join(), "Error spawning thread");
/// ```
#[macro_export]
macro_rules! ok_or_panic {
    ($expr:expr, $($tt:tt)*) => {
        ::anyhow::Context::with_context($expr, || format!($($tt)*)).unwrap()
    };
}

/// Unwraps the `Option` or panics with the provided message.
///
/// # Examples
/// ```should_panic
/// use tg::or_panic;
/// let vec = Vec::<u8>::new();
/// let first = or_panic!(vec.first(), "Vec should have at least one element!");
/// ```
#[macro_export]
macro_rules! or_panic {
    ($expr:expr, $($tt:tt)*) => {
        ::anyhow::Context::with_context($expr, || format!($($tt)*)).unwrap()
    };
}
