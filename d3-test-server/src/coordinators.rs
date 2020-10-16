use super::*;
use alice_service::alice;
use d3_components::components::{ComponentError, ComponentInfo};
use d3_components::coordinators::CoordinatorInfo;
use echo_service::coordinator;
use monitor_service::monitor;

/// coordinators (I dislike the name) assemble components to form a service.
/// The idea being that they have an understanding of what's needed and can
/// call upon components to create instances.
pub fn configure(
    settings: &settings::Settings,
    components: &[ComponentInfo],
) -> Result<Vec<CoordinatorInfo>, ComponentError> {
    let mut active_coordinators: Vec<CoordinatorInfo> = Vec::new();
    for c in &settings.coordinator {
        // c is the coordinator HashMap
        for k in c.keys() {
            let result = match k {
                settings::Coordinator::EchoCoordinator => coordinator::echo_service::configure(settings, components),
                settings::Coordinator::ChatCoordinator => {
                    chat_service::chat_coordinator::configure(settings, components)
                },
                settings::Coordinator::MonitorCoordinator => monitor::configure(settings, components),
                settings::Coordinator::AliceCoordinator => alice::configure(settings, components),
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
                active_coordinators.push(CoordinatorInfo::new(*k, sender))
            }
        }
    }
    Ok(active_coordinators)
}
