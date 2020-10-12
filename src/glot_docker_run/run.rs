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
    AttachStream(docker::StreamError),
}



pub fn run<Payload: Serialize>(path: &Path, config: &docker::ContainerConfig, payload: &Payload) -> Result<(), Error> {

    let container_response = with_unixstream(&path, |stream| {
        docker::create_container(stream, config)
            .map_err(Error::CreateContainer)
    })?;

    let containerId = &container_response.body().id;

    with_unixstream(&path, |stream| {
        docker::start_container(stream, &containerId)
            .map_err(Error::StartContainer)
    })?;

    let result = with_unixstream(&path, |stream| {
        docker::attach_and_send_payload(stream, &containerId, payload)
            .map_err(Error::AttachStream)
    })?;


    println!("{:?}", result);

    Ok(())
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
