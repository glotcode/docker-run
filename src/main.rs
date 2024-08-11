mod docker_run;

use std::process;
use std::time::Duration;

use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::App;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web::HttpServer;
use actix_web::{get, post, web};

use docker_run::api;
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
    req_body: web::Json<api::run::RequestBody>,
    config: web::Data<config::Config>,
) -> HttpResponse {
    if !has_valid_access_token(&req, &config) {
        prepare_error_response(api::authorization_error())
    } else {
        api::run::handle(&config, req_body.into_inner())
            .map(prepare_success_response)
            .unwrap_or_else(prepare_error_response)
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

    Ok(config::Config {
        server,
        api,
        unix_socket,
        container,
        run,
        debug,
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
