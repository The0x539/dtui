use std::sync::RwLock;

use std::net::SocketAddr;

use uuid::Uuid;
use serde::{Serialize, Deserialize};
use lazy_static::lazy_static;

type FnvIndexMap<K, V> = indexmap::IndexMap<K, V, fnv::FnvBuildHasher>;
 
#[derive(PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum HostAddr {
    Address(SocketAddr),
    Domain(String, Option<u16>),
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Host {
    username: String,
    password: String, // ¯\_(ツ)_/¯
    host: i128,
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct ConnectionManagerConfig {
    hosts: FnvIndexMap<Uuid, Host>,
    autoconnect_host_id: Option<Uuid>,
    hide_connection_manager_on_start: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Config {
    connection_manager: RwLock<ConnectionManagerConfig>,
}

lazy_static! {
    static ref CONFIG: Config = confy::load("dtui").unwrap();
}

#[allow(dead_code)]
fn get_config() -> &'static Config {
    &self::CONFIG
}
