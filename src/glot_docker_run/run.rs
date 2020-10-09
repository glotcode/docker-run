use std::os::unix::net::UnixStream;
use http::{Request, Response, StatusCode, HeaderValue};
use http::header;
use std::io::{Read, Write};
use std::time::Duration;
use httparse;
use std::str;
use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;
use serde_json::{Value, Map};
use serde_json;

use crate::glot_docker_run::docker;
use crate::glot_docker_run::http_extra;


pub fn run<Stream, Payload>(mut stream: Stream, config: &docker::ContainerConfig, payload: &Payload)
    where
        Stream: Read + Write,
        Payload: Serialize,
    {

    let container_response = docker::create_container(&mut stream, config).unwrap();

    let mut start_stream = UnixStream::connect("/Users/pii/Library/Containers/com.docker.docker/Data/docker.raw.sock").unwrap();
    let containerId = &container_response.body().id;
    docker::start_container(&mut start_stream, &containerId);

    let mut attach_stream = UnixStream::connect("/Users/pii/Library/Containers/com.docker.docker/Data/docker.raw.sock").unwrap();
    let result = docker::attach_and_send_payload(&mut attach_stream, &containerId, payload);
    println!("{:?}", result);
}
