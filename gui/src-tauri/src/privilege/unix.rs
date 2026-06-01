#[cfg(unix)]
pub fn is_elevated() -> bool {
    // On Unix, root == uid 0. GUI is not designed to auto-promote; user is
    // expected to launch with sudo (or systemd unit with User=root).
    unsafe { libc::getuid() == 0 }
}
