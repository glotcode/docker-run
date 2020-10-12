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
use std::path::Path;
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



pub fn run<Payload: Serialize>(path: &Path, config: &docker::ContainerConfig, payload: &Payload) -> Result<RunResult, Error> {

    let container_response = with_unixstream(&path, |stream| {
        docker::create_container(stream, config)
            .map_err(Error::CreateContainer)
    })?;

    let containerId = &container_response.body().id;

    with_unixstream(&path, |stream| {
        docker::start_container(stream, &containerId)
            .map_err(Error::StartContainer)
    })?;

    with_unixstream(&path, |stream| {
        run_code(stream, &containerId, payload)
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


fn with_unixstream<F, T>(path: &Path, f: F) -> Result<T, Error>
    where
        F: FnOnce(&mut UnixStream) -> Result<T, Error> {

    let mut stream = UnixStream::connect(path)
        .map_err(Error::Connect)?;

    // TODO: get timeout from config
    stream.set_read_timeout(Some(Duration::new(10, 0)))
        .map_err(Error::SetStreamTimeout)?;

    // TODO: get timeout from config
    stream.set_write_timeout(Some(Duration::new(10, 0)))
        .map_err(Error::SetStreamTimeout)?;

    let result = f(&mut stream)?;

    stream.shutdown(Shutdown::Both);

    Ok(result)
}


fn decode_dict(data: &[u8]) -> Result<Map<String, Value>, serde_json::Error> {
    serde_json::from_slice(data)
}
