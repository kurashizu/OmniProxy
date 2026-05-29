#[cfg(unix)]
use tracing::info;

pub fn init() {
    #[cfg(unix)]
    raise_nofile_limit();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "client=info".into());

    #[cfg(feature = "console")]
    {
        use tracing_subscriber::prelude::*;
        let (console_layer, server) = console_subscriber::ConsoleLayer::new();
        tokio::spawn(server.serve());
        tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .init();
    }

    #[cfg(not(feature = "console"))]
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
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
