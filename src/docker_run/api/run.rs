use serde_json::{Value, Map};
use std::time::Duration;

use crate::docker_run::docker;
use crate::docker_run::run;
use crate::docker_run::config;
use crate::docker_run::api;

#[derive(Debug, serde::Deserialize)]
struct RunRequest {
    image: String,
    limits: RunLimits,
    payload: Map<String, Value>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunLimits {
    max_execution_time: u64,
    max_output_size: usize,
}



pub fn handle(config: &config::Config, request: &mut tiny_http::Request) -> Result<Vec<u8>, api::ErrorResponse> {

    let reader = request.as_reader();

    let run_request: RunRequest = serde_json::from_reader(reader)
        .map_err(|err| api::ErrorResponse{
            status_code: 400,
            body: serde_json::to_vec(&api::ErrorBody{
                error: "request.parse".to_string(),
                message: format!("Failed to parse json from request: {}", err),
            }).unwrap_or(err.to_string().as_bytes().to_vec())
        })?;

    let container_config = docker::default_container_config(run_request.image);

    let res = run::run(config.unix_socket.clone(), run::RunRequest{
        container_config,
        payload: run_request.payload,
        limits: run::Limits{
            max_execution_time: Duration::from_secs(run_request.limits.max_execution_time),
            max_output_size: run_request.limits.max_output_size,
        },
    });

    match res {
        Ok(data) => {
            serde_json::to_vec(&data).map_err(|err| {
                api::ErrorResponse{
                    status_code: 400,
                    body: serde_json::to_vec(&api::ErrorBody{
                        error: "response.serialize".to_string(),
                        message: format!("Failed to serialize response: {}", err),
                    }).unwrap_or(err.to_string().as_bytes().to_vec())
                }
            })
        }

        Err(err) => {
            Err(api::ErrorResponse{
                // TODO: set correct status code
                status_code: 400,
                body: serde_json::to_vec(&api::ErrorBody{
                    error: error_code(&err),
                    message: err.to_string(),
                }).unwrap_or(err.to_string().as_bytes().to_vec())
            })
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
