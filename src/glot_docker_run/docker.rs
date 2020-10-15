use http;
use crate::glot_docker_run::http_extra;
use serde::{Serialize, Deserialize};
use serde_json;
use std::io::{Read, Write};
use std::io;
use std::convert::TryInto;

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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Ulimit {
    pub name: String,
    pub soft: i64,
    pub hard: i64,
}

pub fn default_container_config(image_name: String) -> ContainerConfig {
    ContainerConfig{
        hostname: "glot-runner".to_string(),
        user: "glot".to_string(),
        attach_stdin: true,
        attach_stdout: true,
        attach_stderr: true,
        tty: false,
        open_stdin: true,
        stdin_once: true,
        //cmd: Vec<String>,
        //entrypoint: Vec<String>,
        image: image_name,
        network_disabled: true,
        host_config: HostConfig{
            memory: 500000000,
            privileged: false,
            cap_add: vec![],
            cap_drop: vec!["MKNOD".to_string()],
            ulimits: vec![
                Ulimit{
                    name: "nofile".to_string(),
                    soft: 90,
                    hard: 100,
                },
                Ulimit{
                    name: "nproc".to_string(),
                    soft: 90,
                    hard: 100,
                },
            ],
        },
    }
}

#[derive(Debug)]
pub enum Error {
    BuildRequest(BuildRequestError),
    SendRequest(http_extra::Error),
}

#[derive(Debug)]
pub enum BuildRequestError {
    Body(serde_json::Error),
    Request(http::Error),
}


#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct VersionResponse {
    pub version: String,
    pub api_version: String,
    pub kernel_version: String,
}

pub fn version_request() -> Result<http::Request<http_extra::Body>, http::Error> {
    http::Request::get("/version")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
}

pub fn version<Stream: Read + Write>(mut stream: Stream) -> Result<http::Response<VersionResponse>, Error> {
    let req = version_request()
        .map_err(|x| Error::BuildRequest(BuildRequestError::Request(x)))?;

    http_extra::send_request(stream, req)
        .map_err(Error::SendRequest)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerCreatedResponse {
    pub id: String,
    pub warnings: Vec<String>,
}

pub fn create_container_request(config: &ContainerConfig) -> Result<http::Request<http_extra::Body>, BuildRequestError> {
    let body = serde_json::to_vec(config)
        .map_err(BuildRequestError::Body)?;

    http::Request::post("/containers/create")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Content-Length", body.len())
        .header("Connection", "close")
        .body(http_extra::Body::Bytes(body))
        .map_err(BuildRequestError::Request)
}

pub fn create_container<Stream: Read + Write>(mut stream: Stream, config: &ContainerConfig) -> Result<http::Response<ContainerCreatedResponse>, Error> {
    let req = create_container_request(config)
        .map_err(Error::BuildRequest)?;

    http_extra::send_request(stream, req)
        .map_err(Error::SendRequest)
}


pub fn start_container_request(containerId: &str) -> Result<http::Request<http_extra::Body>, http::Error> {
    let url = format!("/containers/{}/start", containerId);

    http::Request::post(url)
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
}


pub fn start_container<Stream: Read + Write>(mut stream: Stream, containerId: &str) -> Result<http::Response<http_extra::EmptyResponse>, Error> {
    let req = start_container_request(containerId)
        .map_err(|x| Error::BuildRequest(BuildRequestError::Request(x)))?;

    http_extra::send_request(stream, req)
        .map_err(Error::SendRequest)
}

pub fn remove_container_request(containerId: &str) -> Result<http::Request<http_extra::Body>, http::Error> {
    let url = format!("/containers/{}?v=1&force=1", containerId);

    http::Request::delete(url)
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
}


pub fn remove_container<Stream: Read + Write>(mut stream: Stream, containerId: &str) -> Result<http::Response<http_extra::EmptyResponse>, Error> {
    let req = remove_container_request(containerId)
        .map_err(|x| Error::BuildRequest(BuildRequestError::Request(x)))?;

    http_extra::send_request(stream, req)
        .map_err(Error::SendRequest)
}

pub fn attach_container_request(containerId: &str) -> Result<http::Request<http_extra::Body>, http::Error> {
    let url = format!("/containers/{}/attach?stream=1&stdout=1&stdin=1&stderr=1", containerId);

    http::Request::post(url)
        .header("Host", "127.0.0.1")
        .body(http_extra::Body::Empty())
}

pub fn attach_container<Stream: Read + Write>(stream: Stream, containerId: &str) -> Result<http::Response<http_extra::EmptyResponse>, Error> {
    let req = attach_container_request(containerId)
        .map_err(|x| Error::BuildRequest(BuildRequestError::Request(x)))?;

    http_extra::send_request(stream, req)
        .map_err(Error::SendRequest)
}

#[derive(Debug)]
pub enum StreamError {
    Read(io::Error),
    ReadStreamType(io::Error),
    UnknownStreamType(u8),
    ReadStreamLength(io::Error),
    InvalidStreamLength(<usize as std::convert::TryFrom<u32>>::Error),
}


#[derive(Debug)]
pub struct StreamOutput {
    pub stdin: Vec<u8>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}


// TODO: add config for max read limit
pub fn read_stream<R: Read>(mut r: R) -> Result<StreamOutput, StreamError> {
    let mut reader = iowrap::Eof::new(r);
    let mut stdin = Vec::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    while !reader.eof().map_err(StreamError::Read)? {
        let stream_type = read_stream_type(&mut reader)?;
        let stream_length = read_stream_length(&mut reader)?;

        let mut buffer = vec![0u8; stream_length];
        reader.read_exact(&mut buffer)
            .map_err(StreamError::Read);

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
    }

    Ok(StreamOutput{
        stdin: stdin,
        stdout: stdout,
        stderr: stderr,
    })
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
    reader.read_exact(&mut buffer)
        .map_err(StreamError::ReadStreamType)?;

    StreamType::from_byte(buffer[0])
        .ok_or(StreamError::UnknownStreamType(buffer[0]))
}

fn read_stream_length<R: Read>(mut reader: R) -> Result<usize, StreamError> {
    let mut buffer = [0; 4];
    reader.read_exact(&mut buffer)
        .map_err(StreamError::ReadStreamLength)?;

    u32::from_be_bytes(buffer)
        .try_into()
        .map_err(StreamError::InvalidStreamLength)
}
