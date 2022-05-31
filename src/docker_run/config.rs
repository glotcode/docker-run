use crate::docker_run::api;
use crate::docker_run::debug;
use crate::docker_run::run;
use crate::docker_run::unix_stream;

#[derive(Clone, Debug)]
pub struct Config {
    pub server: ServerConfig,
    pub api: api::ApiConfig,
    pub unix_socket: unix_stream::Config,
    pub container: run::ContainerConfig,
    pub run: run::Limits,
    pub debug: debug::Config,
}

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub listen_port: u16,
    pub worker_threads: usize,
}
