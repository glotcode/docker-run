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
use serde_json;

use glot_docker_run::docker;
use glot_docker_run::http_extra;


fn main() {
    //let path = "/containers/0b0e9fc7eeb667dcde1f446373e3e5a09993cd4e27e0f8ace94e8ade065d4a25/stats";
    let path = "/version";

    let req = Request::get(path)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Content-Length", 0)
        .header("Connection", "close")
        .body(())
        .unwrap();

    let config = docker::default_container_config("glot/bash:latest".to_string());
    docker::create_container(&config);

    let req_str = http_extra::request_to_string(req);

    let mut stream = UnixStream::connect("/Users/pii/Library/Containers/com.docker.docker/Data/docker.raw.sock").unwrap();

    stream.write_all(req_str.as_bytes()).unwrap();
    let mut resp_bytes = Vec::new();
    stream.set_read_timeout(Some(Duration::new(10, 0)));
    stream.read_to_end(&mut resp_bytes).unwrap();
    let resp : Result<Response<DockerVersion>, http_extra::ParseError> = http_extra::parse_response(resp_bytes);

    println!("{:?}", resp);
}


#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct DockerVersion {
    version: String,
    api_version: String,
    kernel_version: String,
}
