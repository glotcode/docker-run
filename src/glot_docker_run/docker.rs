use http;
use crate::glot_docker_run::http_extra;
use serde::{Serialize, Deserialize};
use serde_json;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerConfig {
    hostname: String,
    //domain_name: String,
    user: String,
    attach_stdin: bool,
    attach_stdout: bool,
    attach_stderr: bool,
    tty: bool,
    open_stdin: bool,
    stdin_once: bool,
    //cmd: Vec<String>,
    //entrypoint: Vec<String>,
    image: String,
    network_disabled: bool,
    host_config: HostConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct HostConfig {
    //cpu_shared: i32,
    memory: i64,
    privileged: bool,
    cap_add: Vec<String>,
    cap_drop: Vec<String>,
    ulimits: Vec<Ulimit>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Ulimit {
    name: String,
    soft: i64,
    hard: i64,
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


pub fn version() -> http::Request<http_extra::Body> {
    http::Request::get("/version")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
        .unwrap()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerCreatedResponse {
    id: String,
    warnings: Vec<String>,
}


pub fn create_container(config: &ContainerConfig) -> http::Request<http_extra::Body> {
    let body = serde_json::to_vec(config).unwrap();

    http::Request::post("/containers/create")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Content-Length", body.len())
        .header("Connection", "close")
        .body(http_extra::Body::Bytes(body))
        .unwrap()
}


pub fn start_container(containerId: &str) -> http::Request<http_extra::Body> {
    let url = format!("/containers/{}/start", containerId);

    http::Request::post(url)
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Connection", "close")
        .body(http_extra::Body::Empty())
        .unwrap()
}
