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

pub fn list_to_values<S: std::fmt::Debug>(list: impl IntoIterator<Item = S>) -> String {
    let mut list = list.into_iter();

    let mut acc = String::from("(");
    if let Some(f) = list.next() {
        acc = format!("{acc}{f:?})");
    }
    for s in list {
        acc = format!("{acc},({s:?})");
    }

    acc
}

pub fn list_to_values_and_key(key: impl std::fmt::Debug, list: impl AsRef<[String]>) -> String {
    let list = list.as_ref();

    let mut acc = format!("({key:?},\"");
    let separator = format!("\"),({key:?},\"");
    if let Some(f) = list.first() {
        acc += f.as_ref();
    }
    for s in list.get(1..).unwrap_or_default() {
        acc = acc + &separator + s.as_ref();
    }
    acc += "\")";

    acc
}

pub fn list_to_sql<S: std::fmt::Debug>(list: impl IntoIterator<Item = S>) -> String {
    let mut list = list.into_iter();

    let mut acc = String::from("(");
    if let Some(f) = list.next() {
        acc = format!("{acc}{f:?}");
    }
    for s in list {
        acc = format!("{acc},{s:?}");
    }
    acc += ")";

    acc
}

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
