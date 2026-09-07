//! OmniNova Native Messaging host.
//!
//! stdout is reserved for Native Messaging frames. Diagnostics go to stderr.

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::new("omninova_browser_host=info"))
        .init();

    if let Err(err) = omninova_browser_host::verify_connecting_origin(std::env::args()) {
        eprintln!("omninova-browser-host: {}", err.code());
        std::process::exit(2);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(err) = runtime.block_on(omninova_browser_host::run_native_host()) {
        eprintln!("omninova-browser-host: {}", err.code());
        std::process::exit(1);
    }
}
