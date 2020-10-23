use serde::Serialize;
use serde_json::{Value, Map};
use std::time::Duration;

use crate::docker_run::docker;
use crate::docker_run::run;
use crate::docker_run::config;

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



pub struct Error {
    pub status_code: u16,
    pub message: String,
}



pub fn handle(config: &config::Config, request: &mut tiny_http::Request) -> Result<Vec<u8>, Error> {

    let reader = request.as_reader();

    let run_request: RunRequest = serde_json::from_reader(reader)
        .map_err(|err| Error{
            status_code: 400,
            message: err.to_string()
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
            Ok(serde_json::to_vec(&data).unwrap())
        }

        Err(err) => {
            Err(Error{
                status_code: 400,
                message: "TODO".to_string(),
            })
        }
    }
}
