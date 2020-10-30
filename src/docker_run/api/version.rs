use std::fmt;

use crate::docker_run::config;
use crate::docker_run::api;
use crate::docker_run::docker;
use crate::docker_run::unix_stream;


#[derive(Debug, serde::Serialize)]
struct Response {
    docker: docker::VersionResponse,
}


pub fn handle(config: &config::Config, request: &mut tiny_http::Request) -> Result<Vec<u8>, api::ErrorResponse> {
    api::check_access_token(&config.api, request)?;

    match docker_version(&config.unix_socket) {
        Ok(data) => {
            serde_json::to_vec_pretty(&data).map_err(|err| {
                api::ErrorResponse{
                    status_code: 500,
                    body: serde_json::to_vec_pretty(&api::ErrorBody{
                        error: "response.serialize".to_string(),
                        message: format!("Failed to serialize response: {}", err),
                    }).unwrap_or_else(|_| err.to_string().as_bytes().to_vec())
                }
            })
        }

        Err(err) => {
            Err(api::ErrorResponse{
                status_code: 500,
                body: serde_json::to_vec_pretty(&api::ErrorBody{
                    error: error_code(&err),
                    message: err.to_string(),
                }).unwrap_or_else(|_| err.to_string().as_bytes().to_vec())
            })
        }
    }
}


fn docker_version(stream_config: &unix_stream::Config) -> Result<Response, Error> {
    let response = unix_stream::with_stream(&stream_config, Error::UnixStream, |stream| {
        docker::version(stream)
            .map_err(Error::Version)
    })?;

    Ok(Response{
        docker: response.body().clone(),
    })
}



pub enum Error {
    UnixStream(unix_stream::Error),
    Version(docker::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::UnixStream(err) => {
                write!(f, "Unix socket failure: {}", err)
            }

            Error::Version(err) => {
                write!(f, "Failed to get docker version: {}", err)
            }
        }
    }
}

pub fn error_code(error: &Error) -> String {
    match error {
        Error::UnixStream(_) => {
            "docker.unixsocket".to_string()
        }

        Error::Version(_) => {
            "docker.version".to_string()
        }
    }
}
