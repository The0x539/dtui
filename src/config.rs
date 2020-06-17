use std::sync::{Arc, RwLock};

use uuid::Uuid;
use serde::{Serialize, Deserialize};
use lazy_static::lazy_static;

type FnvIndexMap<K, V> = indexmap::IndexMap<K, V, fnv::FnvBuildHasher>;

const APP_NAME: &str = "dtui";
 
#[derive(Serialize, Deserialize)]
pub struct Host {
    pub username: String,
    pub password: String, // ¯\_(ツ)_/¯
    pub address: String,
    pub port: u16,
}

#[derive(Default, Serialize, Deserialize)]
pub struct ConnectionManagerConfig {
    pub autoconnect: Option<Uuid>,
    pub hide_on_start: bool,
    pub hosts: FnvIndexMap<Uuid, Host>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    pub connection_manager: ConnectionManagerConfig,
}

impl Config {
    fn save(&mut self) {
        // Mutation isn't required, but exclusive access makes sense.
        // Moreover, if you didn't already have a mutable ref to the config,
        // then you can't possibly have any changes to save anyway.
        confy::store(APP_NAME, self).unwrap()
    }
}

lazy_static! {
    static ref CONFIG: Arc<RwLock<Config>> = {
        let cfg: Config = confy::load(APP_NAME).unwrap();
        let cmgr = &cfg.connection_manager;
        if let Some(id) = cmgr.autoconnect {
            assert!(cmgr.hosts.contains_key(&id));
        }
        Arc::new(RwLock::new(cfg))
    };
}

pub fn get_config() -> Arc<RwLock<Config>> {
    Arc::clone(&self::CONFIG)
}
