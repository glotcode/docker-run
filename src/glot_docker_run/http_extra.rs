use http::{Request, Response};
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
use std::str::FromStr;
use iowrap;


pub enum Body {
    Empty(),
    Bytes(Vec<u8>),
}

pub type JsonDict = Map<String, Value>;


#[derive(Debug)]
pub enum Error {
    WriteRequest(io::Error),
    ReadResponse(io::Error),
    ParseResponse(ParseError),
}

pub fn send_request<Stream, ResponseBody>(mut stream: Stream, req: Request<Body>) -> Result<Response<ResponseBody>, Error>
    where
        Stream: Read + Write,
        ResponseBody: DeserializeOwned,
    {
    write_request_head(&mut stream, &req)
        .map_err(Error::WriteRequest)?;

    write_request_body(&mut stream, &req)
        .map_err(Error::WriteRequest)?;

    let mut reader = BufReader::new(stream);

    let response_head = read_response_head(&mut reader)
        .map_err(Error::ReadResponse)?;

    let response_parts = parse_response_head(response_head)
        .map_err(Error::ParseResponse)?;

    let empty_content_length = header::HeaderValue::from_static("0");
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
    let path = req.uri()
        .path_and_query()
        .map(|x| x.as_str())
        .unwrap_or("");

    format!("{} {} {:?}", req.method(), path, req.version())
}

pub fn format_headers<T>(req: &Request<T>) -> String {
    req.headers()
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value.to_str().unwrap()))
        .collect::<Vec<String>>()
        .join("\r\n")
}

fn write_request_head<T, W: Write>(mut writer: W, req: &Request<T>) -> Result<(), io::Error> {
    let request_line = format_request_line(&req);
    write!(writer, "{}\r\n", request_line)?;

    let headers = format_headers(&req);
    write!(writer, "{}\r\n\r\n", headers)
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

fn read_response_head<R: BufRead>(mut reader: R) -> Result<Vec<u8>, io::Error> {
    let mut response_headers = Vec::new();

    for n in 0..20 {
        if response_headers.ends_with(&[0xD, 0xA, 0xD, 0xA]) {
            break;
        }

        reader.read_until(0xA, &mut response_headers)?;
    }

    Ok(response_headers)
}



#[derive(Debug)]
pub enum ParseError {
    Parse(httparse::Error),
    Empty(),
    Partial(),
    Response(ResponseError),
}

pub fn parse_response_head(bytes: Vec<u8>) -> Result<response::Parts, ParseError> {
    let mut headers = [httparse::EMPTY_HEADER; 30];
    let mut resp = httparse::Response::new(&mut headers);

    match resp.parse(&bytes) {
        Ok(httparse::Status::Complete(_)) => {
            let parts = to_http_parts(resp)
                .map_err(ParseError::Response)?;
            Ok(parts)
        }

        Ok(httparse::Status::Partial) => {
            if bytes.len() == 0 {
                Err(ParseError::Empty())
            } else {
                Err(ParseError::Partial())
            }
        }

        Err(err) => {
            Err(ParseError::Parse(err))
        }
    }
}

#[derive(Debug)]
enum ResponseError {
    HeaderName(header::InvalidHeaderName),
    HeaderValue(header::InvalidHeaderValue),
    StatusCode(),
    Builder(http::Error),
}

fn to_http_parts(parsed: httparse::Response) -> Result<response::Parts, ResponseError> {
    let mut response = Response::builder();
    let headers = response.headers_mut().unwrap();

    for hdr in parsed.headers.iter() {
        let name = header::HeaderName::from_str(hdr.name)
            .map_err(ResponseError::HeaderName)?;

        let value = header::HeaderValue::from_bytes(hdr.value)
            .map_err(ResponseError::HeaderValue)?;

        headers.insert(name, value);
    }

    let code = parsed.code
        .ok_or(ResponseError::StatusCode())?;

    let foo = response.status(code).body(())
        .map_err(ResponseError::Builder)?;

    Ok(foo.into_parts().0)
}
