use tokio::signal;

pub mod auth;
pub mod header_params;
pub mod notify;
pub mod status;
#[cfg(test)]
pub mod testing;
pub mod tracing;

/// Registers Caller.
/// # Panics
/// Can panic if registering the handler fails. Since this happens at program startup time
/// it is acceptable
#[allow(
    clippy::expect_used,
    reason = "Run during server ramp-up. No unexpected shutdown during runtime."
)]
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
    () = ctrl_c => {},
    () = terminate => {},
    }
}

#[must_use]
pub fn as_option<T>(v: Vec<T>) -> Option<Vec<T>> {
    if v.is_empty() { None } else { Some(v) }
}
