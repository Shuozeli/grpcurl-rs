mod auth;
mod bank;
mod chat;
mod db;

pub mod pb {
    tonic::include_proto!("bank");

    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("bank_descriptor");
}

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use clap::Parser;
use tonic::transport::Server;

use bank::BankService;
use chat::ChatService;
use db::AccountStore;
use pb::bank_server::BankServer;
use pb::support_server::SupportServer;

#[derive(Parser)]
#[command(name = "bankdemo", about = "Bank demo gRPC server")]
struct Cli {
    /// The port on which bankdemo gRPC server will listen.
    #[arg(short, long, default_value_t = 12345)]
    port: u16,

    /// The path to which bank account data is saved and loaded.
    #[arg(short, long, default_value = "accounts.json")]
    datafile: String,
}

static REQ_COUNTER: AtomicU64 = AtomicU64::new(0);

fn log_interceptor(req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
    let req_id = REQ_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    let peer = req
        .remote_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "?".to_string());
    eprintln!("request {} started from {}", req_id, peer);
    Ok(req)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Load DB
    let store = AccountStore::load(&cli.datafile)?;
    let store = Arc::new(RwLock::new(store));

    let addr = format!("127.0.0.1:{}", cli.port).parse()?;
    eprintln!("server starting, listening on {}", addr);

    let bank_svc = BankService {
        store: Arc::clone(&store),
    };
    let chat_svc = ChatService::new();

    let reflection_svc = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(pb::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    let reflection_svc_alpha = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(pb::FILE_DESCRIPTOR_SET)
        .build_v1alpha()?;

    // Background saver (5s interval)
    let saver_store = Arc::clone(&store);
    let datafile = cli.datafile.clone();
    let shutdown_token = tokio_util::sync::CancellationToken::new();
    let saver_token = shutdown_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let cloned = saver_store.read().unwrap().clone_for_save();
                    if let Err(e) = cloned.save(&datafile) {
                        eprintln!("failed to save data to {:?}: {}", datafile, e);
                    }
                }
                _ = saver_token.cancelled() => {
                    break;
                }
            }
        }
    });

    let shutdown_store = Arc::clone(&store);
    let shutdown_datafile = cli.datafile.clone();

    Server::builder()
        .add_service(reflection_svc)
        .add_service(reflection_svc_alpha)
        .add_service(BankServer::with_interceptor(bank_svc, log_interceptor))
        .add_service(SupportServer::with_interceptor(chat_svc, log_interceptor))
        .serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c().await.ok();
            eprintln!("Shutting down...");
            shutdown_token.cancel();

            // Final flush
            let cloned = shutdown_store.read().unwrap().clone_for_save();
            if let Err(e) = cloned.save(&shutdown_datafile) {
                eprintln!("failed to save data on shutdown: {}", e);
            }
        })
        .await?;

    Ok(())
}
