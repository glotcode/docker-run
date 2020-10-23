#![allow(warnings)]

mod docker_run;

use std::process;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use serde::Serialize;
use serde_json::{Value, Map};
use serde_json;
use tiny_http::{Response, Server};


use docker_run::docker;
use docker_run::run;
use docker_run::config;
use docker_run::environment;
use docker_run::api;


fn main() {
    env_logger::init();
    let config = prepare_config();

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:8088").unwrap());
    println!("Now listening on port 8088");

    let max_threads = 15;

    let mut handles = Vec::new();


    for _ in 0..max_threads {
        let server = server.clone();
        let config = config.clone();

        handles.push(thread::spawn(move || {
            for request in server.incoming_requests() {
                handle_request(&config, request);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}


fn handle_request(config: &config::Config, mut request: tiny_http::Request) {

    let result = api::run::handle(config, &mut request);

    match result {
        Ok(data) => {
            success_response(request, &data)
        }

        Err(err) => {
            error_response(request, err)
        }
    }
}

fn success_response(mut request: tiny_http::Request, data: &[u8]) {
    let response = Response::new(
        tiny_http::StatusCode(200),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
        ],
        data,
        Some(data.len()),
        None,
    );

    request.respond(response);
}

fn error_response(mut request: tiny_http::Request, error: api::run::Error) {
    let data = error.message.as_bytes();

    let response = Response::new(
        tiny_http::StatusCode(error.status_code),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
        ],
        data,
        Some(data.len()),
        None,
    );

    request.respond(response);
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
