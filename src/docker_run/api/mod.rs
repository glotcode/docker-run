pub mod run;


#[derive(Debug)]
pub struct Error {
    pub status_code: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: String,
    pub message: String,
}
