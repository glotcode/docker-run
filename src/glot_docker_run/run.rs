use std::os::unix::net::UnixStream;
use http::{Request, Response, StatusCode, HeaderValue};
use http::header;
use std::io;
use std::io::{Read, Write};
use std::time::Duration;
use httparse;
use std::str;
use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;
use serde_json::{Value, Map};
use serde_json;
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use crate::glot_docker_run::docker;
use crate::glot_docker_run::http_extra;


#[derive(Debug)]
pub enum Error {
    Connect(io::Error),
    SetStreamTimeout(io::Error),
    CreateContainer(docker::Error),
    StartContainer(docker::Error),
    AttachContainer(docker::Error),
    SerializePayload(serde_json::Error),
    ReadStream(docker::StreamError),
}


#[derive(Debug)]
pub struct RunRequest<Payload: Serialize> {
    pub container_config: docker::ContainerConfig,
    pub payload: Payload,
    pub limits: Limits,
}


#[derive(Debug)]
pub struct Limits {
    pub max_execution_time: Duration,
    pub max_output_size: usize,
}



pub fn run<T: Serialize>(stream_config: UnixStreamConfig, run_request: RunRequest<T>) -> Result<RunResult, Error> {
    let container_response = with_unixstream(&stream_config, |stream| {
        docker::create_container(stream, &run_request.container_config)
            .map_err(Error::CreateContainer)
    })?;

    let containerId = &container_response.body().id;

    let result = run_with_container(&stream_config, run_request, &containerId);

    with_unixstream(&stream_config, |stream| {
        let _ = docker::remove_container(stream, &containerId);
        Ok(())
    });

    result
}

pub fn run_with_container<T: Serialize>(stream_config: &UnixStreamConfig, run_request: RunRequest<T>, containerId: &str) -> Result<RunResult, Error> {

    with_unixstream(&stream_config, |stream| {
        docker::start_container(stream, &containerId)
            .map_err(Error::StartContainer)
    })?;

    let run_config = UnixStreamConfig{
        read_timeout: run_request.limits.max_execution_time,
        ..stream_config.clone()
    };

    with_unixstream(&run_config, |stream| {
        run_code(stream, &containerId, &run_request.payload)
    })
}

pub fn run_code<Stream, Payload>(mut stream: Stream, containerId: &str, payload: Payload) -> Result<RunResult, Error>
    where
        Stream: Read + Write,
        Payload: Serialize,
    {

    docker::attach_container(&mut stream, containerId)
        .map_err(Error::AttachContainer)?;

    // Send payload
    serde_json::to_writer(&mut stream, &payload)
        .map_err(Error::SerializePayload);

    // Read response
    let output = docker::read_stream(stream)
        .map_err(Error::ReadStream)?;

    Ok(run_result_from_stream_output(output))
}


#[derive(Debug)]
pub enum RunResult {
    Success(Map<String, Value>),
    Failure(RunFailure),
}

#[derive(Debug)]
pub enum RunFailure {
    UnexpectedStdin(Vec<u8>),
    UnexpectedStderr(Vec<u8>),
    StdoutDecode(serde_json::Error),
}

fn run_result_from_stream_output(output: docker::StreamOutput) -> RunResult {
    if output.stdin.len() > 0 {
        RunResult::Failure(RunFailure::UnexpectedStdin(output.stdin))
    } else if (output.stderr.len() > 0) {
        RunResult::Failure(RunFailure::UnexpectedStderr(output.stderr))
    } else {
        match decode_dict(&output.stdout) {
            Ok(json_dict) =>
                RunResult::Success(json_dict),

            Err(err) =>
                RunResult::Failure(RunFailure::StdoutDecode(err)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnixStreamConfig {
    pub path: PathBuf,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
}

fn with_unixstream<F, T>(config: &UnixStreamConfig, f: F) -> Result<T, Error>
    where
        F: FnOnce(&mut UnixStream) -> Result<T, Error> {

    let mut stream = UnixStream::connect(&config.path)
        .map_err(Error::Connect)?;

    stream.set_read_timeout(Some(config.read_timeout))
        .map_err(Error::SetStreamTimeout)?;

    stream.set_write_timeout(Some(config.write_timeout))
        .map_err(Error::SetStreamTimeout)?;

    let result = f(&mut stream)?;

    stream.shutdown(Shutdown::Both);

    Ok(result)
}


fn decode_dict(data: &[u8]) -> Result<Map<String, Value>, serde_json::Error> {
    serde_json::from_slice(data)
}
