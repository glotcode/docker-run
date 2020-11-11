pub mod root;
pub mod run;
pub mod version;
pub mod not_found;

use std::io;
use std::fmt;
use std::thread;



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

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub access_token: ascii::AsciiString,
}

fn check_access_token(config: &ApiConfig, request: &tiny_http::Request) -> Result<(), ErrorResponse> {
    let is_allowed = request.headers().iter()
        .filter(|header| header.field.equiv("X-Access-Token"))
        .map(|header| header.value.clone())
        .any(|value| value == config.access_token);

    if is_allowed {
        Ok(())
    } else {
        Err(authorization_error())
    }
}

pub fn authorization_error() -> ErrorResponse {
    ErrorResponse{
        status_code: 401,
        body: ErrorBody{
            error: "access_token".to_string(),
            message: "Missing or wrong access token".to_string(),
        }
    }
}

pub fn read_json_body<T: serde::de::DeserializeOwned>(request: &mut tiny_http::Request) -> Result<T, ErrorResponse> {
    serde_json::from_reader(request.as_reader())
        .map_err(|err| ErrorResponse{
            status_code: 400,
            body: ErrorBody{
                error: "request.parse".to_string(),
                message: format!("Failed to parse json from request: {}", err),
            }
        })
}

pub struct SuccessResponse {
    status_code: u16,
    body: Vec<u8>,
}


pub fn prepare_json_response<T: serde::Serialize>(body: &T) -> Result<SuccessResponse, ErrorResponse> {
    match serde_json::to_vec_pretty(body) {
        Ok(data) => {
            Ok(SuccessResponse{
                status_code: 200,
                body: data,
            })
        }

        Err(err) => {
            Err(ErrorResponse{
                status_code: 500,
                body: ErrorBody{
                    error: "response.serialize".to_string(),
                    message: format!("Failed to serialize response: {}", err),
                }
            })
        }
    }
}


pub fn success_response(request: tiny_http::Request, data: &SuccessResponse) -> Result<(), io::Error> {
    let body = data.body.as_slice();

    let response = tiny_http::Response::new(
        tiny_http::StatusCode(data.status_code),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
        ],
        body,
        Some(body.len()),
        None,
    );

    request.respond(response)
}

pub fn error_response(request: tiny_http::Request, error: ErrorResponse) -> Result<(), io::Error> {
    let error_body = serde_json::to_vec_pretty(&error.body)
        .unwrap_or_else(|_| b"Failed to serialize error body".to_vec());

    let response = tiny_http::Response::new(
        tiny_http::StatusCode(error.status_code),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
        ],
        error_body.as_slice(),
        Some(error_body.len()),
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
    pub body: ErrorBody,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: String,
    pub message: String,
}

