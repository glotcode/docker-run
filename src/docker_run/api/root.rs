use crate::docker_run::api;

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

#[derive(Debug, serde::Serialize)]
struct ServiceInfo {
    name: String,
    version: String,
    description: String,
}

pub fn handle() -> Result<api::SuccessResponse, api::ErrorResponse> {
    api::prepare_json_response(
        &ServiceInfo {
            name: "docker-run".to_string(),
            version: VERSION.unwrap_or("unknown").to_string(),
            description: "Api for running code in transient docker containers".to_string(),
        },
        api::JsonFormat::Pretty,
    )
}
