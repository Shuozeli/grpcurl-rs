mod cli;
mod validate;

use clap::Parser;
use cli::{Cli, Command};
use std::process;

use grpcurl_core::connection::{self, ConnectionConfig};
use grpcurl_core::descriptor::{self, DescriptorSource};
use grpcurl_core::format;
use grpcurl_core::metadata;
use grpcurl_core::reflection;

/// Exit code offset to avoid conflicts with gRPC status codes.
/// gRPC Cancelled=1, Unknown=2, so we offset by 64.
const STATUS_CODE_OFFSET: i32 = 64;

#[tokio::main]
async fn main() {
    let normalized = cli::normalize_args(std::env::args());
    let cli = Cli::parse_from(normalized);

    let parsed = match validate::validate(&cli) {
        Ok(parsed) => parsed,
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("Try 'grpcurl --help' for more details.");
            process::exit(2);
        }
    };

    let conn_config = cli.connection_config();

    match parsed.command {
        Command::List => {
            let source =
                match create_descriptor_source(&cli, &conn_config, parsed.address.as_deref()).await
                {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to create descriptor source: {e}");
                        process::exit(1);
                    }
                };

            if let Err(err) =
                grpcurl_core::commands::list::run_list(source.as_ref(), parsed.symbol.as_deref())
                    .await
            {
                match parsed.symbol.as_deref() {
                    Some(svc) => eprintln!("Failed to list methods for service \"{svc}\": {err}"),
                    None => eprintln!("Failed to list services: {err}"),
                }
                process::exit(1);
            }

            // Export protoset/protos if requested
            let export_symbols =
                resolve_export_symbols(source.as_ref(), parsed.symbol.as_deref()).await;
            export_protoset(&cli, source.as_ref(), &export_symbols).await;
            export_proto_files(&cli, source.as_ref(), &export_symbols).await;
        }
        Command::Describe => {
            let source =
                match create_descriptor_source(&cli, &conn_config, parsed.address.as_deref()).await
                {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to create descriptor source: {e}");
                        process::exit(1);
                    }
                };
            let format_options = format::FormatOptions {
                emit_defaults: cli.emit_defaults,
                allow_unknown_fields: cli.allow_unknown_fields,
            };
            if let Err(err) = grpcurl_core::commands::describe::run_describe(
                source.as_ref(),
                parsed.symbol.as_deref(),
                &format_options,
                cli.msg_template,
            )
            .await
            {
                match parsed.symbol.as_deref() {
                    Some(sym) => eprintln!("Failed to resolve symbol \"{sym}\": {err}"),
                    None => eprintln!("Failed to describe services: {err}"),
                }
                process::exit(1);
            }

            // Export protoset/protos if requested
            let export_symbols =
                resolve_export_symbols(source.as_ref(), parsed.symbol.as_deref()).await;
            export_protoset(&cli, source.as_ref(), &export_symbols).await;
            export_proto_files(&cli, source.as_ref(), &export_symbols).await;
        }
        Command::Invoke => {
            let address = parsed
                .address
                .as_deref()
                .expect("address required for invoke");
            let symbol = parsed
                .symbol
                .as_deref()
                .expect("symbol required for invoke");
            let verbosity = cli.verbosity();

            let source = match create_descriptor_source(&cli, &conn_config, Some(address)).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to create descriptor source: {e}");
                    process::exit(1);
                }
            };

            // Create a channel for the RPC invocation
            let channel = match connection::create_channel(&conn_config, address).await {
                Ok(ch) => ch,
                Err(e) => {
                    eprintln!("Failed to connect to {address}: {e}");
                    process::exit(1);
                }
            };

            let invoke_config = cli.invoke_config();

            match grpcurl_core::commands::invoke::run_invoke(
                &invoke_config,
                channel,
                symbol,
                source.as_ref(),
            )
            .await
            {
                Ok(invoke_result) => {
                    // Verbose summary: "Sent N request(s) and received M response(s)"
                    // Go prints this to stdout (fmt.Printf in main.go)
                    if verbosity > 0 {
                        let req_word = if invoke_result.num_requests == 1 {
                            "request"
                        } else {
                            "requests"
                        };
                        let resp_word = if invoke_result.num_responses == 1 {
                            "response"
                        } else {
                            "responses"
                        };
                        println!(
                            "Sent {} {} and received {} {}",
                            invoke_result.num_requests,
                            req_word,
                            invoke_result.num_responses,
                            resp_word
                        );
                    }

                    // Handle gRPC status
                    if let Some(ref status) = invoke_result.status {
                        if status.code() != tonic::Code::Ok {
                            if cli.format_error {
                                // Format the error using the format flag
                                eprintln!(
                                    "ERROR:\n  Code: {}\n  Message: {}",
                                    format::status_code_name(status.code()),
                                    status.message()
                                );
                            } else {
                                format::print_status(status, None);
                            }
                            process::exit(STATUS_CODE_OFFSET + status.code() as i32);
                        }
                    }
                }
                Err(err) => {
                    eprintln!("Error invoking method \"{symbol}\": {err}");
                    process::exit(1);
                }
            }
        }
    }
}

/// Resolve export symbols: if a specific symbol was given, use it;
/// otherwise list all services.
async fn resolve_export_symbols(
    source: &dyn DescriptorSource,
    symbol: Option<&str>,
) -> Vec<String> {
    match symbol {
        Some(sym) => vec![sym.to_string()],
        None => match descriptor::list_services(source).await {
            Ok(svcs) => svcs,
            Err(e) => {
                eprintln!("Failed to resolve symbols for export: {e}");
                process::exit(1);
            }
        },
    }
}

/// Export protoset file if --protoset-out is set.
async fn export_protoset(cli: &Cli, source: &dyn DescriptorSource, symbols: &[String]) {
    if let Some(ref protoset_out) = cli.protoset_out {
        if let Err(e) = descriptor::write_protoset(protoset_out, source, symbols).await {
            eprintln!("Failed to write protoset output: {e}");
            process::exit(1);
        }
    }
}

/// Export .proto files if --proto-out-dir is set.
async fn export_proto_files(cli: &Cli, source: &dyn DescriptorSource, symbols: &[String]) {
    if let Some(ref proto_out_dir) = cli.proto_out_dir {
        if let Err(e) = descriptor::write_proto_files(proto_out_dir, source, symbols).await {
            eprintln!("Failed to write proto files: {e}");
            process::exit(1);
        }
    }
}

/// Create a descriptor source from CLI flags.
///
/// Matching Go's behavior:
/// - If proto/protoset files are specified AND an address is available with
///   reflection enabled, creates a CompositeSource (reflection + file fallback)
/// - If only proto/protoset files: uses FileSource
/// - If only address: uses ServerSource (reflection)
async fn create_descriptor_source(
    cli: &Cli,
    conn_config: &ConnectionConfig,
    address: Option<&str>,
) -> grpcurl_core::error::Result<Box<dyn DescriptorSource>> {
    // Build file-based source if proto/protoset files are specified
    let file_source: Option<Box<dyn DescriptorSource>> = if !cli.protoset.is_empty() {
        Some(Box::new(descriptor::descriptor_source_from_protosets(
            &cli.protoset,
        )?))
    } else if !cli.proto.is_empty() {
        Some(Box::new(descriptor::descriptor_source_from_proto_files(
            &cli.import_path,
            &cli.proto,
        )?))
    } else {
        None
    };

    // Build reflection source if address is available and reflection is not disabled.
    // When proto/protoset files are provided, auto-disable reflection unless
    // explicitly enabled with --use-reflection=true (matching Go behavior).
    let has_proto_files = !cli.protoset.is_empty() || !cli.proto.is_empty();
    let use_reflection = match cli.use_reflection {
        Some(true) => true,
        Some(false) => false,
        None => !has_proto_files,
    };
    let reflection_source: Option<Box<dyn DescriptorSource>> = if let Some(addr) = address {
        if use_reflection {
            let channel = connection::create_channel(conn_config, addr).await?;

            // Build reflection metadata: -H (all) + --reflect-header (reflection-only)
            let mut reflect_headers: Vec<String> = cli.header.clone();
            reflect_headers.extend(cli.reflect_header.clone());
            if cli.expand_headers {
                reflect_headers = metadata::expand_headers(&reflect_headers)?;
            }
            let reflect_md = metadata::metadata_from_headers(&reflect_headers);

            let source = if reflect_md.is_empty() {
                reflection::ServerSource::new(channel).with_max_msg_sz(cli.max_msg_sz)
            } else {
                reflection::ServerSource::with_metadata(channel, reflect_md)
                    .with_max_msg_sz(cli.max_msg_sz)
            };
            Some(Box::new(source))
        } else {
            None
        }
    } else {
        None
    };

    // Combine sources: composite when both available, otherwise use whichever exists
    match (reflection_source, file_source) {
        (Some(reflection), Some(file)) => {
            Ok(Box::new(descriptor::CompositeSource::new(reflection, file)))
        }
        (Some(reflection), None) => Ok(reflection),
        (None, Some(file)) => Ok(file),
        (None, None) => Err(grpcurl_core::error::GrpcurlError::InvalidArgument(
            "No host:port specified, no protoset specified, and no proto sources specified.".into(),
        )),
    }
}
