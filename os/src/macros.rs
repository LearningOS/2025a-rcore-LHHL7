#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        #[cfg(feature = "trace")]
        log::trace!($($arg)*);
    };
}
