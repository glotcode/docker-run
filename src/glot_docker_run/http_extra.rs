use http::{Request, Response, HeaderValue};
use http::header;
use http::header::CONTENT_LENGTH;
use http::response;
use std::io::{Read, Write};
use httparse;
use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;
use serde_json;
use serde_json::{Value, Map};
use std::io;
use std::io::BufReader;
use std::io::BufRead;
use iowrap;


pub enum Body {
    Empty(),
    Bytes(Vec<u8>),
}

pub type JsonDict = Map<String, Value>;


pub fn send_request<Stream, ResponseBody>(mut stream: Stream, req: Request<Body>) -> Result<Response<ResponseBody>, io::Error>
    where
        Stream: Read + Write,
        ResponseBody: DeserializeOwned,
    {
    write_request_head(&mut stream, &req);
    write_request_body(&mut stream, &req);

    let mut reader = BufReader::new(stream);

    let response_head = read_response_head(&mut reader);
    let response_parts = parse_response_head(response_head).unwrap();

    let empty_content_length = HeaderValue::from_static("0");
    let content_length_value = response_parts.headers.get(CONTENT_LENGTH).unwrap_or(&empty_content_length);
    let content_length = content_length_value.to_str().unwrap().parse().unwrap();

    let body : Result<_, io::Error> = if content_length > 0 {
        let mut buffer = vec![0u8; content_length];
        let res = reader.read_exact(&mut buffer);
        Ok(buffer)
    } else {
        Ok(vec![])
    };

    let response_body = serde_json::from_slice(&body.unwrap()).unwrap();

    Ok(Response::from_parts(response_parts, response_body))
}


#[derive(Debug)]
pub struct EmptyResponse {}

impl<'de> Deserialize<'de> for EmptyResponse {
    fn deserialize<D>(_: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(EmptyResponse{})
    }
}

pub fn format_request_line<T>(req: &Request<T>) -> String {
    format!("{} {} {:?}", req.method(), req.uri().path_and_query().unwrap(), req.version())
}

pub fn format_headers<T>(req: &Request<T>) -> String {
    req.headers()
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value.to_str().unwrap()))
        .collect::<Vec<String>>()
        .join("\r\n")
}

fn write_request_head<T, W: Write>(mut writer: W, req: &Request<T>) {
    let request_line = format_request_line(&req);
    write!(writer, "{}\r\n", request_line);

    let headers = format_headers(&req);
    write!(writer, "{}\r\n\r\n", headers);
}

fn write_request_body<W: Write>(mut writer: W, req: &Request<Body>) -> Result<(), io::Error>{
    match req.body() {
        Body::Empty() => {
            Ok(())
        }

        Body::Bytes(body) => {
            writer.write_all(body)
        }
    }
}

fn read_response_head<R: BufRead>(mut reader: R) -> Vec<u8> {
    let mut response_headers = Vec::new();

    for n in 0..20 {
        if response_headers.ends_with(&[0xD, 0xA, 0xD, 0xA]) {
            break;
        }

        reader.read_until(0xA, &mut response_headers);
    }

    response_headers
}



#[derive(Debug)]
pub enum ParseError {
    Parse(httparse::Error),
    PartialParse(),
}

pub fn parse_response_head(bytes: Vec<u8>) -> Result<response::Parts, ParseError> {
    println!("parse_response_head: {:#?}", String::from_utf8(bytes.clone()));
    let mut headers = [httparse::EMPTY_HEADER; 30];
    let mut resp = httparse::Response::new(&mut headers);

    match resp.parse(&bytes) {
        Ok(httparse::Status::Complete(_)) => {
            let parts = to_http_parts(resp);
            Ok(parts)
        }

        Ok(httparse::Status::Partial) => {
            Err(ParseError::PartialParse())
        }

        Err(err) => {
            Err(ParseError::Parse(err))
        }
    }
}

fn to_http_parts(parsed: httparse::Response) -> response::Parts {
    let mut response = Response::builder();
    let headers = response.headers_mut().unwrap();

    for header in parsed.headers.iter() {
        let header_name = header.name.parse::<header::HeaderName>().unwrap();
        let header_value = HeaderValue::from_bytes(header.value).unwrap();
        headers.insert(header_name, header_value);
    }

    response
        .status(parsed.code.unwrap_or(0))
        .body(())
        .unwrap()
        .into_parts()
        .0
}
