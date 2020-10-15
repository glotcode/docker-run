#![allow(dead_code)]

mod glot_docker_run;

use std::time::Duration;
use serde::Serialize;
use std::path::Path;

use glot_docker_run::docker;
use glot_docker_run::run;


fn main() {
    let payload = Payload{
        language: "bash".to_string(),
        files: vec![File{
            name: "main.sh".to_string(),
            content: "echo hello".to_string(),
        }],
        stdin: "".to_string(),
        command: "".to_string(),
    };

    let config = docker::default_container_config("glot/bash:latest".to_string());
    let path = Path::new("/Users/pii/Library/Containers/com.docker.docker/Data/docker.raw.sock");

    let unixstream_config = run::UnixStreamConfig{
        path: path.to_path_buf(),
        read_timeout: Duration::from_secs(3),
        write_timeout: Duration::from_secs(3),
    };

    let res = run::run(unixstream_config, run::RunRequest{
        container_config: config,
        payload,
        limits: run::Limits{
            max_execution_time: Duration::from_secs(30),
            max_output_size: 100000,
        },
    });
    println!("{:?}", res);
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
