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
use std::thread::sleep;
use std::io::BufReader;
use std::io::BufRead;


pub enum Body {
    Empty(),
    Bytes(Vec<u8>),
}

pub type JsonDict = Map<String, Value>;

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


pub fn send_request<Stream: Read + Write, ResponseBody: DeserializeOwned>(mut stream: Stream, req: Request<Body>) -> Result<Response<ResponseBody>, io::Error> {
    let head = format!("{} {} {:?}", req.method().as_str(), req.uri().path_and_query().unwrap(), req.version());

    let headers = req.headers()
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value.to_str().unwrap()))
        .collect::<Vec<String>>();

    match req.body() {
        Body::Empty() => {
            write!(stream, "{}\n{}\n\n", head, headers.join("\r\n"))
        },

        Body::Bytes(body) => {
            write!(stream, "{}\n{}\n\n", head, headers.join("\r\n"));
            stream.write_all(body)
        },
    }?;

    let mut resp_bytes = Vec::new();
    stream.read_to_end(&mut resp_bytes)?;

    let resp = parse_response(resp_bytes).unwrap();
    Ok(resp)
}

pub fn send_attach_request<Stream: Read + Write>(mut stream: Stream, req: Request<Body>) -> Result<Response<EmptyResponse>, io::Error> {
    let head = format!("{} {} {:?}", req.method().as_str(), req.uri().path_and_query().unwrap(), req.version());

    let headers = req.headers()
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value.to_str().unwrap()))
        .collect::<Vec<String>>();

    match req.body() {
        Body::Empty() => {
            println!("{}\n{}\n\n", head, headers.join("\r\n"));
            write!(stream, "{}\n{}\n\n", head, headers.join("\r\n"))
        },

        Body::Bytes(body) => {
            write!(stream, "{}\n{}\n\n", head, headers.join("\r\n"));
            stream.write_all(body)
        },
    }?;

    let mut buffered_stream = BufReader::new(stream);
    let mut resp_bytes = Vec::new();

    for n in 0..20 {
        if resp_bytes.ends_with(&[0xD, 0xA, 0xD, 0xA]) {
            break;
        }

        buffered_stream.read_until(0xA, &mut resp_bytes);
    }

    println!("finished reading");
    println!("{:?}", resp_bytes.clone());
    println!("{}", String::from_utf8(resp_bytes.clone()).unwrap());
    let resp = parse_response_headers(resp_bytes).unwrap();

    let payload = serde_json::to_vec(&Payload{
        language: "bash".to_string(),
        files: vec![File{
            name: "main.sh".to_string(),
            content: "echo hello".to_string(),
        }],
        stdin: "".to_string(),
        command: "".to_string(),
    }).unwrap();

    println!("payload: {}", String::from_utf8(payload.clone()).unwrap());
    stream = buffered_stream.into_inner();
    stream.write_all(&payload);
    //write!(stream, "\n");
    //stream.flush();

    let mut run_result = Vec::new();
    // TODO: just read 8 bytes at the time
    stream.read_to_end(&mut run_result)?;
    // TODO: parse stream
    // [8]byte{STREAM_TYPE, 0, 0, 0, SIZE1, SIZE2, SIZE3, SIZE4}
    let foo = String::from_utf8(run_result).unwrap();
    println!("run_result: {}", foo);

    Ok(resp)
}


#[derive(Serialize)]
struct Payload {
    language: String,
    files: Vec<File>,
    stdin: String,
    command: String,
}

#[derive(Serialize)]
struct File {
    name: String,
    content: String,
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
            let response_body = serde_json::from_slice(&bytes[parsed_len..]).unwrap();
            Ok(to_http_response(resp, response_body))
        }

        Ok(httparse::Status::Partial) => {
            Err(ParseError::PartialParse())
        }

        Err(err) => {
            Err(ParseError::Parse(err))
        }
    }
}

pub fn parse_response_headers(bytes: Vec<u8>) -> Result<Response<EmptyResponse>, ParseError> {
    let mut headers = [httparse::EMPTY_HEADER; 30];
    let mut resp = httparse::Response::new(&mut headers);

    match resp.parse(&bytes) {
        Ok(httparse::Status::Complete(parsed_len)) => {
            Ok(to_http_response(resp, EmptyResponse{}))
        }

        Ok(httparse::Status::Partial) => {
            Err(ParseError::PartialParse())
        }

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
