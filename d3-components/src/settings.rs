extern crate config;
extern crate serde;

use config::{Config, ConfigError, Environment, File};
use std::collections::{HashMap, HashSet};
use std::env;

/// Its unfortunate that we need to make all the bits public. There's
/// possibly a way to avoid this with serde; I haven't figured it out
/// yet.

/// This is the top-level settings object
#[derive(Debug, Default, Deserialize)]
pub struct Settings {
    pub dev_mode: bool,
    pub log_level: String,
    pub test_server_duration: u64,
    pub executor: Option<Executor>,
    pub features: HashSet<Feature>,
    pub services: HashSet<Service>,
    pub coordinator: Vec<HashMap<Coordinator, CoordinatorVariant>>,
    pub component: Vec<HashMap<Component, ComponentVariant>>,
    pub additional: Vec<HashMap<Additional, AdditionalVariant>>,
}

pub type Feature = String;
pub type Service = String;
pub type Coordinator = String;
pub type Component = String;

/// Some tuning params, these might be better as fields
#[derive(Debug, Deserialize)]
pub struct Executor {
    pub executors: Option<usize>,
    pub queue_size: Option<usize>,
    pub time_slice: Option<usize>,
}

/// All of the coordinator config variants
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum CoordinatorVariant {
    SimpleTcpConfig {
        tcp_address: String,
        kv: Option<HashMap<String, String>>,
    },
}

/// All of the component config variants
#[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum ComponentVariant {
    SimpleConfig {
        enabled: bool,
        kv: Option<HashMap<String, String>>,
    },
}

/// There's always something that is being tinkered with.
/// Additional is for those things that are being experimented with
#[derive(Debug, Deserialize, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Additional {
    Forwarder,
}

/// These are some fields, which may replace kv
#[derive(Debug, Deserialize, Copy, Clone, Eq, PartialEq, Hash)]
#[allow(non_camel_case_types)]
pub enum Field {
    daisy_chain,
    fanout_fanin,
    chaos_monkey,
    forwarding_multiplier,
    machines,
    messages,
    iterations,
    timeout,
    fanin_capacity,
    inflection_value,
    unbound_queue,
}
/// a more general solution would be to use a variant rather than usize
pub type FieldMap = HashMap<Field, usize>;

/// We don't want to mess with other variants while experimenting
#[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum AdditionalVariant {
    Forwarder {
        run: Vec<Field>,
        default: FieldMap,
        daisy_chain: Option<FieldMap>,
        fanout_fanin: Option<FieldMap>,
        chaos_monkey: Option<FieldMap>,
    },
}

/// Finally, we get to where we assemble the settings from all the
/// different sources and freeze it.
impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let mut s = Config::new();

        // Start off by merging in the "default" configuration file
        s.merge(File::with_name("config/default"))?;

        // Add in the current environment file
        // Default to 'development' env
        // Note that this file is _optional_
        let env = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());
        s.merge(File::with_name(&format!("config/{}", env)).required(false))?;

        // Add in a local configuration file
        // This file shouldn't be checked in to git
        s.merge(File::with_name("config/local").required(false))?;

        // Add in settings from the environment (with a prefix of APP)
        // Eg.. `APP_DEBUG=1 ./target/app` would set the `debug` key
        s.merge(Environment::with_prefix("app"))?;

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_into()
    }
}

// Hopefully, most components will fit into this model
#[derive(Debug, Default)]
pub struct SimpleConfig {
    pub enabled: bool,
    pub kv: Option<HashMap<String, String>>,
}
impl SimpleConfig {
    pub fn from(v: &ComponentVariant) -> Self {
        match v.clone() {
            ComponentVariant::SimpleConfig { enabled, kv } => Self { enabled, kv },
        }
    }
}
