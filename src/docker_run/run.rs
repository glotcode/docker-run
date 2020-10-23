use std::os::unix::net::UnixStream;
use std::io;
use std::io::{Read, Write};
use std::time::Duration;
use std::str;
use std::fmt;
use serde::Serialize;
use serde_json::{Value, Map};
use std::net::Shutdown;
use std::path::PathBuf;
use crate::docker_run::docker;


#[derive(Debug)]
pub enum Error {
    Connect(io::Error),
    SetStreamTimeout(io::Error),
    CreateContainer(docker::Error),
    StartContainer(docker::Error),
    AttachContainer(docker::Error),
    SerializePayload(serde_json::Error),
    ReadStream(docker::StreamError),
    StreamStdinUnexpected(Vec<u8>),
    StreamStderr(Vec<u8>),
    StreamStdoutDecode(serde_json::Error),
}




impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Connect(err) => {
                write!(f, "Failed to connect to docker unix socket: {}", err)
            }

            Error::SetStreamTimeout(err) => {
                write!(f, "Failed set timeout on unix socket: {}", err)
            }

            Error::CreateContainer(err) => {
                write!(f, "Failed to create container: {}", err)
            }

            Error::StartContainer(err) => {
                write!(f, "Failed to start container: {}", err)
            }

            Error::AttachContainer(err) => {
                write!(f, "Failed to attach to container: {}", err)
            }

            Error::SerializePayload(err) => {
                write!(f, "Failed to send payload to stream: {}", err)
            }

            Error::ReadStream(err) => {
                write!(f, "Failed while reading stream: {}", err)
            }

            Error::StreamStdinUnexpected(bytes) => {
                let msg = String::from_utf8(bytes.to_vec())
                    .unwrap_or(format!("{:?}", bytes));

                write!(f, "Code runner returned unexpected stdin data: {}", msg)
            }

            Error::StreamStderr(bytes) => {
                let msg = String::from_utf8(bytes.to_vec())
                    .unwrap_or(format!("{:?}", bytes));

                write!(f, "Code runner failed with the following message: {}", msg)
            }

            Error::StreamStdoutDecode(err) => {
                write!(f, "Failed to decode json returned from code runner: {}", err)
            }
        }
    }
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



pub fn run<T: Serialize>(stream_config: UnixStreamConfig, run_request: RunRequest<T>) -> Result<Map<String, Value>, Error> {
    let container_response = with_unixstream(&stream_config, |stream| {
        docker::create_container(stream, &run_request.container_config)
            .map_err(Error::CreateContainer)
    })?;

    let container_id = &container_response.body().id;

    let result = run_with_container(&stream_config, run_request, &container_id);

    let _ = with_unixstream(&stream_config, |stream| {
        match docker::remove_container(stream, &container_id) {
            Ok(_) => {}

            Err(err) => {
                log::error!("Failed to remove container: {}", err);
            }
        }

        Ok(())
    });

    result
}

pub fn run_with_container<T: Serialize>(stream_config: &UnixStreamConfig, run_request: RunRequest<T>, container_id: &str) -> Result<Map<String, Value>, Error> {

    with_unixstream(&stream_config, |stream| {
        docker::start_container(stream, &container_id)
            .map_err(Error::StartContainer)
    })?;

    let run_config = UnixStreamConfig{
        read_timeout: run_request.limits.max_execution_time,
        ..stream_config.clone()
    };

    with_unixstream(&run_config, |stream| {
        run_code(stream, &container_id, &run_request)
    })
}

pub fn run_code<Stream, Payload>(mut stream: Stream, container_id: &str, run_request: &RunRequest<Payload>) -> Result<Map<String, Value>, Error>
    where
        Stream: Read + Write,
        Payload: Serialize,
    {

    docker::attach_container(&mut stream, container_id)
        .map_err(Error::AttachContainer)?;

    // Send payload
    serde_json::to_writer(&mut stream, &run_request.payload)
        .map_err(Error::SerializePayload)?;

    // Read response
    let output = docker::read_stream(stream, run_request.limits.max_output_size)
        .map_err(Error::ReadStream)?;

    // Return error if we recieved stdin or stderr data from the stream
    err_if_false(output.stdin.is_empty(), Error::StreamStdinUnexpected(output.stdin))?;
    err_if_false(output.stderr.is_empty(), Error::StreamStderr(output.stderr))?;


    // Decode stdout data to dict
    match decode_dict(&output.stdout) {
        Ok(json_dict) => {
            Ok(json_dict)
        }

        Err(err) => {
            Err(Error::StreamStdoutDecode(err))
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

    let _ = stream.shutdown(Shutdown::Both);

    Ok(result)
}


fn decode_dict(data: &[u8]) -> Result<Map<String, Value>, serde_json::Error> {
    serde_json::from_slice(data)
}


fn err_if_false<E>(value: bool, err: E) -> Result<(), E> {
    if value {
        Ok(())
    } else {
        Err(err)
    }
}
