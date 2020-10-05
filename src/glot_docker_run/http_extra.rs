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
use std::io;
use serde_json::{Value, Map};


pub enum Body {
    Empty(),
    Bytes(Vec<u8>),
}



pub fn send_request<T: Read + Write>(mut stream: T, req: Request<Body>) -> Result<Response<Map<String, Value>>, io::Error> {
    let head = format!("{} {} {:?}", req.method().as_str(), req.uri().path(), req.version());

    let headers = req.headers()
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value.to_str().unwrap()))
        .collect::<Vec<String>>();

    match req.body() {
        Body::Empty() => {
            write!(stream, "{}\r\n{}\r\n\r\n\r\n", head, headers.join("\r\n"))
        },

        Body::Bytes(body) => {
            write!(stream, "{}\r\n{}\r\n\r\n", head, headers.join("\r\n"));
            stream.write_all(body)
        },
    }?;

    let mut resp_bytes = Vec::new();
    stream.read_to_end(&mut resp_bytes)?;

    let resp = parse_response(resp_bytes).unwrap();
    Ok(resp)
}

pub fn request_to_string<T>(req: Request<T>) -> String {
    let head = format!("{} {} {:?}", req.method().as_str(), req.uri().path(), req.version());

    let headers = req.headers()
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value.to_str().unwrap()))
        .collect::<Vec<String>>();

    format!("{}\r\n{}\r\n\r\n\r\n", head, headers.join("\r\n"))
}

#[derive(Debug)]
pub enum ParseError {
    Parse(httparse::Error),
    PartialParse(),
}

pub fn parse_response<T: DeserializeOwned>(bytes: Vec<u8>) -> Result<Response<T>, ParseError> {
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
