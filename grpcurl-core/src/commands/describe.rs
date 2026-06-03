use crate::descriptor::{DescriptorSource, SymbolDescriptor};
use crate::descriptor_text;
use crate::error::Result as GrpcurlResult;
use crate::format::{self, FormatOptions};
use crate::out;
use crate::output::Output;

pub async fn run_describe(
    source: &dyn DescriptorSource,
    symbol: Option<&str>,
    _format_options: &FormatOptions,
    msg_template: bool,
    output: &mut Output,
) -> GrpcurlResult<()> {
    match symbol {
        Some(sym) => {
            let desc = source.find_symbol(sym).await?;
            let text = descriptor_text::get_descriptor_text(&desc);
            out!(output, "{sym} is {}:", desc.type_label());
            output.println(&text);

            if msg_template {
                if let SymbolDescriptor::Message(msg_desc) = &desc {
                    print_msg_template(msg_desc, output)?;
                }
            }
        }
        None => {
            let services = source.list_services().await?;
            for service in &services {
                let desc = source.find_symbol(service).await?;
                let text = descriptor_text::get_descriptor_text(&desc);
                out!(output, "{service} is {}:", desc.type_label());
                output.println(&text);
            }
        }
    }
    Ok(())
}

fn print_msg_template(
    desc: &prost_reflect::MessageDescriptor,
    output: &mut Output,
) -> GrpcurlResult<()> {
    let template = format::make_template(desc);

    let template_options = FormatOptions {
        emit_defaults: true,
        allow_unknown_fields: false,
    };
    let formatter = format::json_formatter(&template_options);
    let text = (formatter)(&template)?;

    output.println("\nMessage template:");
    output.println(&text);
    Ok(())
}
