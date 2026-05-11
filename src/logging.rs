use tracing_subscriber::FmtSubscriber;

pub fn init(debug_mode: bool) {
    let filter_directives = if debug_mode {
        "info,set_desto=debug"
    } else {
        "info,winit=warn"
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter_directives));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_ansi(!cfg!(target_os = "windows"))
        .with_target(true)
        .with_thread_ids(debug_mode)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}
