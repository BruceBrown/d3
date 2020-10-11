use super::*;
use d3_components::components::*;
use d3_components::settings::SimpleConfig;
use echo_service::component;

/// This is the entry point for getting all the components configured and active.
/// It lives in main, but may move. It needs to be very high in the stack as it
/// communicates with services.
///

/// configure enabled components and return their senders.
pub fn configure(settings: &settings::Settings) -> Result<Vec<ComponentInfo>, ComponentError> {
    let mut active_components: Vec<ComponentInfo> = Vec::new();
    for c in &settings.component {
        // c is the component HashMap
        for (k, v) in c {
            let config = SimpleConfig::from(v);
            if config.enabled {
                let result = match k {
                    settings::Component::EchoConsumer => {
                        component::echo_consumer::configure(config, &settings)
                    }
                    settings::Component::EchoProducer => {
                        component::echo_producer::configure(config, &settings)
                    }
                    settings::Component::ChatConsumer => {
                        chat_service::chat_consumer::configure(config, &settings)
                    }
                    settings::Component::ChatProducer => {
                        chat_service::chat_producer::configure(config, &settings)
                    }
                    #[allow(unreachable_patterns)]
                    _ => {
                        log::warn!("unhandled {:#?} component configuration", k);
                        Ok(None)
                    }
                };
                if let Err(e) = result {
                    return Err(e);
                }
                if let Ok(Some(sender)) = result {
                    active_components.push(ComponentInfo::new(*k, sender));
                }
            }
        }
    }
    Ok(active_components)
}
