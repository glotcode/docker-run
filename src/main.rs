mod docker_run;

use std::process;
use std::time::{Duration, Instant};

use actix_web::App;
use actix_web::HttpMessage;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web::HttpServer;
use actix_web::http::StatusCode;
use actix_web::http::header::ContentType;
use actix_web::{get, post, web};
use serde::Serialize;

use docker_run::api;
use docker_run::cleanup;
use docker_run::config;
use docker_run::debug;
use docker_run::environment;
use docker_run::run;
use docker_run::unix_stream;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let env = environment::get_environment();
    let config = prepare_config(&env);

    let listen_addr = config.server.listen_addr.clone();
    let listen_port = config.server.listen_port;
    let worker_threads = config.server.worker_threads;

    log::info!("Listening on {}:{}", listen_addr, listen_port,);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
            // Match actix-web's default Json extractor limit while parsing the
            // run request explicitly so deserialization can be measured.
            .app_data(web::PayloadConfig::new(2 * 1024 * 1024))
            .service(index_api)
            .service(version_api)
            .service(run_api)
    })
    .workers(worker_threads)
    .client_request_timeout(Duration::from_secs(60))
    .bind((listen_addr, listen_port))?
    .run()
    .await
}

#[get("/")]
async fn index_api() -> HttpResponse {
    api::root::handle()
        .map(prepare_success_response)
        .unwrap_or_else(prepare_error_response)
}

#[get("/version")]
async fn version_api(req: HttpRequest, config: web::Data<config::Config>) -> HttpResponse {
    if !has_valid_access_token(&req, &config) {
        prepare_error_response(api::authorization_error())
    } else {
        api::version::handle(&config)
            .map(prepare_success_response)
            .unwrap_or_else(prepare_error_response)
    }
}

#[post("/run")]
async fn run_api(
    req: HttpRequest,
    req_body: web::Bytes,
    config: web::Data<config::Config>,
) -> HttpResponse {
    let request_started_at = Instant::now();
    let mut measurements = run::Measurements::default();

    let parse_started_at = Instant::now();
    let parsed_body = if has_json_content_type(&req) {
        serde_json::from_slice::<api::run::RequestBody>(&req_body).map_err(|err| {
            api::ErrorResponse {
                status_code: 400,
                body: api::ErrorBody {
                    error: "request.deserialize".to_string(),
                    message: format!("Failed to deserialize request body: {err}"),
                },
            }
        })
    } else {
        Err(api::ErrorResponse {
            status_code: 400,
            body: api::ErrorBody {
                error: "request.content_type".to_string(),
                message: "Content-Type must be application/json or application/*+json".to_string(),
            },
        })
    };
    measurements.request_parse_us = Some(run::elapsed_us(parse_started_at));

    let image = parsed_body.as_ref().ok().map(|body| body.image.clone());
    let result = match parsed_body {
        Err(err) => Err(err),
        Ok(_) if !has_valid_access_token(&req, &config) => Err(api::authorization_error()),
        Ok(req_body) => api::run::handle(&config, req_body, &mut measurements),
    };

    let (response, status_code, error) = match result {
        Ok(data) => {
            let status_code = data.status_code;
            let started_at = Instant::now();
            let response = prepare_success_response(data);
            measurements.response_build_us = Some(
                measurements
                    .response_build_us
                    .unwrap_or_default()
                    .saturating_add(run::elapsed_us(started_at)),
            );
            (response, status_code, None)
        }
        Err(data) => {
            let status_code = data.status_code;
            let error = Some(data.body.error.clone());
            let started_at = Instant::now();
            let response = prepare_error_response(data);
            measurements.response_build_us = Some(run::elapsed_us(started_at));
            (response, status_code, error)
        }
    };

    measurements.total_us = run::elapsed_us(request_started_at);
    log_run_request(image, status_code, error, measurements);

    response
}

fn has_json_content_type(request: &HttpRequest) -> bool {
    request.mime_type().ok().flatten().is_some_and(|mime| {
        mime.subtype() == "json" || mime.suffix().is_some_and(|suffix| suffix == "json")
    })
}

#[derive(Serialize)]
struct RunRequestLog {
    event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    status_code: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    measurements: run::Measurements,
}

fn log_run_request(
    image: Option<String>,
    status_code: u16,
    error: Option<String>,
    measurements: run::Measurements,
) {
    let entry = RunRequestLog {
        event: "run_request",
        image,
        status_code,
        error,
        measurements,
    };

    match serde_json::to_string(&entry) {
        Ok(entry) => log::info!(target: "docker_run::request", "{entry}"),
        Err(err) => {
            log::error!(target: "docker_run::request", "Failed to serialize run request log: {err}")
        }
    }
}

fn prepare_success_response(data: api::SuccessResponse) -> HttpResponse {
    let status_code = StatusCode::from_u16(data.status_code).unwrap_or(StatusCode::OK);

    HttpResponse::build(status_code)
        .content_type(ContentType::json())
        .body(data.body)
}

fn prepare_error_response(data: api::ErrorResponse) -> HttpResponse {
    let status_code =
        StatusCode::from_u16(data.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let body = serde_json::to_vec_pretty(&data.body)
        .unwrap_or_else(|_| b"Failed to serialize error body".to_vec());

    HttpResponse::build(status_code)
        .content_type(ContentType::json())
        .body(body)
}

fn has_valid_access_token(request: &HttpRequest, config: &config::Config) -> bool {
    let access_token = request
        .headers()
        .get("X-Access-Token")
        .map(|token| token.to_str().unwrap_or(""));

    match access_token {
        Some(token) => token == config.api.access_token,
        None => false,
    }
}

fn prepare_config(env: &environment::Environment) -> config::Config {
    match build_config(env) {
        Ok(config) => config,

        Err(err) => {
            log::error!("Failed to build config: {}", err);
            process::exit(1)
        }
    }
}

fn build_config(env: &environment::Environment) -> Result<config::Config, environment::Error> {
    let server = build_server_config(env)?;
    let api = build_api_config(env)?;
    let unix_socket = build_unix_socket_config(env)?;
    let container = build_container_config(env)?;
    let run = build_run_config(env)?;
    let debug = build_debug_config(env)?;
    let cleanup_config = build_cleanup_config(env, !debug.keep_container)?;
    let cleanup = cleanup::start(unix_socket.clone(), cleanup_config);

    Ok(config::Config {
        server,
        api,
        unix_socket,
        container,
        run,
        debug,
        cleanup,
    })
}

fn build_cleanup_config(
    env: &environment::Environment,
    recover_stale: bool,
) -> Result<cleanup::Config, environment::Error> {
    let worker_threads = environment::lookup(env, "DOCKER_CLEANUP_WORKER_THREADS").unwrap_or(2);
    let io_timeout: u64 =
        environment::lookup(env, "DOCKER_CLEANUP_UNIX_SOCKET_TIMEOUT").unwrap_or(30);

    Ok(cleanup::Config {
        worker_threads,
        io_timeout: Duration::from_secs(io_timeout),
        recover_stale,
    })
}

fn build_server_config(
    env: &environment::Environment,
) -> Result<config::ServerConfig, environment::Error> {
    let listen_addr = environment::lookup(env, "SERVER_LISTEN_ADDR")?;
    let listen_port = environment::lookup(env, "SERVER_LISTEN_PORT")?;
    let worker_threads = environment::lookup(env, "SERVER_WORKER_THREADS")?;

    Ok(config::ServerConfig {
        listen_addr,
        listen_port,
        worker_threads,
    })
}

fn build_api_config(env: &environment::Environment) -> Result<api::ApiConfig, environment::Error> {
    let access_token = environment::lookup(env, "API_ACCESS_TOKEN")?;

    Ok(api::ApiConfig { access_token })
}

fn build_unix_socket_config(
    env: &environment::Environment,
) -> Result<unix_stream::Config, environment::Error> {
    let path = environment::lookup(env, "DOCKER_UNIX_SOCKET_PATH")?;
    let read_timeout = environment::lookup(env, "DOCKER_UNIX_SOCKET_READ_TIMEOUT")?;
    let write_timeout = environment::lookup(env, "DOCKER_UNIX_SOCKET_WRITE_TIMEOUT")?;

    Ok(unix_stream::Config {
        path,
        read_timeout: Duration::from_secs(read_timeout),
        write_timeout: Duration::from_secs(write_timeout),
    })
}

fn build_container_config(
    env: &environment::Environment,
) -> Result<run::ContainerConfig, environment::Error> {
    let hostname = environment::lookup(env, "DOCKER_CONTAINER_HOSTNAME")?;
    let user = environment::lookup(env, "DOCKER_CONTAINER_USER")?;
    let memory = environment::lookup(env, "DOCKER_CONTAINER_MEMORY")?;
    let network_disabled = environment::lookup(env, "DOCKER_CONTAINER_NETWORK_DISABLED")?;
    let ulimit_nofile_soft = environment::lookup(env, "DOCKER_CONTAINER_ULIMIT_NOFILE_SOFT")?;
    let ulimit_nofile_hard = environment::lookup(env, "DOCKER_CONTAINER_ULIMIT_NOFILE_HARD")?;
    let ulimit_nproc_soft = environment::lookup(env, "DOCKER_CONTAINER_ULIMIT_NPROC_SOFT")?;
    let ulimit_nproc_hard = environment::lookup(env, "DOCKER_CONTAINER_ULIMIT_NPROC_HARD")?;
    let cap_add = environment::lookup(env, "DOCKER_CONTAINER_CAP_ADD").unwrap_or_default();
    let cap_drop = environment::lookup(env, "DOCKER_CONTAINER_CAP_DROP").unwrap_or_default();
    let readonly_rootfs =
        environment::lookup(env, "DOCKER_CONTAINER_READONLY_ROOTFS").unwrap_or(false);
    let tmp_dir_path: Option<String> =
        environment::lookup_optional(env, "DOCKER_CONTAINER_TMP_DIR_PATH")?;
    let tmp_dir_options = environment::lookup(env, "DOCKER_CONTAINER_TMP_DIR_OPTIONS")
        .unwrap_or_else(|_| "rw,noexec,nosuid,size=65536k".to_string());
    let work_dir_path: Option<String> =
        environment::lookup_optional(env, "DOCKER_CONTAINER_WORK_DIR_PATH")?;
    let work_dir_options = environment::lookup(env, "DOCKER_CONTAINER_WORK_DIR_OPTIONS")
        .unwrap_or_else(|_| "rw,exec,nosuid,size=131072k".to_string());

    Ok(run::ContainerConfig {
        hostname,
        user,
        memory,
        network_disabled,
        ulimit_nofile_soft,
        ulimit_nofile_hard,
        ulimit_nproc_soft,
        ulimit_nproc_hard,
        cap_add: environment::space_separated_string(cap_add),
        cap_drop: environment::space_separated_string(cap_drop),
        readonly_rootfs,
        tmp_dir: tmp_dir_path.map(|path| run::Tmpfs {
            path,
            options: tmp_dir_options,
        }),
        work_dir: work_dir_path.map(|path| run::Tmpfs {
            path,
            options: work_dir_options,
        }),
    })
}

fn build_run_config(env: &environment::Environment) -> Result<run::Limits, environment::Error> {
    let max_execution_time = environment::lookup(env, "RUN_MAX_EXECUTION_TIME")?;
    let max_output_size = environment::lookup(env, "RUN_MAX_OUTPUT_SIZE")?;

    Ok(run::Limits {
        max_execution_time: Duration::from_secs(max_execution_time),
        max_output_size,
    })
}

fn build_debug_config(env: &environment::Environment) -> Result<debug::Config, environment::Error> {
    let keep_container = environment::lookup(env, "DEBUG_KEEP_CONTAINER").unwrap_or(false);

    Ok(debug::Config { keep_container })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_request_log_has_nested_measurements_and_is_one_line() {
        let entry = RunRequestLog {
            event: "run_request",
            image: Some("glot/python:latest".to_string()),
            status_code: 200,
            error: None,
            measurements: run::Measurements {
                request_parse_us: Some(12),
                total_us: 34,
                ..run::Measurements::default()
            },
        };

        let json = serde_json::to_string(&entry).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(!json.contains('\n'));
        assert_eq!(value["measurements"]["request_parse_us"], 12);
        assert_eq!(value["measurements"]["total_us"], 34);
        assert!(value["measurements"].get("container_create_us").is_none());
    }
}
