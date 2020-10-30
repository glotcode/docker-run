use serde_json::{Value, Map};

use crate::docker_run::docker;
use crate::docker_run::run;
use crate::docker_run::config;
use crate::docker_run::api;

#[derive(Debug, serde::Deserialize)]
struct RunRequest {
    image: String,
    payload: Map<String, Value>,
}


pub fn handle(config: &config::Config, request: &mut tiny_http::Request) -> Result<Vec<u8>, api::ErrorResponse> {
    api::check_access_token(&config.api, request)?;

    let reader = request.as_reader();

    let run_request: RunRequest = serde_json::from_reader(reader)
        .map_err(|err| api::ErrorResponse{
            status_code: 400,
            body: serde_json::to_vec(&api::ErrorBody{
                error: "request.parse".to_string(),
                message: format!("Failed to parse json from request: {}", err),
            }).unwrap_or_else(|_| err.to_string().as_bytes().to_vec())
        })?;

    let container_config = run::prepare_container_config(run_request.image, config.container.clone());

    let res = run::run(config.unix_socket.clone(), run::RunRequest{
        container_config,
        payload: run_request.payload,
        limits: config.run.clone(),
    });

    match res {
        Ok(data) => {
            serde_json::to_vec(&data).map_err(|err| {
                api::ErrorResponse{
                    status_code: 400,
                    body: serde_json::to_vec(&api::ErrorBody{
                        error: "response.serialize".to_string(),
                        message: format!("Failed to serialize response: {}", err),
                    }).unwrap_or_else(|_| err.to_string().as_bytes().to_vec())
                }
            })
        }

        Err(err) => {
            Err(api::ErrorResponse{
                status_code: status_code(&err),
                body: serde_json::to_vec(&api::ErrorBody{
                    error: error_code(&err),
                    message: err.to_string(),
                }).unwrap_or_else(|_| err.to_string().as_bytes().to_vec())
            })
        }
    }
}

pub fn status_code(error: &run::Error) -> u16 {
    match error {
        run::Error::UnixStream(_) => {
            500
        }

        run::Error::CreateContainer(_) => {
            400
        }

        run::Error::StartContainer(_) => {
            500
        }

        run::Error::AttachContainer(_) => {
            500
        }

        run::Error::SerializePayload(_) => {
            400
        }

        run::Error::ReadStream(stream_error) => {
            match stream_error {
                docker::StreamError::MaxExecutionTime() => {
                    400
                }

                docker::StreamError::MaxReadSize(_) => {
                    400
                }

                _ => {
                    500
                }
            }
        }

        run::Error::StreamStdinUnexpected(_) => {
            500
        }

        run::Error::StreamStderr(_) => {
            500
        }

        run::Error::StreamStdoutDecode(_) => {
            500
        }
    }
}

pub fn error_code(error: &run::Error) -> String {
    match error {
        run::Error::UnixStream(_) => {
            "docker.unixsocket".to_string()
        }

        run::Error::CreateContainer(_) => {
            "docker.container.create".to_string()
        }

        run::Error::StartContainer(_) => {
            "docker.container.start".to_string()
        }

        run::Error::AttachContainer(_) => {
            "docker.container.attach".to_string()
        }

        run::Error::SerializePayload(_) => {
            "docker.container.stream.payload.serialize".to_string()
        }

        run::Error::ReadStream(stream_error) => {
            match stream_error {
                docker::StreamError::MaxExecutionTime() => {
                    "limits.execution_time".to_string()
                }

                docker::StreamError::MaxReadSize(_) => {
                    "limits.read.size".to_string()
                }

                _ => {
                    "docker.container.stream.read".to_string()
                }
            }
        }

        run::Error::StreamStdinUnexpected(_) => {
            "coderunner.stdin".to_string()
        }

        run::Error::StreamStderr(_) => {
            "coderunner.stderr".to_string()
        }

        run::Error::StreamStdoutDecode(_) => {
            "coderunner.stdout.decode".to_string()
        }
    }
}
