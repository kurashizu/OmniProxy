#[cfg(unix)]
use tracing::info;

pub fn init() {
    #[cfg(unix)]
    raise_nofile_limit();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    #[cfg(feature = "console")]
    {
        use tracing_subscriber::prelude::*;
        let (console_layer, server) = console_subscriber::ConsoleLayer::new();
        tokio::spawn(server.serve());
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "client=info".into());
        tracing_subscriber::registry()
            // Route fmt output to stderr so the proxy's stderr relay
            // (which the GUI's `last_error` buffer tracks) captures
            // everything the client logs at WARN/ERROR.
            // with_ansi(false) — GUI surfaces this in dialogs/banners,
            // escape codes would render as literal garbage.
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_ansi(false)
                    .with_filter(env_filter),
            )
            .with(console_layer)
            .init();
    }

    #[cfg(not(feature = "console"))]
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "client=info".into()),
        )
        .init();
}

#[cfg(unix)]
fn raise_nofile_limit() {
    unsafe {
        let mut rl = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut rl) == 0 {
            let target = rl.rlim_max.min(65535);
            if rl.rlim_cur < target {
                rl.rlim_cur = target;
                libc::setrlimit(libc::RLIMIT_NOFILE, &rl);
                info!("raised nofile limit to {target}");
            }
        }
    }
}
