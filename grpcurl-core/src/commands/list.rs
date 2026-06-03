use crate::descriptor::{self, DescriptorSource};
use crate::error::Result;
use crate::output::Output;

pub async fn run_list(
    source: &dyn DescriptorSource,
    symbol: Option<&str>,
    output: &mut Output,
) -> Result<()> {
    match symbol {
        Some(service) => {
            let methods = descriptor::list_methods(source, service).await?;
            for method in &methods {
                output.println(method);
            }
        }
        None => {
            let services = descriptor::list_services(source).await?;
            for service in &services {
                output.println(service);
            }
        }
    }
    Ok(())
}
