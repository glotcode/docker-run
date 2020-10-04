use std::os::unix::net::UnixStream;
use http::{Request, Response, StatusCode, HeaderValue};
use http::header;
use std::io::{Read, Write};
use std::time::Duration;
use httparse;
use std::str;
use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;
use serde_json;


fn main() {
    //let path = "/containers/0b0e9fc7eeb667dcde1f446373e3e5a09993cd4e27e0f8ace94e8ade065d4a25/stats";
    let path = "/version";

    let req = Request::get(path)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Host", "127.0.0.1")
        .header("Content-Length", 0)
        .header("Connection", "close")
        .body(())
        .unwrap();


    let req_str = request_to_string(req);

    let mut stream = UnixStream::connect("/Users/pii/Library/Containers/com.docker.docker/Data/docker.raw.sock").unwrap();

    stream.write_all(req_str.as_bytes()).unwrap();
    let mut resp_bytes = Vec::new();
    stream.set_read_timeout(Some(Duration::new(10, 0)));
    stream.read_to_end(&mut resp_bytes).unwrap();
    let resp : Result<Response<DockerVersion>, ParseError> = parse(resp_bytes);

    println!("{:?}", resp);
}


#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct DockerVersion {
    version: String,
    api_version: String,
    kernel_version: String,
}

fn request_to_string<T>(req: Request<T>) -> String {
    let head = format!("{} {} {:?}", req.method().as_str(), req.uri().path(), req.version());

    let headers = req.headers()
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value.to_str().unwrap()))
        .collect::<Vec<String>>();

    format!("{}\r\n{}\r\n\r\n\r\n", head, headers.join("\r\n"))
}

#[derive(Debug)]
enum ParseError {
    Parse(httparse::Error),
    PartialParse(),
}

fn parse<T: DeserializeOwned>(bytes: Vec<u8>) -> Result<Response<T>, ParseError> {
    let mut headers = [httparse::EMPTY_HEADER; 30];
    let mut resp = httparse::Response::new(&mut headers);

    match resp.parse(&bytes) {
        Ok(httparse::Status::Complete(parsed_len)) => {
            let foo = serde_json::from_slice(&bytes[parsed_len..]).unwrap();
            Ok(to_http_response(resp, foo))
        }
        Ok(httparse::Status::Partial) => {
            Err(ParseError::PartialParse())
        },
        Err(err) => {
            Err(ParseError::Parse(err))
        }
    }
}



fn to_http_response<T>(parsed: httparse::Response, body: T) -> Response<T> {
    let mut response = Response::builder();
    let headers = response.headers_mut().unwrap();

    for header in parsed.headers.iter() {
        let header_name = header.name.parse::<header::HeaderName>().unwrap();
        let header_value = HeaderValue::from_bytes(header.value).unwrap();
        headers.insert(header_name, header_value);
    }

    response
        .status(parsed.code.unwrap_or(0))
        .body(body)
        .unwrap()
}
