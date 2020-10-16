#![allow(dead_code)]

mod glot_docker_run;

use std::process;
use std::time::Duration;
use serde::Serialize;

use glot_docker_run::docker;
use glot_docker_run::run;
use glot_docker_run::config;
use glot_docker_run::environment;


fn main() {
    env_logger::init();
    let config = prepare_config();

    let payload = Payload{
        language: "bash".to_string(),
        files: vec![File{
            name: "main.sh".to_string(),
            content: "echo hello".to_string(),
        }],
        stdin: "".to_string(),
        command: "".to_string(),
    };

    let container_config = docker::default_container_config("glot/bash:latest".to_string());

    let res = run::run(config.unix_socket, run::RunRequest{
        container_config,
        payload,
        limits: run::Limits{
            max_execution_time: Duration::from_secs(30),
            max_output_size: 100000,
        },
    });
    println!("{:?}", res);
}

fn prepare_config() -> config::Config {
    let env = environment::get_environment();

    match build_config(&env) {
        Ok(cfg) => {
            cfg
        }

        Err(err) => {
            log::error!("Failed to build config: {}", err);
            process::exit(1)
        }
    }
}

fn build_config(env: &environment::Environment) -> Result<config::Config, environment::Error> {
    let server = build_server_config(env)?;
    let unix_socket = build_unix_socket_config(env)?;

    Ok(config::Config{
        server,
        unix_socket,
    })
}

fn build_server_config(env: &environment::Environment) -> Result<config::ServerConfig, environment::Error> {
    let listen_addr = environment::lookup(env, "SERVER_LISTEN_ADDR")?;
    let listen_port = environment::lookup(env, "SERVER_LISTEN_PORT")?;

    Ok(config::ServerConfig{
        listen_addr,
        listen_port,
    })
}


fn build_unix_socket_config(env: &environment::Environment) -> Result<run::UnixStreamConfig, environment::Error> {
    let path = environment::lookup(env, "UNIX_SOCKET_PATH")?;
    let read_timeout = environment::lookup(env, "UNIX_SOCKET_READ_TIMEOUT")?;
    let write_timeout = environment::lookup(env, "UNIX_SOCKET_WRITE_TIMEOUT")?;

    Ok(run::UnixStreamConfig{
        path,
        read_timeout: Duration::from_secs(read_timeout),
        write_timeout: Duration::from_secs(write_timeout),
    })
}



// TODO: remove struct, this service should just proxy json from input
#[derive(Serialize)]
struct Payload {
    language: String,
    files: Vec<File>,
    stdin: String,
    command: String,
}

#[derive(Serialize)]
struct File {
    name: String,
    content: String,
}
