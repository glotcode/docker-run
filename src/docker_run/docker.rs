use crate::docker_run::http_extra;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::io;
use std::io::{Read, Write};

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerConfig {
    pub hostname: String,
    pub user: String,
    pub attach_stdin: bool,
    pub attach_stdout: bool,
    pub attach_stderr: bool,
    pub tty: bool,
    pub open_stdin: bool,
    pub stdin_once: bool,
    pub image: String,
    pub network_disabled: bool,
    pub host_config: HostConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct HostConfig {
    pub memory: i64,
    pub privileged: bool,
    pub cap_add: Vec<String>,
    pub cap_drop: Vec<String>,
    pub ulimits: Vec<Ulimit>,
    pub readonly_rootfs: bool,
    pub tmpfs: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Ulimit {
    pub name: String,
    pub soft: i64,
    pub hard: i64,
}

#[derive(Debug)]
pub enum Error {
    PrepareRequest(PrepareRequestError),
    SendRequest(http_extra::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::PrepareRequest(err) => {
                write!(f, "Failed to prepare request: {}", err)
            }

            Error::SendRequest(err) => {
                write!(f, "Failed while sending request: {}", err)
            }
        }
    }
}

#[derive(Debug)]
pub enum PrepareRequestError {
    SerializeBody(serde_json::Error),
    Request(http::Error),
}

impl fmt::Display for PrepareRequestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PrepareRequestError::SerializeBody(err) => {
                write!(f, "Failed to serialize request body: {}", err)
            }

            PrepareRequestError::Request(err) => {
                write!(f, "{}", err)
            }
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all(deserialize = "PascalCase"))]
#[serde(rename_all(serialize = "camelCase"))]
pub struct VersionResponse {
    pub version: String,
    pub api_version: String,
    pub git_commit: String,
    pub go_version: String,
    pub os: String,
    pub arch: String,
    pub kernel_version: String,
    pub build_time: String,
    pub platform: VersionPlatformResponse,
    pub components: Vec<VersionComponentResponse>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all(deserialize = "PascalCase"))]
#[serde(rename_all(serialize = "camelCase"))]
pub struct VersionPlatformResponse {
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all(deserialize = "PascalCase"))]
#[serde(rename_all(serialize = "camelCase"))]
pub struct VersionComponentResponse {
    pub name: String,
    pub version: String,
}

pub fn version_request() -> Result<http::Request<http_extra::Body>, http::Error> {
    http::Request::get("/version")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
}

pub fn version<Stream: Read + Write>(
    stream: Stream,
) -> Result<http::Response<VersionResponse>, Error> {
    let req =
        version_request().map_err(|x| Error::PrepareRequest(PrepareRequestError::Request(x)))?;

    http_extra::send_request(stream, req).map_err(Error::SendRequest)
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all(deserialize = "PascalCase"))]
#[serde(rename_all(serialize = "camelCase"))]
pub struct ContainerCreatedResponse {
    pub id: String,
    pub warnings: Vec<String>,
}

pub fn create_container_request(
    config: &ContainerConfig,
    container_name: &str,
) -> Result<http::Request<http_extra::Body>, PrepareRequestError> {
    let body = serde_json::to_vec(config).map_err(PrepareRequestError::SerializeBody)?;
    let url = format!("/containers/create?name={}", container_name);

    http::Request::post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Content-Length", body.len())
        .header("Connection", "close")
        .body(http_extra::Body::Bytes(body))
        .map_err(PrepareRequestError::Request)
}

pub fn create_container<Stream: Read + Write>(
    stream: Stream,
    config: &ContainerConfig,
    container_name: &str,
) -> Result<http::Response<ContainerCreatedResponse>, Error> {
    let req = create_container_request(config, container_name).map_err(Error::PrepareRequest)?;

    http_extra::send_request(stream, req).map_err(Error::SendRequest)
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all(deserialize = "PascalCase"))]
pub struct ContainerListItem {
    pub id: String,
    pub names: Vec<String>,
    pub state: String,
    pub created: i64,
}

pub fn list_containers_request() -> Result<http::Request<http_extra::Body>, http::Error> {
    http::Request::get("/containers/json?all=1")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
}

pub fn list_containers<Stream: Read + Write>(
    stream: Stream,
) -> Result<http::Response<Vec<ContainerListItem>>, Error> {
    let req = list_containers_request()
        .map_err(|x| Error::PrepareRequest(PrepareRequestError::Request(x)))?;

    http_extra::send_request(stream, req).map_err(Error::SendRequest)
}

pub fn start_container_request(
    container_id: &str,
) -> Result<http::Request<http_extra::Body>, http::Error> {
    let url = format!("/containers/{}/start", container_id);

    http::Request::post(url)
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
}

pub fn start_container<Stream: Read + Write>(
    stream: Stream,
    container_id: &str,
) -> Result<http::Response<http_extra::EmptyResponse>, Error> {
    let req = start_container_request(container_id)
        .map_err(|x| Error::PrepareRequest(PrepareRequestError::Request(x)))?;

    http_extra::send_request(stream, req).map_err(Error::SendRequest)
}

pub fn remove_container_request(
    container_id: &str,
) -> Result<http::Request<http_extra::Body>, http::Error> {
    let url = format!("/containers/{}?v=1&force=1", container_id);

    http::Request::delete(url)
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
}

pub fn remove_container<Stream: Read + Write>(
    stream: Stream,
    container_id: &str,
) -> Result<http::Response<http_extra::EmptyResponse>, Error> {
    let req = remove_container_request(container_id)
        .map_err(|x| Error::PrepareRequest(PrepareRequestError::Request(x)))?;

    http_extra::send_request(stream, req).map_err(Error::SendRequest)
}

pub fn is_not_found(err: &Error) -> bool {
    matches!(
        err,
        Error::SendRequest(http_extra::Error::BadStatus(status, _))
            if *status == http::StatusCode::NOT_FOUND
    )
}

pub fn attach_container_request(
    container_id: &str,
) -> Result<http::Request<http_extra::Body>, http::Error> {
    let url = format!(
        "/containers/{}/attach?stream=1&stdout=1&stdin=1&stderr=1",
        container_id
    );

    http::Request::post(url)
        .header("Host", "127.0.0.1")
        .body(http_extra::Body::Empty())
}

pub fn attach_container<Stream: Read + Write>(
    stream: Stream,
    container_id: &str,
) -> Result<http::Response<http_extra::EmptyResponse>, Error> {
    let req = attach_container_request(container_id)
        .map_err(|x| Error::PrepareRequest(PrepareRequestError::Request(x)))?;

    http_extra::send_request(stream, req).map_err(Error::SendRequest)
}

#[derive(Debug)]
pub enum StreamError {
    Read(io::Error),
    ReadStreamType(io::Error),
    UnknownStreamType(u8),
    ReadStreamLength(io::Error),
    InvalidStreamLength(<usize as std::convert::TryFrom<u32>>::Error),
    MaxExecutionTime(),
    MaxReadSize(usize),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StreamError::Read(err) => {
                write!(f, "{}", err)
            }

            StreamError::ReadStreamType(err) => {
                write!(f, "Failed to read stream type: {}", err)
            }

            StreamError::UnknownStreamType(stream_type) => {
                write!(f, "Unknown stream type: (type: {})", stream_type)
            }

            StreamError::ReadStreamLength(err) => {
                write!(f, "Failed to read stream length: {}", err)
            }

            StreamError::InvalidStreamLength(err) => {
                write!(f, "Failed to parse stream length: {}", err)
            }

            StreamError::MaxExecutionTime() => {
                write!(f, "Max execution time exceeded")
            }

            StreamError::MaxReadSize(max_size) => {
                write!(f, "Max output size exceeded ({} bytes)", max_size)
            }
        }
    }
}

#[derive(Debug)]
pub struct StreamOutput {
    pub stdin: Vec<u8>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn read_stream<R: Read>(r: R, max_read_size: usize) -> Result<StreamOutput, StreamError> {
    let mut reader = iowrap::Eof::new(r);
    let mut read_size = 0;
    let mut stdin = Vec::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    while !reader.eof().map_err(io_read_error_to_stream_error)? {
        let stream_type = read_stream_type(&mut reader)?;
        let stream_length = read_stream_length(&mut reader)?;

        let mut buffer = vec![0u8; stream_length];
        reader
            .read_exact(&mut buffer)
            .map_err(io_read_error_to_stream_error)?;

        match stream_type {
            StreamType::Stdin() => {
                stdin.append(&mut buffer);
            }

            StreamType::Stdout() => {
                stdout.append(&mut buffer);
            }

            StreamType::Stderr() => {
                stderr.append(&mut buffer);
            }
        }

        read_size += stream_length;

        err_if_false(
            read_size <= max_read_size,
            StreamError::MaxReadSize(max_read_size),
        )?;
    }

    Ok(StreamOutput {
        stdin,
        stdout,
        stderr,
    })
}

fn io_read_error_to_stream_error(err: io::Error) -> StreamError {
    if err.kind() == io::ErrorKind::WouldBlock {
        StreamError::MaxExecutionTime()
    } else {
        StreamError::Read(err)
    }
}

fn err_if_false<E>(value: bool, err: E) -> Result<(), E> {
    if value { Ok(()) } else { Err(err) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container_config() -> ContainerConfig {
        ContainerConfig {
            hostname: "glot".to_string(),
            user: "glot".to_string(),
            attach_stdin: true,
            attach_stdout: true,
            attach_stderr: true,
            tty: false,
            open_stdin: true,
            stdin_once: true,
            image: "glot/python:latest".to_string(),
            network_disabled: true,
            host_config: HostConfig {
                memory: 1_000_000,
                privileged: false,
                cap_add: vec![],
                cap_drop: vec![],
                ulimits: vec![],
                readonly_rootfs: true,
                tmpfs: HashMap::new(),
            },
        }
    }

    #[test]
    fn create_request_uses_the_recoverable_container_name() {
        let request = create_container_request(&container_config(), "docker-run-123").unwrap();

        assert_eq!(request.uri(), "/containers/create?name=docker-run-123");
    }

    #[test]
    fn not_found_is_recognized_as_already_removed() {
        let error = Error::SendRequest(http_extra::Error::BadStatus(
            http::StatusCode::NOT_FOUND,
            vec![],
        ));

        assert!(is_not_found(&error));
    }
}

#[derive(Debug)]
enum StreamType {
    Stdin(),
    Stdout(),
    Stderr(),
}

impl StreamType {
    fn from_byte(n: u8) -> Option<StreamType> {
        match n {
            0 => Some(StreamType::Stdin()),
            1 => Some(StreamType::Stdout()),
            2 => Some(StreamType::Stderr()),
            _ => None,
        }
    }
}

fn read_stream_type<R: Read>(mut reader: R) -> Result<StreamType, StreamError> {
    let mut buffer = [0; 4];
    reader
        .read_exact(&mut buffer)
        .map_err(StreamError::ReadStreamType)?;

    StreamType::from_byte(buffer[0]).ok_or(StreamError::UnknownStreamType(buffer[0]))
}

fn read_stream_length<R: Read>(mut reader: R) -> Result<usize, StreamError> {
    let mut buffer = [0; 4];
    reader
        .read_exact(&mut buffer)
        .map_err(StreamError::ReadStreamLength)?;

    u32::from_be_bytes(buffer)
        .try_into()
        .map_err(StreamError::InvalidStreamLength)
}
