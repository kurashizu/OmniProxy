pub mod unix;
pub mod windows;

#[cfg(windows)]
pub use windows::is_elevated;

#[cfg(unix)]
pub use unix::is_elevated;

#[cfg(not(any(windows, unix)))]
pub fn is_elevated() -> bool {
    false
}
