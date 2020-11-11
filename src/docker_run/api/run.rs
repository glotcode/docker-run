use serde_json::{Value, Map};

use crate::docker_run::docker;
use crate::docker_run::run;
use crate::docker_run::config;
use crate::docker_run::api;

#[derive(Debug, serde::Deserialize)]
struct RequestBody {
    image: String,
    payload: Map<String, Value>,
}


pub fn handle(config: &config::Config, request: &mut tiny_http::Request) -> Result<api::SuccessResponse, api::ErrorResponse> {
    api::check_access_token(&config.api, request)?;

    let req_body: RequestBody = api::read_json_body(request)?;
    let container_config = run::prepare_container_config(req_body.image, config.container.clone());

    let run_result = run::run(config.unix_socket.clone(), run::RunRequest{
        container_config,
        payload: req_body.payload,
        limits: config.run.clone(),
    }).map_err(handle_error)?;

    api::prepare_json_response(&run_result)
}

fn handle_error(err: run::Error) -> api::ErrorResponse {
    match &err {
        run::Error::UnixStream(_) => {
            error_response(&err, 500, "docker.unixsocket")
        }

        run::Error::CreateContainer(_) => {
            error_response(&err, 400, "docker.container.create")
        }

        run::Error::StartContainer(_) => {
            error_response(&err, 500, "docker.container.start")
        }

        run::Error::AttachContainer(_) => {
            error_response(&err, 500, "docker.container.attach")
        }

        run::Error::SerializePayload(_) => {
            error_response(&err, 400, "docker.container.stream.payload.serialize")
        }

        run::Error::ReadStream(stream_error) => {
            match stream_error {
                docker::StreamError::MaxExecutionTime() => {
                    error_response(&err, 400, "limits.execution_time")
                }

                docker::StreamError::MaxReadSize(_) => {
                    error_response(&err, 400, "limits.read.size")
                }

                _ => {
                    error_response(&err, 500, "docker.container.stream.read")
                }
            }
        }

        run::Error::StreamStdinUnexpected(_) => {
            error_response(&err, 500, "coderunner.stdin")
        }

        run::Error::StreamStderr(_) => {
            error_response(&err, 500, "coderunner.stderr")
        }

        run::Error::StreamStdoutDecode(_) => {
            error_response(&err, 500, "coderunner.stdout.decode")
        }
    }
}

fn error_response(err: &run::Error, status_code: u16, error_code: &str) -> api::ErrorResponse {
    api::ErrorResponse{
        status_code,
        body: api::ErrorBody{
            error: error_code.to_string(),
            message: err.to_string(),
        }
    }
}
