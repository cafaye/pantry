//! The pantry binary.
//!
//! Read the registry from disk, serve it, shut down on a signal. There is no
//! database, no cache to warm and no upstream to wait for: the whole startup
//! cost is reading a handful of YAML files and validating them against core's
//! schema.
//!
//! A registry that fails to load does **not** stop the process. It leaves
//! `/healthz` answering 200 and `/readyz` answering 503, which is the whole
//! liveness/readiness split: an orchestrator holds traffic back instead of
//! restarting a process that is running perfectly well with nothing to serve.

use std::path::PathBuf;
use std::process::ExitCode;

use pantry::AppState;

/// Where the registry lives. The container ships it at `/etc/pantry/registry`;
/// a checkout reads `registry/` next to this binary's manifest.
const DEFAULT_REGISTRY_DIR: &str = "registry";

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let registry_dir = registry_dir();
    let bind = bind_address();

    let state = AppState::from_dir(&registry_dir);
    match state.registry() {
        Ok(registry) => tracing::info!(
            services = registry.len(),
            directory = %registry_dir.display(),
            "registry loaded"
        ),
        Err(error) => tracing::error!(
            %error,
            directory = %registry_dir.display(),
            "registry did not load; serving /healthz and /readyz only"
        ),
    }

    let listener = match tokio::net::TcpListener::bind(&bind).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%bind, %error, "cannot listen");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(%bind, "pantry is listening");

    if let Err(error) = axum::serve(listener, pantry::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!(%error, "the server stopped with an error");
        return ExitCode::FAILURE;
    }

    tracing::info!("pantry stopped");
    ExitCode::SUCCESS
}

/// `PANTRY_REGISTRY_DIR`, or `registry/` relative to the working directory.
fn registry_dir() -> PathBuf {
    std::env::var_os("PANTRY_REGISTRY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REGISTRY_DIR))
}

/// `PANTRY_BIND`, or `0.0.0.0:8080` — the port kit's Rust template exposes.
fn bind_address() -> String {
    std::env::var("PANTRY_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string())
}

/// Logs as JSON, as kit's Docker images expect. `RUST_LOG` filters it; the
/// default keeps startup and refusals and drops per-request chatter.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,axum=warn,tower_http=warn"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(false)
        .init();
}

/// SIGINT locally, SIGTERM in a container. The second signal stops waiting:
/// a pod that will not shut down inside its grace period gets killed anyway,
/// and so should this one.
async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("SIGINT handler installs");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler installs")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => tracing::info!("SIGINT, stopping"),
        () = terminate => tracing::info!("SIGTERM, stopping"),
    }
}
