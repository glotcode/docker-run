use http::{Request, Response, HeaderValue};
use http::header;
use std::io::{Read, Write};
use httparse;
use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;
use serde_json;
use serde_json::{Value, Map};
use std::io;
use std::io::BufReader;
use std::io::BufRead;
use std::convert::TryInto;
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

    let mut resp_bytes = Vec::new();
    stream.read_to_end(&mut resp_bytes)?;

    let resp = parse_response(resp_bytes).unwrap();
    Ok(resp)
}

pub fn open_raw_stream<Stream>(mut stream: Stream, req: Request<Body>) -> Result<Response<EmptyResponse>, io::Error>
    where
        Stream: Read + Write,
    {
    write_request_head(&mut stream, &req);
    write_request_body(&mut stream, &req);

    let response_headers = read_response_headers(&mut stream);
    let response = parse_response(response_headers).unwrap();

    Ok(response)
}


pub fn send_payload<Stream, Payload>(mut stream: Stream, payload: Payload) -> Result<StreamResult, StreamError>
    where
        Stream: Read + Write,
        Payload: Serialize,
    {

    let bytes = serde_json::to_vec(&payload).unwrap();

    stream.write_all(&bytes);

    read_stream(stream)
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

fn read_response_headers<R: Read>(mut reader: R) -> Vec<u8> {
    let mut buffered_reader = BufReader::new(reader);
    let mut response_headers = Vec::new();

    for n in 0..20 {
        if response_headers.ends_with(&[0xD, 0xA, 0xD, 0xA]) {
            break;
        }

        buffered_reader.read_until(0xA, &mut response_headers);
    }

    response_headers
}


#[derive(Debug)]
enum StreamType {
    Stdin(),
    Stdout(),
    Stderr(),
}

impl StreamType {
    fn from_byte(n: u8) -> Option<StreamType> {
        match n {
            0 => Some(StreamType::Stdin()),
            1 => Some(StreamType::Stdout()),
            2 => Some(StreamType::Stderr()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum StreamError {
    Read(io::Error),
}


type StreamResult = Result<Vec<u8>, Vec<u8>>;


fn read_stream<R: Read>(mut r: R) -> Result<StreamResult, StreamError> {
    let mut reader = iowrap::Eof::new(r);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    while !reader.eof().map_err(StreamError::Read)? {
        let stream_type = read_stream_type(&mut reader);
        let stream_length = read_stream_length(&mut reader);

        let mut buffer = vec![0u8; stream_length];
        reader.read_exact(&mut buffer);

        match stream_type {
            StreamType::Stdin() => {

            }

            StreamType::Stdout() => {
                stdout.append(&mut buffer);
            }

            StreamType::Stderr() => {
                stderr.append(&mut buffer);
            }
        }
    }

    if stderr.len() > 0 {
        Ok(Err(stderr))
    } else {
        Ok(Ok(stdout))
    }
}

fn read_stream_type<R: Read>(mut reader: R) -> StreamType {
    let mut buffer = [0; 4];
    reader.read_exact(&mut buffer);

    StreamType::from_byte(buffer[0]).unwrap()
}

fn read_stream_length<R: Read>(mut reader: R) -> usize {
    let mut buffer = [0; 4];
    reader.read_exact(&mut buffer);

    u32::from_be_bytes(buffer).try_into().unwrap()
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
