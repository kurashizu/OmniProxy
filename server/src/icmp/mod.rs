#[cfg(unix)]
mod unix;
#[cfg(not(unix))]
mod other;

#[cfg(unix)]
pub(crate) use unix::run;
#[cfg(not(unix))]
pub(crate) use other::run;