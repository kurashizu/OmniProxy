#[cfg(not(unix))]
mod other;
#[cfg(unix)]
mod unix;

#[cfg(not(unix))]
pub(crate) use other::run;
#[cfg(unix)]
pub(crate) use unix::run;
