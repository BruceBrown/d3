#[allow(unused_imports)]
#[macro_use]
extern crate smart_default;

#[allow(unused_imports)]
#[macro_use]
extern crate log;
extern crate simplelog;

use simplelog::*;

use std::str::FromStr;

use d3_components::components::ComponentCmd;
use d3_components::network;
use d3_components::settings;
use d3_core::executor;

use d3_test_drivers::chaos_monkey::ChaosMonkeyDriver;
use d3_test_drivers::daisy_chain::DaisyChainDriver;
use d3_test_drivers::fanout_fanin::FanoutFaninDriver;

mod components;
mod coordinators;
mod forwarder;

/// Currently, this is pretty thin. It consists of:
///     1) Reading configuration
///     2) Starting the server,
///     3) Running the configured server
///     4) Stopping the server
///
/// The only feature enabled is the forwarder, which is used for
/// testing. It can operate as a daisy chain or as a fanout-fanin.
///
/// The next thing do is create a trivial server capable of carrying
/// out tasks. We're going 3 tiers, centralized around a session.
/// Over the years, I've found that I've needed to share information
/// between sessions, and it was alwys a strange bolt-on thing. This
/// time around I think we'll plan on it sooner and make it part of
/// the model. There are also utility things that aren't necessarily
/// part of a session, we'll call them services. That leaves us
/// with components, which are factories for objects that participate
/// in a session. However, we still need something to wrap the session
/// allowing intrasession communication. Its somewhat important, even
/// if its not used, as it provides the home for cooperating sessions.
///
/// For a given server configuration, you're going to have a set of
/// components. Those components spawn instances forming a session
/// within ... ugh a conduit. There are essentially 3 kinds of
/// objects, a source, a sink and a source-sink, data flows in
/// a single direction, which isn't to say there can't be a means
/// for the sink to communicate with the source, it just requires
/// the source to also be a sink and the sink to be a source.
///
/// In the end, what this all boils down to is that a given object
/// implements an instruction set and gives its sender out to a
/// source.

fn main() {
    // process the configuration file.
    let settings = settings::Settings::new().expect("configuration error");
    // initialize the logger
    let level_filter = <log::LevelFilter as FromStr>::from_str(&settings.log_level).unwrap();
    CombinedLogger::init(vec![TermLogger::new(
        level_filter,
        Config::default(),
        TerminalMode::Mixed,
    )])
    .unwrap();
    // ensure we have a log of the settings
    log::warn!("{:?}", settings);

    // scream if we're in dev_mode
    if settings.dev_mode {
        log::error!("Running with dev_mode enabled")
    }

    // apply any tuning configuration
    if let Some(e) = settings.executor.as_ref() {
        if let Some(c) = e.queue_size.as_ref() {
            executor::set_default_channel_capacity(*c);
            log::info!("default q_size={}", executor::get_default_channel_capacity());
        }
        if let Some(c) = e.executors.as_ref() {
            executor::set_executor_count(*c);
            log::info!("executors={}", executor::get_executor_count());
        }
        if let Some(c) = e.time_slice.as_ref() {
            executor::set_time_slice(std::time::Duration::from_millis(*c as u64));
            log::info!("timeslice={:#?}", executor::get_time_slice());
        }
    };

    // start the server and network
    executor::start_server();
    network::start_network();

    // run until your done...
    run_server(&settings);

    // finally, shut down the network and server
    network::stop_network();
    executor::stop_server();
}

fn run_server(settings: &settings::Settings) {
    // configure components, Ok() contains vec<ComponentInfo>
    let result = components::configure(settings);
    if result.is_err() {
        log::error!("Component construction failed, shutting down")
    }
    let components = result.unwrap();
    if components.is_empty() {
        log::error!("No Components have been configured.")
    }
    // configure coordinators, Ok() contains vec<CoordinatorInfo>
    let result = coordinators::configure(settings, &components);
    if result.is_err() {
        log::error!("Coordinator construction failed, shutting down")
    }
    let coordinators = result.unwrap();
    if coordinators.is_empty() {
        log::error!("No Coordinators have been configured.")
    }

    // now that configuration is complete, start things up
    components.iter().for_each(|c| {
        if c.sender().send(ComponentCmd::Start).is_err() {
            log::warn!("unable to start component {:#?}", c.component())
        }
    });

    coordinators.iter().for_each(|c| {
        if c.sender().send(ComponentCmd::Start).is_err() {
            log::warn!("unable to start coordinator {:#?}", c.coordinator())
        }
    });

    // if forwarder is configured, get it running
    if settings.features.contains(&settings::Feature::Forwarder) {
        forwarder::run(settings);
    }

    let duration = std::time::Duration::from_secs(settings.test_server_duration * 60);
    std::thread::sleep(duration);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::atomic::AtomicCell;
    use d3_core::machine_impl::*;
    use d3_derive::*;
    use std::sync::Arc;

    #[test]
    fn alice() {
        // install a simple logger
        CombinedLogger::init(vec![TermLogger::new(
            LevelFilter::Error,
            Config::default(),
            TerminalMode::Mixed,
        )])
        .unwrap();

        // tweaks for more responsive testing
        executor::set_selector_maintenance_duration(std::time::Duration::from_millis(20));

        // get the server running
        executor::start_server();
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Time to create a default Alice, and connect her to the collective.
        // We end up with a thread-safe Alice and a command sender.
        let (alice, alice_sender) = executor::connect(Alice::default());

        // Let's prove that we have a default Alice.
        assert_eq!(alice.lock().unwrap().get_state(), AliceCmd::Init);

        // and that she's shared
        assert_eq!(Arc::strong_count(&alice), 2);

        // Let's send her a command to start. Don't forget, this is all
        // async, so let's give Alice some time to wake up.
        alice_sender.send(AliceCmd::Start).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(alice.lock().unwrap().get_state(), AliceCmd::Start);

        // Let's send her a command to stop
        alice_sender.send(AliceCmd::Stop).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(alice.lock().unwrap().get_state(), AliceCmd::Stop);

        // Let's tell her to initialize
        alice_sender.send(AliceCmd::Init).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(alice.lock().unwrap().get_state(), AliceCmd::Stop);

        // drop her sender...and she should go away
        drop(alice_sender);
        std::thread::sleep(std::time::Duration::from_millis(100));

        // and lets see that she's been forgotten and no longer shared
        assert_eq!(Arc::strong_count(&alice), 1);

        // now we can stop the server
        executor::stop_server();
    }
    // Here's a simple machine, we'll call her Alice.
    #[derive(SmartDefault, Debug, Copy, Clone, Eq, PartialEq, MachineImpl)]
    pub enum AliceCmd {
        #[default]
        Init,
        Start,
        Stop,
    }

    #[derive(Default)]
    struct Alice {
        state: AtomicCell<AliceCmd>,
    }
    impl Alice {
        fn get_state(&self) -> AliceCmd { self.state.load() }
    }
    impl Machine<AliceCmd> for Alice {
        fn disconnected(&self) {
            log::info!("Alice has left the building");
        }
        fn receive(&self, cmd: AliceCmd) {
            match cmd {
                AliceCmd::Init => {
                    log::error!("I'm Alice, I don't need to be initialized again.");
                },
                AliceCmd::Start => {
                    log::info!("I'm Alice, and I'm Starting up.");
                    self.state.store(cmd);
                },
                AliceCmd::Stop => {
                    log::info!("I'm Alice, and I'm Shutting down.");
                    self.state.store(cmd);
                },
            }
        }
    }
}
