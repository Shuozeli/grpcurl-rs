use crate::descriptor::{DescriptorSource, SymbolDescriptor};
use crate::descriptor_text;
use crate::format::{self, FormatOptions};

pub async fn run_describe(
    source: &dyn DescriptorSource,
    symbol: Option<&str>,
    format_options: &FormatOptions,
    msg_template: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match symbol {
        Some(sym) => {
            let desc = source.find_symbol(sym).await?;
            let text = descriptor_text::get_descriptor_text(&desc);
            println!("{sym} is {}:", desc.type_label());
            println!("{text}");

            // If --msg-template and the symbol is a message, show a JSON template
            if msg_template {
                if let SymbolDescriptor::Message(msg_desc) = &desc {
                    print_msg_template(msg_desc, format_options)?;
                }
            }
        }
        None => {
            // Describe all services in declaration order (not sorted).
            let services = source.list_services().await?;
            for service in &services {
                let desc = source.find_symbol(service).await?;
                let text = descriptor_text::get_descriptor_text(&desc);
                println!("{service} is {}:", desc.type_label());
                println!("{text}");
            }
        }
    }
    Ok(())
}

/// Print a JSON template for a message type.
///
/// Uses emit_defaults=true to show all fields with their default values.
fn print_msg_template(
    desc: &prost_reflect::MessageDescriptor,
    _format_options: &FormatOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let template = format::make_template(desc);

    // Always use emit_defaults=true for templates to show all fields
    let template_options = FormatOptions {
        emit_defaults: true,
        allow_unknown_fields: false,
    };
    let formatter = format::json_formatter(&template_options);
    let output = (formatter)(&template)?;

    println!("\nMessage template:");
    println!("{output}");
    Ok(())
}
