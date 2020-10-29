use super::*;

use alice_service::alice;
use chat_service::*;
use d3::components::coordinators::CoordinatorInfo;
use d3::components::{ComponentError, ComponentInfo};
use echo_service::*;
use monitor_service::monitor;
/// coordinators (I dislike the name) assemble components to form a service.
/// The idea being that they have an understanding of what's needed and can
/// call upon components to create instances.

pub fn configure(settings: &settings::Settings, components: &[ComponentInfo]) -> Result<Vec<CoordinatorInfo>, ComponentError> {
    let mut active_coordinators: Vec<CoordinatorInfo> = Vec::new();
    for c in &settings.coordinator {
        // c is the coordinator HashMap
        for k in c.keys() {
            let result = match k.as_str() {
                "EchoCoordinator" => echo_coordinator::configure(settings, components),
                "ChatCoordinator" => chat_coordinator::configure(settings, components),
                "MonitorCoordinator" => monitor::configure(settings, components),
                "AliceCoordinator" => alice::configure(settings, components),
                #[allow(unreachable_patterns)]
                _ => {
                    log::warn!("unhandled {:#?} coordintor configuration", k);
                    Ok(None)
                },
            };
            if let Err(e) = result {
                return Err(e);
            }
            if let Ok(Some(sender)) = result {
                active_coordinators.push(CoordinatorInfo::new(k.clone(), sender))
            }
        }
    }
    Ok(active_coordinators)
}
