use crate::docker_run::unix_stream;
use crate::docker_run::run;
use crate::docker_run::api;
use crate::docker_run::debug;

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
    pub worker_threads: u16,
}

impl ServerConfig {
    pub fn listen_addr_with_port(&self) -> String {
        format!("{}:{}", self.listen_addr, self.listen_port)
    }
}
