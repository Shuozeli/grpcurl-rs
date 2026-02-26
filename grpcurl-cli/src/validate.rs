use grpcurl_core::format::Format;

use crate::cli::{Cli, Command, ParsedArgs};

/// Validate all CLI flags and positional arguments.
///
/// Implements all 28 validation rules from the original grpcurl, in order.
/// Hard errors return `Err(message)`. Warnings are printed to stderr but
/// do not prevent execution.
pub fn validate(cli: &Cli) -> Result<ParsedArgs, String> {
    // Rule 1: -connect-timeout must not be negative.
    if let Some(t) = cli.connect_timeout {
        if t < 0.0 {
            return Err("The --connect-timeout argument must not be negative.".into());
        }
    }

    // Rule 2: -keepalive-time must not be negative.
    if let Some(t) = cli.keepalive_time {
        if t < 0.0 {
            return Err("The --keepalive-time argument must not be negative.".into());
        }
    }

    // Rule 3: -max-time must not be negative.
    if let Some(t) = cli.max_time {
        if t < 0.0 {
            return Err("The --max-time argument must not be negative.".into());
        }
    }

    // Rule 4: -max-msg-sz must not be negative.
    if let Some(sz) = cli.max_msg_sz {
        if sz < 0 {
            return Err("The --max-msg-sz argument must not be negative.".into());
        }
    }

    // Derive TLS mode: default is TLS unless plaintext or alts.
    let use_tls = !cli.plaintext && !cli.alts;

    // Rule 5: -plaintext and -alts are mutually exclusive.
    if cli.plaintext && cli.alts {
        return Err("The --plaintext and --alts arguments are mutually exclusive.".into());
    }

    // Rule 6: -insecure requires TLS.
    if cli.insecure && !use_tls {
        return Err("The --insecure argument can only be used with TLS.".into());
    }

    // Rule 7: -cert requires TLS.
    if cli.cert.is_some() && !use_tls {
        return Err("The --cert argument can only be used with TLS.".into());
    }

    // Rule 8: -key requires TLS.
    if cli.key.is_some() && !use_tls {
        return Err("The --key argument can only be used with TLS.".into());
    }

    // Rule 9: -cert and -key must both be present or both absent.
    if cli.cert.is_some() != cli.key.is_some() {
        return Err(
            "The --cert and --key arguments must be used together and both be present.".into(),
        );
    }

    // Rule 10: -alts-handshaker-service requires -alts.
    if cli.alts_handshaker_service.is_some() && !cli.alts {
        return Err(
            "The --alts-handshaker-service argument must be used with the --alts argument.".into(),
        );
    }

    // Rule 11: -alts-target-service-account requires -alts.
    if !cli.alts_target_service_account.is_empty() && !cli.alts {
        return Err(
            "The --alts-target-service-account argument must be used with the --alts argument."
                .into(),
        );
    }

    // Rule 12: -format must be json or text.
    // (Handled by clap's FromStr on Format enum, but kept as a conceptual rule.)

    // Rule 13: -emit-defaults with non-json format emits a warning.
    if cli.emit_defaults && cli.format != Format::Json {
        warn("The --emit-defaults is only used when using json format.");
    }

    // ── Parse positional arguments ────────────────────────────────────

    let mut args = cli.args.iter().map(String::as_str).collect::<Vec<_>>();

    // Rule 14: At least one positional argument is required.
    if args.is_empty() {
        return Err("Too few arguments.".into());
    }

    // Rule 15: If first arg is not 'list' or 'describe', it is the address.
    let address = if args[0] != "list" && args[0] != "describe" {
        let addr = args.remove(0).to_string();
        Some(addr)
    } else {
        None
    };

    if args.is_empty() {
        return Err("Too few arguments.".into());
    }

    // Rule 16: Determine the command.
    let command;
    if args[0] == "list" {
        command = Command::List;
        args.remove(0);
    } else if args[0] == "describe" {
        command = Command::Describe;
        args.remove(0);
    } else {
        // Rule 16: If neither list nor describe, mode is invoke.
        command = Command::Invoke;
    }

    // Rule 17: For invoke, the symbol (method name) is required.
    let symbol = if command == Command::Invoke {
        if args.is_empty() {
            return Err("Too few arguments.".into());
        }
        Some(args.remove(0).to_string())
    } else {
        // Rule 18: -d with list/describe emits a warning (unused).
        if cli.data.is_some() {
            warn("The -d argument is not used with 'list' or 'describe' verb.");
        }
        // Rule 19: -rpc-header with list/describe emits a warning (unused).
        if !cli.rpc_header.is_empty() {
            warn("The --rpc-header argument is not used with 'list' or 'describe' verb.");
        }
        if !args.is_empty() {
            Some(args.remove(0).to_string())
        } else {
            None
        }
    };

    // Rule 20: Extra positional arguments are rejected.
    if !args.is_empty() {
        return Err("Too many arguments.".into());
    }

    // Rule 21: For invoke, address is required.
    if command == Command::Invoke && address.is_none() {
        return Err("No host:port specified.".into());
    }

    // Rule 22: At least one of: address, -protoset, or -proto must be given.
    if address.is_none() && cli.protoset.is_empty() && cli.proto.is_empty() {
        return Err(
            "No host:port specified, no protoset specified, and no proto sources specified.".into(),
        );
    }

    // Rule 23: -reflect-header with -protoset emits a warning (unused).
    if !cli.protoset.is_empty() && !cli.reflect_header.is_empty() {
        warn("The --reflect-header argument is not used when --protoset files are used.");
    }

    // Rule 24: -protoset and -proto are mutually exclusive.
    if !cli.protoset.is_empty() && !cli.proto.is_empty() {
        return Err("Use either --protoset files or --proto files, but not both.".into());
    }

    // Rule 25: -import-path without -proto emits a warning (unused).
    if !cli.import_path.is_empty() && cli.proto.is_empty() {
        warn("The --import-path argument is not used unless --proto files are used.");
    }

    // Rule 26: If -use-reflection is false, at least one of -protoset or -proto must be given.
    let use_reflection_explicit = cli.use_reflection;
    if use_reflection_explicit == Some(false) && cli.protoset.is_empty() && cli.proto.is_empty() {
        return Err(
            "No protoset files or proto files specified and --use-reflection set to false.".into(),
        );
    }

    // Rule 27: If -protoset or -proto is given and -use-reflection was not explicitly set,
    // reflection defaults to false.
    // (This is runtime behavior, not validation. Noted here for completeness.)

    // Rule 28: -servername and -authority cannot both be set to different values.
    if let (Some(sn), Some(auth)) = (&cli.servername, &cli.authority) {
        if sn == auth {
            warn("Both --servername and --authority are present; prefer only --authority.");
        } else {
            return Err("Cannot specify different values for --servername and --authority.".into());
        }
    }

    Ok(ParsedArgs {
        address,
        command,
        symbol,
    })
}

fn warn(msg: &str) {
    eprintln!("Warning: {msg}");
}
