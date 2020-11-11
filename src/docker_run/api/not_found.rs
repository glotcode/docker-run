use crate::docker_run::config;
use crate::docker_run::api;



pub fn handle(_: &config::Config, _: &mut tiny_http::Request) -> Result<api::SuccessResponse, api::ErrorResponse> {

    Err(api::ErrorResponse{
        status_code: 404,
        body: api::ErrorBody{
            error: "route.not_found".to_string(),
            message: "Route not found".to_string(),
        }
    })
}
