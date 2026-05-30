pub(crate) fn format_bytes(b: u64) -> String {
    if b > 1_099_511_627_776 {
        format!("{:.2} TiB", b as f64 / 1_099_511_627_776.0)
    } else if b > 1_073_741_824 {
        format!("{:.2} GiB", b as f64 / 1_073_741_824.0)
    } else if b > 1_048_576 {
        format!("{:.2} MiB", b as f64 / 1_048_576.0)
    } else if b > 1024 {
        format!("{:.2} KiB", b as f64 / 1024.0)
    } else {
        format!("{b} B")
    }
}

pub(crate) fn format_speed(bps: f64) -> String {
    if bps > 1_099_511_627_776.0 {
        format!("{:.2} TiB/s", bps / 1_099_511_627_776.0)
    } else if bps > 1_073_741_824.0 {
        format!("{:.2} GiB/s", bps / 1_073_741_824.0)
    } else if bps > 1_048_576.0 {
        format!("{:.2} MiB/s", bps / 1_048_576.0)
    } else if bps > 1024.0 {
        format!("{:.2} KiB/s", bps / 1024.0)
    } else {
        format!("{:.1} B/s", bps)
    }
}
