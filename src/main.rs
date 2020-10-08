#![allow(warnings)]

mod glot_docker_run;

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

use glot_docker_run::docker;
use glot_docker_run::http_extra;


fn main() {
    let mut stream = UnixStream::connect("/Users/pii/Library/Containers/com.docker.docker/Data/docker.raw.sock").unwrap();
    stream.set_read_timeout(Some(Duration::new(10, 0)));


    let config = docker::default_container_config("glot/bash:latest".to_string());
    //let create_container_req = docker::create_container(&config);
    //let resp : Result<Response<docker::ContainerCreatedResponse>, _>= http_extra::send_request(stream, create_container_req);


    //let start_container_req = docker::start_container("79c5f827cab3ebffcdbd1f210a9825402ebcb87eae14e51950a8972c446c622d");
    //let resp : Result<Response<http_extra::EmptyResponse>, _>= http_extra::send_request(stream, start_container_req);

    let attach_container_req = docker::attach_container("79c5f827cab3ebffcdbd1f210a9825402ebcb87eae14e51950a8972c446c622d");
    let resp : Result<Response<http_extra::EmptyResponse>, _>= http_extra::open_raw_stream(&stream, attach_container_req);

    println!("{:?}", resp);

    let payload = Payload{
        language: "bash".to_string(),
        files: vec![File{
            name: "main.sh".to_string(),
            content: "echo hello".to_string(),
        }],
        stdin: "".to_string(),
        command: "".to_string(),
    };

    let foo = http_extra::send_payload(&stream, payload);
    println!("{:?}", foo);
}

// TODO: remove struct, this service should just proxy json from input
#[derive(Serialize)]
struct Payload {
    language: String,
    files: Vec<File>,
    stdin: String,
    command: String,
}

#[derive(Serialize)]
struct File {
    name: String,
    content: String,
}



#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct DockerVersion {
    version: String,
    api_version: String,
    kernel_version: String,
}
