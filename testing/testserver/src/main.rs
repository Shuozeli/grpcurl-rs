mod service;

use clap::Parser;
use tonic::transport::Server;
use tonic_reflection::server::Builder as ReflectionBuilder;

pub mod pb {
    tonic::include_proto!("testing");

    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("testing_descriptor");
}

#[derive(Parser, Debug)]
#[command(
    name = "testserver",
    about = "Test gRPC server for grpcurl verification"
)]
struct Cli {
    /// Port to listen on (0 for ephemeral)
    #[arg(short = 'p', long = "port", default_value_t = 0)]
    port: u16,

    /// Suppress request logging
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Disable server reflection
    #[arg(long = "noreflect")]
    noreflect: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", cli.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    println!("Listening on {}", local_addr);

    let test_service = pb::test_service_server::TestServiceServer::new(service::TestServiceImpl);
    let complex_service =
        pb::complex_service_server::ComplexServiceServer::new(service::ComplexServiceImpl);

    let mut builder = Server::builder();

    if !cli.noreflect {
        let reflection_service = ReflectionBuilder::configure()
            .register_encoded_file_descriptor_set(pb::FILE_DESCRIPTOR_SET)
            .build_v1()?;

        let reflection_service_alpha = ReflectionBuilder::configure()
            .register_encoded_file_descriptor_set(pb::FILE_DESCRIPTOR_SET)
            .build_v1alpha()?;

        builder
            .add_service(reflection_service)
            .add_service(reflection_service_alpha)
            .add_service(test_service)
            .add_service(complex_service)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await?;
    } else {
        builder
            .add_service(test_service)
            .add_service(complex_service)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await?;
    }

    Ok(())
}
