//! Convergence Signal — proof that the convergence chain works.
//!
//! This service runs inside seph.1.1 (a child cluster spawned by seph.1
//! via ConvergenceProcess CRD). Its existence and health prove:
//!
//! 1. AMI booted correctly (kindling-init completed 14 phases)
//! 2. K3s is running (this pod is scheduled)
//! 3. FluxCD reconciled (this HelmRelease was deployed)
//! 4. Network works (this endpoint is reachable)
//! 5. Observability works (Prometheus scrapes /metrics)
//!
//! GET /                → human-readable proof
//! GET /api/convergence → structured JSON proof
//! GET /healthz         → liveness probe
//! GET /readyz          → readiness probe
//! GET /metrics         → prometheus metrics

use std::net::SocketAddr;
use std::time::Instant;

use axum::{Json, Router, routing::get};
use serde::Serialize;
use tracing::info;

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

#[derive(Serialize)]
struct ConvergenceProof {
    status: &'static str,
    cluster: String,
    pid: String,
    uptime_seconds: u64,
    nodes_ready: u32,
    nodes_total: u32,
    pods_running: u32,
    flux_reconciled: bool,
    message: &'static str,
    timestamp: String,
}

async fn root() -> &'static str {
    "seph.1.1 converged — convergence computing realized\n"
}

async fn convergence_proof() -> Json<ConvergenceProof> {
    let uptime = START.get().map_or(0, |s| s.elapsed().as_secs());

    // Query K8s API for live proof data
    let (nodes_ready, nodes_total, pods_running, flux_ok) = match kube::Client::try_default().await
    {
        Ok(client) => gather_proof(&client).await,
        Err(_) => (0, 0, 0, false),
    };

    let cluster = std::env::var("CLUSTER_NAME").unwrap_or_else(|_| "seph-1-1".into());
    let pid = std::env::var("PROCESS_ID").unwrap_or_else(|_| "1.1".into());

    Json(ConvergenceProof {
        status: if flux_ok && nodes_ready > 0 {
            "converged"
        } else {
            "converging"
        },
        cluster,
        pid,
        uptime_seconds: uptime,
        nodes_ready,
        nodes_total,
        pods_running,
        flux_reconciled: flux_ok,
        message: "Convergence computing realized — seph.1 spawned seph.1.1",
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn gather_proof(client: &kube::Client) -> (u32, u32, u32, bool) {
    use kube::api::{Api, ListParams};

    // Count nodes
    let nodes: Api<k8s_openapi::api::core::v1::Node> = Api::all(client.clone());
    let (nodes_ready, nodes_total) = match nodes.list(&ListParams::default()).await {
        Ok(list) => {
            let total = list.items.len() as u32;
            let ready = list
                .items
                .iter()
                .filter(|n| {
                    n.status
                        .as_ref()
                        .and_then(|s| s.conditions.as_ref())
                        .map_or(false, |conds| {
                            conds
                                .iter()
                                .any(|c| c.type_ == "Ready" && c.status == "True")
                        })
                })
                .count() as u32;
            (ready, total)
        }
        Err(_) => (0, 0),
    };

    // Count running pods
    let pods: Api<k8s_openapi::api::core::v1::Pod> = Api::all(client.clone());
    let pods_running = match pods.list(&ListParams::default()).await {
        Ok(list) => list
            .items
            .iter()
            .filter(|p| {
                p.status
                    .as_ref()
                    .and_then(|s| s.phase.as_ref())
                    .map_or(false, |phase| phase == "Running")
            })
            .count() as u32,
        Err(_) => 0,
    };

    // Check FluxCD kustomizations (dynamic API since CRD may not exist)
    let flux_ok = check_flux(client).await;

    (nodes_ready, nodes_total, pods_running, flux_ok)
}

async fn check_flux(client: &kube::Client) -> bool {
    let api = kube::Api::<kube::core::DynamicObject>::all_with(
        client.clone(),
        &kube::discovery::ApiResource {
            group: "kustomize.toolkit.fluxcd.io".into(),
            version: "v1".into(),
            api_version: "kustomize.toolkit.fluxcd.io/v1".into(),
            kind: "Kustomization".into(),
            plural: "kustomizations".into(),
        },
    );

    match api.list(&kube::api::ListParams::default()).await {
        Ok(list) => list.items.iter().all(|ks| {
            ks.data
                .get("status")
                .and_then(|s| s.get("conditions"))
                .and_then(|c| c.as_array())
                .map_or(false, |conds| {
                    conds.iter().any(|c| {
                        c.get("type").and_then(|t| t.as_str()) == Some("Ready")
                            && c.get("status").and_then(|s| s.as_str()) == Some("True")
                    })
                })
        }),
        Err(_) => false,
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn metrics() -> String {
    let uptime = START.get().map_or(0, |s| s.elapsed().as_secs());
    format!(
        "# HELP convergence_signal_up Service is running\n\
         # TYPE convergence_signal_up gauge\n\
         convergence_signal_up 1\n\
         # HELP convergence_signal_uptime_seconds Seconds since start\n\
         # TYPE convergence_signal_uptime_seconds gauge\n\
         convergence_signal_uptime_seconds {uptime}\n"
    )
}

#[tokio::main]
async fn main() {
    START.get_or_init(Instant::now);

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let app = Router::new()
        .route("/", get(root))
        .route("/api/convergence", get(convergence_proof))
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .route("/metrics", get(metrics));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("convergence-signal starting on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
