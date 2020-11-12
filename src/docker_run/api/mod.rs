pub mod root;
pub mod run;
pub mod version;
pub mod not_found;

use std::io;
use std::fmt;
use std::thread;



pub struct ServerConfig<C, H> {
    pub listen_addr: String,
    pub worker_threads: u16,
    pub handler_config: C,
    pub handler: H,
}

pub struct Server {
    server: tiny_http::Server,
}

impl Server {
    pub fn new(listen_addr: String) -> Result<Server, io::Error> {
        let server = tiny_http::Server::new(listen_addr)?;

        Ok(Server{
            server,
        })
    }

    pub fn start<C, H>(&self, config: ServerConfig<C, H>) -> Result<Workers, Error>
        where
            C: Send + Clone + 'static,
            H: Send + Copy + 'static,
            H: FnOnce(&C, tiny_http::Request) {

        let mut handles = Vec::new();
        let request_handler = config.handler;

        for n in 0..config.worker_threads {
            let handler_config = config.handler_config.clone();
            let server = self.server.try_clone()
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

        Ok(Workers{
            handles
        })
    }
}

pub struct Workers {
    handles: Vec<thread::JoinHandle<()>>
}

impl Workers {
    pub fn wait(self) {
        // Wait for threads to complete, in practice this will block forever unless:
        // - The server is shutdown
        // - One of the threads panics
        for handle in self.handles {
            handle.join().unwrap();
        }
    }
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

pub enum JsonFormat {
    Minimal,
    Pretty,
}


pub fn prepare_json_response<T: serde::Serialize>(body: &T, format: JsonFormat) -> Result<SuccessResponse, ErrorResponse> {
    let json_to_vec = match format {
        JsonFormat::Minimal => {
            serde_json::to_vec
        }

        JsonFormat::Pretty => {
            serde_json::to_vec_pretty
        }
    };

    match json_to_vec(body) {
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
    CloneServer(io::Error, u16),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
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

