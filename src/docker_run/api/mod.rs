pub mod root;
pub mod run;
pub mod version;
pub mod not_found;

use std::io;
use std::fmt;
use std::thread;
use tiny_http;



pub struct Config<C, H> {
    pub listen_addr: String,
    pub worker_threads: u16,
    pub handler_config: C,
    pub handler: H,
}

pub fn start<C, H>(config: Config<C, H>) -> Result<(), Error>
    where
        C: Send + Clone + 'static,
        H: Send + Copy + 'static,
        H: FnOnce(&C, tiny_http::Request) {

    let server = tiny_http::Server::new(config.listen_addr)
        .map_err(Error::Bind)?;

    let mut handles = Vec::new();
    let request_handler = config.handler;

    for n in 0..config.worker_threads {
        let handler_config = config.handler_config.clone();
        let server = server.try_clone()
            .map_err(|err| Error::CloneServer(err, n))?;

        handles.push(thread::spawn(move || {
            loop {
                match server.accept() {
                    Ok(client) => {
                        for request in client {
                            request_handler(&handler_config, request);
                        }
                    }

                    Err(tiny_http::AcceptError::Accept(err)) => {
                        log::error!("Accept error on thread {}: {:?}", n, err);
                        break;
                    }

                    Err(tiny_http::AcceptError::ShuttingDown()) => {
                        log::info!("Thread {} shutting down", n);
                        break;
                    }
                }
            }
        }))
    }

    // Wait for threads to complete, in practice this will block forever unless there is a panic
    for handle in handles {
        handle.join().unwrap();
    }

    Ok(())
}


pub fn success_response(request: tiny_http::Request, data: &[u8]) -> Result<(), io::Error> {
    let response = tiny_http::Response::new(
        tiny_http::StatusCode(200),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
        ],
        data,
        Some(data.len()),
        None,
    );

    request.respond(response)
}

pub fn error_response(request: tiny_http::Request, error: ErrorResponse) -> Result<(), io::Error> {
    let response = tiny_http::Response::new(
        tiny_http::StatusCode(error.status_code),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
        ],
        error.body.as_slice(),
        Some(error.body.len()),
        None,
    );

    request.respond(response)
}


pub enum Error {
    Bind(io::Error),
    CloneServer(io::Error, u16),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Bind(err) => {
                write!(f, "Failed to bind: {}", err)
            }

            Error::CloneServer(err, n) => {
                write!(f, "Failed to clone server (n = {}): {}", n, err)
            }
        }
    }
}

#[derive(Debug)]
pub struct ErrorResponse {
    pub status_code: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: String,
    pub message: String,
}

