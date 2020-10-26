#![allow(warnings)]

mod docker_run;

use std::io;
use std::process;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use serde::Serialize;
use serde_json::{Value, Map};
use serde_json;
use tiny_http;


use docker_run::docker;
use docker_run::run;
use docker_run::config;
use docker_run::environment;
use docker_run::api;


fn main() {
    env_logger::init();

    match start() {
        Ok(()) => {}

        Err(Error::BuildConfig(err)) => {
            log::error!("Failed to build config: {}", err);
            process::exit(1)
        }

        Err(Error::StartServer(err)) => {
            log::error!("Failed to start server: {}", err);
            process::exit(1)
        }
    }
}

enum Error {
    BuildConfig(environment::Error),
    StartServer(io::Error),
}

fn start() -> Result<(), Error> {
    let env = environment::get_environment();
    let config = build_config(&env)
        .map_err(Error::BuildConfig)?;

    let server = tiny_http::Server::new(config.server.listen_addr_with_port())
        .map_err(Error::StartServer)?;

    let handles = (0..config.server.worker_threads).fold(Vec::new(), |mut acc, _| {
        let server = server.try_clone().unwrap();
        let config = config.clone();

        acc.push(thread::spawn(move || {
            loop {
                match server.accept() {
                    Ok(client) => {
                        for request in client {
                            handle_request(&config, request);
                        }
                    }

                    Err(err) => {
                        log::error!("Accept error: {:?}", err);
                        break;
                    }
                }
            }
        }));

        acc
    });


    log::info!("Listening on {} with {} worker threads", config.server.listen_addr_with_port(), config.server.worker_threads);

    // Wait for threads to complete, in practice this will block forever unless there is a panic
    for handle in handles {
        handle.join().unwrap();
    }

    Ok(())
}


fn handle_request(config: &config::Config, mut request: tiny_http::Request) {

    let handler = router(&request);

    match handler(config, &mut request) {
        Ok(data) => {
            success_response(request, &data)
        }

        Err(err) => {
            error_response(request, err)
        }
    }
}

fn router(request: &tiny_http::Request) -> fn(config: &config::Config, request: &mut tiny_http::Request) -> Result<Vec<u8>, api::Error> {
    match (request.method(), request.url()) {
        (tiny_http::Method::Get, "/") => {
            api::root::handle
        }

        (tiny_http::Method::Post, "/run") => {
            api::run::handle
        }

        _ => {
            api::not_found::handle
        }
    }
}

fn success_response(mut request: tiny_http::Request, data: &[u8]) {
    let response = tiny_http::Response::new(
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

fn error_response(mut request: tiny_http::Request, error: api::Error) {
    let response = tiny_http::Response::new(
        tiny_http::StatusCode(error.status_code),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
        ],
        error.body.as_slice(),
        Some(error.body.len()),
        None,
    );

    request.respond(response);
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
    let worker_threads = environment::lookup(env, "SERVER_WORKER_THREADS")?;

    Ok(config::ServerConfig{
        listen_addr,
        listen_port,
        worker_threads,
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
