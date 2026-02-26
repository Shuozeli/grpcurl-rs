use crate::descriptor::{self, DescriptorSource};

pub async fn run_list(
    source: &dyn DescriptorSource,
    symbol: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    match symbol {
        Some(service) => {
            // List all methods of the given service
            let methods = descriptor::list_methods(source, service).await?;
            if methods.is_empty() {
                // Match Go behavior: empty service prints nothing
            } else {
                for method in &methods {
                    println!("{method}");
                }
            }
        }
        None => {
            // List all services
            let services = descriptor::list_services(source).await?;
            if services.is_empty() {
                // Match Go behavior: no services prints nothing
            } else {
                for service in &services {
                    println!("{service}");
                }
            }
        }
    }
    Ok(())
}
