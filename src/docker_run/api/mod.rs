pub mod root;
pub mod run;
pub mod version;

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub access_token: String,
}

pub fn authorization_error() -> ErrorResponse {
    ErrorResponse {
        status_code: 401,
        body: ErrorBody {
            error: "access_token".to_string(),
            message: "Missing or wrong access token".to_string(),
        },
    }
}

pub struct SuccessResponse {
    pub status_code: u16,
    pub body: Vec<u8>,
}

pub enum JsonFormat {
    Minimal,
    Pretty,
}

pub fn prepare_json_response<T: serde::Serialize>(
    body: &T,
    format: JsonFormat,
) -> Result<SuccessResponse, ErrorResponse> {
    let json_to_vec = match format {
        JsonFormat::Minimal => serde_json::to_vec,

        JsonFormat::Pretty => serde_json::to_vec_pretty,
    };

    match json_to_vec(body) {
        Ok(data) => Ok(SuccessResponse {
            status_code: 200,
            body: data,
        }),

        Err(err) => Err(ErrorResponse {
            status_code: 500,
            body: ErrorBody {
                error: "response.serialize".to_string(),
                message: format!("Failed to serialize response: {}", err),
            },
        }),
    }
}

#[derive(Debug)]
pub struct ErrorResponse {
    pub status_code: u16,
    pub body: ErrorBody,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: String,
    pub message: String,
}
