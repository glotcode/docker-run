use std::fmt;

use crate::docker_run::api;
use crate::docker_run::config;
use crate::docker_run::docker;
use crate::docker_run::unix_stream;

#[derive(Debug, serde::Serialize)]
struct VersionInfo {
    docker: docker::VersionResponse,
}

pub fn handle(config: &config::Config) -> Result<api::SuccessResponse, api::ErrorResponse> {
    let data = get_version_info(&config.unix_socket).map_err(handle_error)?;

    api::prepare_json_response(&data, api::JsonFormat::Pretty)
}

fn get_version_info(stream_config: &unix_stream::Config) -> Result<VersionInfo, Error> {
    let docker_response = unix_stream::with_stream(stream_config, Error::UnixStream, |stream| {
        docker::version(stream).map_err(Error::Version)
    })?;

    Ok(VersionInfo {
        docker: docker_response.body().clone(),
    })
}

fn handle_error(err: Error) -> api::ErrorResponse {
    match err {
        Error::UnixStream(_) => api::ErrorResponse {
            status_code: 500,
            body: api::ErrorBody {
                error: "docker.unixsocket".to_string(),
                message: err.to_string(),
            },
        },

        Error::Version(_) => api::ErrorResponse {
            status_code: 500,
            body: api::ErrorBody {
                error: "docker.version".to_string(),
                message: err.to_string(),
            },
        },
    }
}

pub enum Error {
    UnixStream(unix_stream::Error),
    Version(docker::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::UnixStream(err) => {
                write!(f, "Unix socket failure: {}", err)
            }

            Error::Version(err) => {
                write!(f, "Failed to get docker version: {}", err)
            }
        }
    }
}
