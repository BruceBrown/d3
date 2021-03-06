use super::*;

struct ForwarderSettings {
    run: Vec<settings::Field>,
    default: settings::FieldMap,
    daisy_chain: Option<settings::FieldMap>,
    fanout_fanin: Option<settings::FieldMap>,
    chaos_monkey: Option<settings::FieldMap>,
}

/// Take the setting from the variant and turn them into a concrete stuct that we can
/// pass around. Probably could do this with a trait...
pub fn run(settings: &settings::Settings) {
    log::info!("running forwarder");
    // let things settle down before diving in...
    std::thread::sleep(std::time::Duration::from_millis(750));
    // pull the forwarder info out of the additoanl hash map
    //
    for a in &settings.additional {
        if let Some(v) = a.get(&settings::Additional::Forwarder) {
            // v is a variant in AdditionalVariant, need to extract things info the Forwarder
            let f = match v.clone() {
                settings::AdditionalVariant::Forwarder {
                    run,
                    default,
                    daisy_chain,
                    fanout_fanin,
                    chaos_monkey,
                } => ForwarderSettings {
                    run,
                    default,
                    daisy_chain,
                    fanout_fanin,
                    chaos_monkey,
                },
            };
            // at this point f represents the Forwarder parameters
            for r in &f.run {
                match r {
                    settings::Field::daisy_chain => run_daisy_chain(&f),
                    settings::Field::fanout_fanin => run_fanout_fanin(&f),
                    settings::Field::chaos_monkey => run_chaos_monkey(&f),
                    _ => (),
                }
            }
        }
    }
}

/// This simply takes two maps, merges them, returning the merged result. In our case
/// we're taking a default map and overriding with any fields provided in the primary map
fn merge_maps(map1: settings::FieldMap, map2: settings::FieldMap) -> settings::FieldMap { map1.into_iter().chain(map2).collect() }

#[derive(Debug)]
struct RunParams {
    machine_count: usize,
    messages: usize,
    iterations: usize,
    forwarding_multiplier: usize,
    timeout: std::time::Duration,
    unbound_queue: bool,
}

// convert from a field map to RunParams
impl From<settings::FieldMap> for RunParams {
    fn from(map: settings::FieldMap) -> Self {
        Self {
            machine_count: *map.get(&settings::Field::machines).expect("machines missing"),
            messages: *map.get(&settings::Field::messages).expect("messages missing"),
            iterations: *map.get(&settings::Field::iterations).expect("iterations missing"),
            forwarding_multiplier: *map
                .get(&settings::Field::forwarding_multiplier)
                .expect("forwarding_multiplier missing"),
            timeout: std::time::Duration::from_secs(*map.get(&settings::Field::timeout).expect("timeout missing") as u64),
            unbound_queue: *map.get(&settings::Field::unbound_queue).unwrap_or(&0) != 0,
        }
    }
}

fn run_daisy_chain(settings: &ForwarderSettings) {
    let fields = match &settings.daisy_chain {
        Some(map) => merge_maps(settings.default.clone(), map.clone()),
        None => settings.default.clone(),
    };
    let params = RunParams::from(fields);
    log::info!("daisy_chain: {:?}", params);

    let mut daisy_chain = DaisyChainDriver {
        machine_count: params.machine_count,
        message_count: params.messages,
        bound_queue: !params.unbound_queue,
        forwarding_multiplier: params.forwarding_multiplier,
        duration: params.timeout,
        ..Default::default()
    };

    daisy_chain.setup();
    let t = std::time::Instant::now();
    for _ in 0 .. params.iterations {
        daisy_chain.run();
    }
    log::info!("completed daisy-chain run in {:#?}", t.elapsed());
    DaisyChainDriver::teardown(daisy_chain);
}

fn run_fanout_fanin(settings: &ForwarderSettings) {
    let fields = match &settings.fanout_fanin {
        Some(map) => merge_maps(settings.default.clone(), map.clone()),
        None => settings.default.clone(),
    };
    let params = RunParams::from(fields);
    log::info!("fanout_fanin: {:?}", params);

    let mut fanout_fanin = FanoutFaninDriver {
        machine_count: params.machine_count,
        message_count: params.messages,
        bound_queue: !params.unbound_queue,
        duration: params.timeout,
        ..Default::default()
    };

    fanout_fanin.setup();
    log::debug!("fanout_fanin: starting run");
    let t = std::time::Instant::now();
    for _ in 0 .. params.iterations {
        fanout_fanin.run();
    }
    log::info!("completed fanout_fanin run in {:#?}", t.elapsed());
    FanoutFaninDriver::teardown(fanout_fanin);
}

fn run_chaos_monkey(settings: &ForwarderSettings) {
    let fields = match &settings.chaos_monkey {
        Some(map) => merge_maps(settings.default.clone(), map.clone()),
        None => settings.default.clone(),
    };
    let params = RunParams::from(fields);

    let fields = match &settings.chaos_monkey {
        Some(map) => merge_maps(settings.default.clone(), map.clone()),
        None => settings.default.clone(),
    };
    let inflection_value = *fields.get(&settings::Field::inflection_value).unwrap_or(&1usize);
    log::info!("chaos_monkey: {:?}, inflection_value {}", params, inflection_value);

    let mut chaos_monkey = ChaosMonkeyDriver {
        machine_count: params.machine_count,
        message_count: params.messages,
        bound_queue: !params.unbound_queue,
        duration: params.timeout,
        inflection_value: inflection_value as u32,
        ..Default::default()
    };

    chaos_monkey.setup();
    log::debug!("chaos_monkey: starting run");
    d3::core::executor::stats::request_stats_now();
    let t = std::time::Instant::now();
    for _ in 0 .. params.iterations {
        chaos_monkey.run();
    }
    d3::core::executor::stats::request_stats_now();
    log::info!("completed chaos_monkey run in {:#?}", t.elapsed());
    ChaosMonkeyDriver::teardown(chaos_monkey);
}
