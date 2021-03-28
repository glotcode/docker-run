use std::time::Duration;
use std::str;
use std::fmt;
use serde::Serialize;
use serde_json::{Value, Map};
use std::os::unix::net::UnixStream;
use std::net;
use std::collections::HashMap;

use crate::docker_run::docker;
use crate::docker_run::unix_stream;
use crate::docker_run::debug;


#[derive(Debug)]
pub struct RunRequest<Payload: Serialize> {
    pub container_config: docker::ContainerConfig,
    pub payload: Payload,
    pub limits: Limits,
}


#[derive(Clone, Debug)]
pub struct Limits {
    pub max_execution_time: Duration,
    pub max_output_size: usize,
}



pub fn run<T: Serialize>(stream_config: unix_stream::Config, run_request: RunRequest<T>, debug: debug::Config) -> Result<Map<String, Value>, Error> {
    let container_response = unix_stream::with_stream(&stream_config, Error::UnixStream, |stream| {
        docker::create_container(stream, &run_request.container_config)
            .map_err(Error::CreateContainer)
    })?;

    let container_id = &container_response.body().id;

    let result = run_with_container(&stream_config, run_request, &container_id);

    if !debug.keep_container {
        let _ = unix_stream::with_stream(&stream_config, Error::UnixStream, |stream| {
            match docker::remove_container(stream, &container_id) {
                Ok(_) => {}

                Err(err) => {
                    log::error!("Failed to remove container: {}", err);
                }
            }

            Ok(())
        });
    }

    result
}

pub fn run_with_container<T: Serialize>(stream_config: &unix_stream::Config, run_request: RunRequest<T>, container_id: &str) -> Result<Map<String, Value>, Error> {

    unix_stream::with_stream(&stream_config, Error::UnixStream, |stream| {
        docker::start_container(stream, &container_id)
            .map_err(Error::StartContainer)
    })?;

    let run_config = unix_stream::Config{
        read_timeout: run_request.limits.max_execution_time,
        ..stream_config.clone()
    };

    unix_stream::with_stream(&run_config, Error::UnixStream, |stream| {
        run_code(stream, &container_id, &run_request)
    })
}

pub fn run_code<Payload>(mut stream: &UnixStream, container_id: &str, run_request: &RunRequest<Payload>) -> Result<Map<String, Value>, Error>
    where
        Payload: Serialize,
    {

    docker::attach_container(&mut stream, container_id)
        .map_err(Error::AttachContainer)?;

    // Send payload
    serde_json::to_writer(&mut stream, &run_request.payload)
        .map_err(Error::SerializePayload)?;

    // Shutdown write stream which will trigger an EOF on the reader
    let _ = stream.shutdown(net::Shutdown::Write);

    // Read response
    let output = docker::read_stream(stream, run_request.limits.max_output_size)
        .map_err(Error::ReadStream)?;

    // Return error if we recieved stdin or stderr data from the stream
    err_if_false(output.stdin.is_empty(), Error::StreamStdinUnexpected(output.stdin))?;
    err_if_false(output.stderr.is_empty(), Error::StreamStderr(output.stderr))?;

    // Decode stdout data to dict
    decode_dict(&output.stdout)
        .map_err(Error::StreamStdoutDecode)
}


#[derive(Debug, Clone)]
pub struct ContainerConfig {
    pub hostname: String,
    pub user: String,
    pub memory: i64,
    pub network_disabled: bool,
    pub ulimit_nofile_soft: i64,
    pub ulimit_nofile_hard: i64,
    pub ulimit_nproc_soft: i64,
    pub ulimit_nproc_hard: i64,
    pub cap_add: Vec<String>,
    pub cap_drop: Vec<String>,
    pub readonly_rootfs: bool,
    pub tmp_dir: Option<Tmpfs>,
    pub work_dir: Option<Tmpfs>,
}

#[derive(Debug, Clone)]
pub struct Tmpfs {
    pub path: String,
    pub options: String,
}


impl ContainerConfig {
    pub fn tmpfs_mounts(&self) -> HashMap<String, String> {
        [&self.tmp_dir, &self.work_dir]
            .iter()
            .filter_map(|tmpfs| tmpfs.as_ref())
            .map(|tmpfs| (tmpfs.path.clone(), tmpfs.options.clone()))
            .collect()
    }
}


pub fn prepare_container_config(image_name: String, config: ContainerConfig) -> docker::ContainerConfig {
    let tmpfs = config.tmpfs_mounts();

    docker::ContainerConfig{
        hostname: config.hostname,
        user: config.user,
        attach_stdin: true,
        attach_stdout: true,
        attach_stderr: true,
        tty: false,
        open_stdin: true,
        stdin_once: true,
        image: image_name,
        network_disabled: config.network_disabled,
        host_config: docker::HostConfig{
            memory: config.memory,
            privileged: false,
            cap_add: config.cap_add,
            cap_drop: config.cap_drop,
            ulimits: vec![
                docker::Ulimit{
                    name: "nofile".to_string(),
                    soft: config.ulimit_nofile_soft,
                    hard: config.ulimit_nofile_hard,
                },
                docker::Ulimit{
                    name: "nproc".to_string(),
                    soft: config.ulimit_nproc_soft,
                    hard: config.ulimit_nproc_hard,
                },
            ],
            readonly_rootfs: config.readonly_rootfs,
            tmpfs,
        },
    }
}


#[derive(Debug)]
pub enum Error {
    UnixStream(unix_stream::Error),
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
            Error::UnixStream(err) => {
                write!(f, "Unix socket failure: {}", err)
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
