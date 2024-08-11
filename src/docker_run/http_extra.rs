use http::header;
use http::header::CONTENT_LENGTH;
use http::header::TRANSFER_ENCODING;
use http::response;
use http::status;
use http::{Request, Response};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::fmt;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::{Read, Write};
use std::str::FromStr;

const CARRIAGE_RETURN: u8 = 0xD;
const LINE_FEED: u8 = 0xA;

pub enum Body {
    Empty(),
    Bytes(Vec<u8>),
}

#[derive(Debug)]
pub enum Error {
    WriteRequest(io::Error),
    ReadResponse(io::Error),
    ParseResponseHead(ParseError),
    BadStatus(status::StatusCode, Vec<u8>),
    ReadChunkedBody(ReadChunkError),
    ReadBody(io::Error),
    DeserializeBody(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::WriteRequest(err) => {
                write!(f, "Failed to send request: {}", err)
            }

            Error::ReadResponse(err) => {
                write!(f, "Failed read response: {}", err)
            }

            Error::ParseResponseHead(err) => {
                write!(f, "Failed parse response head: {}", err)
            }

            Error::ReadChunkedBody(err) => {
                write!(f, "Failed read to chunked response body: {}", err)
            }

            Error::ReadBody(err) => {
                write!(f, "Failed read to response body: {}", err)
            }

            Error::BadStatus(status_code, body) => {
                let msg = String::from_utf8(body.to_vec()).unwrap_or(format!("{:?}", body));

                write!(f, "Unexpected status code {}: {}", status_code, msg)
            }

            Error::DeserializeBody(err) => {
                write!(f, "Failed deserialize response body: {}", err)
            }
        }
    }
}

pub fn send_request<Stream, ResponseBody>(
    mut stream: Stream,
    req: Request<Body>,
) -> Result<Response<ResponseBody>, Error>
where
    Stream: Read + Write,
    ResponseBody: DeserializeOwned,
{
    write_request_head(&mut stream, &req).map_err(Error::WriteRequest)?;

    write_request_body(&mut stream, &req).map_err(Error::WriteRequest)?;

    let mut reader = BufReader::new(stream);

    let response_head = read_response_head(&mut reader).map_err(Error::ReadResponse)?;

    let response_parts = parse_response_head(response_head).map_err(Error::ParseResponseHead)?;

    // Read response body
    let raw_body = match get_transfer_encoding(&response_parts.headers) {
        TransferEncoding::Chunked() => {
            read_chunked_response_body(reader).map_err(Error::ReadChunkedBody)?
        }

        _ => {
            let content_length = get_content_length(&response_parts.headers);
            read_response_body(content_length, reader).map_err(Error::ReadBody)?
        }
    };

    err_if_false(
        response_parts.status.is_success(),
        Error::BadStatus(response_parts.status, raw_body.clone()),
    )?;

    let body = serde_json::from_slice(&raw_body).map_err(Error::DeserializeBody)?;

    Ok(Response::from_parts(response_parts, body))
}

fn read_response_body<R: BufRead>(
    content_length: usize,
    mut reader: R,
) -> Result<Vec<u8>, io::Error> {
    if content_length > 0 {
        let mut buffer = vec![0u8; content_length];
        reader.read_exact(&mut buffer)?;
        Ok(buffer)
    } else {
        Ok(vec![])
    }
}

#[derive(Debug)]
pub enum ReadChunkError {
    ReadChunkLength(io::Error),
    ParseChunkLength(std::num::ParseIntError),
    ReadChunk(io::Error),
    SkipLineFeed(io::Error),
}

impl fmt::Display for ReadChunkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ReadChunkError::ReadChunkLength(err) => {
                write!(f, "Failed to read chunk length: {}", err)
            }

            ReadChunkError::ParseChunkLength(err) => {
                write!(f, "Failed parse chunk length: {}", err)
            }

            ReadChunkError::ReadChunk(err) => {
                write!(f, "Failed read chunk: {}", err)
            }

            ReadChunkError::SkipLineFeed(err) => {
                write!(f, "Failed read line feed at end of chunk: {}", err)
            }
        }
    }
}

fn read_chunked_response_body<R: BufRead>(mut reader: R) -> Result<Vec<u8>, ReadChunkError> {
    let mut body = vec![];

    loop {
        let mut chunk = read_response_chunk(&mut reader)?;

        if chunk.is_empty() {
            break;
        }

        body.append(&mut chunk)
    }

    Ok(body)
}

fn read_response_chunk<R: BufRead>(mut reader: R) -> Result<Vec<u8>, ReadChunkError> {
    let mut buffer = String::new();

    reader
        .read_line(&mut buffer)
        .map_err(ReadChunkError::ReadChunkLength)?;

    let chunk_length =
        usize::from_str_radix(buffer.trim_end(), 16).map_err(ReadChunkError::ParseChunkLength)?;

    let chunk = read_response_body(chunk_length, &mut reader).map_err(ReadChunkError::ReadChunk)?;

    let mut void = String::new();
    reader
        .read_line(&mut void)
        .map_err(ReadChunkError::SkipLineFeed)?;

    Ok(chunk)
}

fn get_content_length(headers: &header::HeaderMap<header::HeaderValue>) -> usize {
    headers
        .get(CONTENT_LENGTH)
        .map(|value| value.to_str().unwrap_or("").parse().unwrap_or(0))
        .unwrap_or(0)
}

enum TransferEncoding {
    NoEncoding(),
    Chunked(),
    Other(String),
}

impl TransferEncoding {
    pub fn from_str(value: &str) -> TransferEncoding {
        match value {
            "chunked" => TransferEncoding::Chunked(),

            "" => TransferEncoding::NoEncoding(),

            other => TransferEncoding::Other(other.to_string()),
        }
    }
}

fn get_transfer_encoding(headers: &header::HeaderMap<header::HeaderValue>) -> TransferEncoding {
    let value = headers
        .get(TRANSFER_ENCODING)
        .map(|value| value.to_str().unwrap_or("").to_string())
        .unwrap_or_else(|| "".to_string());

    TransferEncoding::from_str(&value)
}

#[derive(Debug)]
pub struct EmptyResponse {}

impl<'de> Deserialize<'de> for EmptyResponse {
    fn deserialize<D>(_: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(EmptyResponse {})
    }
}

pub fn format_request_line<T>(req: &Request<T>) -> String {
    let path = req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("");

    format!("{} {} {:?}", req.method(), path, req.version())
}

pub fn format_request_headers<T>(req: &Request<T>) -> String {
    req.headers()
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value.to_str().unwrap_or("")))
        .collect::<Vec<String>>()
        .join("\r\n")
}

fn write_request_head<T, W: Write>(mut writer: W, req: &Request<T>) -> Result<(), io::Error> {
    let request_line = format_request_line(&req);
    write!(writer, "{}\r\n", request_line)?;

    let headers = format_request_headers(&req);
    write!(writer, "{}\r\n\r\n", headers)
}

fn write_request_body<W: Write>(mut writer: W, req: &Request<Body>) -> Result<(), io::Error> {
    match req.body() {
        Body::Empty() => Ok(()),

        Body::Bytes(body) => writer.write_all(body),
    }
}

fn read_response_head<R: BufRead>(mut reader: R) -> Result<Vec<u8>, io::Error> {
    let mut response_headers = Vec::new();

    for _ in 0..20 {
        if response_headers.ends_with(&[CARRIAGE_RETURN, LINE_FEED, CARRIAGE_RETURN, LINE_FEED]) {
            break;
        }

        reader.read_until(LINE_FEED, &mut response_headers)?;
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

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::Parse(err) => {
                write!(f, "{}", err)
            }

            ParseError::Empty() => {
                write!(f, "Received empty response")
            }

            ParseError::Partial() => {
                write!(f, "Received partial response")
            }

            ParseError::Response(err) => {
                write!(f, "Invalid response: {}", err)
            }
        }
    }
}

pub fn parse_response_head(bytes: Vec<u8>) -> Result<response::Parts, ParseError> {
    let mut headers = [httparse::EMPTY_HEADER; 30];
    let mut resp = httparse::Response::new(&mut headers);

    match resp.parse(&bytes) {
        Ok(httparse::Status::Complete(_)) => {
            let parts = to_http_parts(resp).map_err(ParseError::Response)?;
            Ok(parts)
        }

        Ok(httparse::Status::Partial) => {
            if bytes.is_empty() {
                Err(ParseError::Empty())
            } else {
                Err(ParseError::Partial())
            }
        }

        Err(err) => Err(ParseError::Parse(err)),
    }
}

#[derive(Debug)]
pub enum ResponseError {
    InvalidBuilder(),
    HeaderName(header::InvalidHeaderName),
    HeaderValue(header::InvalidHeaderValue),
    StatusCode(),
    Builder(http::Error),
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ResponseError::InvalidBuilder() => {
                write!(f, "Invalid response builder")
            }

            ResponseError::HeaderName(err) => {
                write!(f, "Invalid header name: {}", err)
            }

            ResponseError::HeaderValue(err) => {
                write!(f, "Invalid header value: {}", err)
            }

            ResponseError::StatusCode() => {
                write!(f, "Failed to parse status code")
            }

            ResponseError::Builder(err) => {
                write!(f, "Response builder error: {}", err)
            }
        }
    }
}

fn to_http_parts(parsed: httparse::Response) -> Result<response::Parts, ResponseError> {
    let mut builder = Response::builder();
    let headers = builder
        .headers_mut()
        .ok_or(ResponseError::InvalidBuilder())?;

    for hdr in parsed.headers.iter() {
        let name = header::HeaderName::from_str(hdr.name).map_err(ResponseError::HeaderName)?;

        let value =
            header::HeaderValue::from_bytes(hdr.value).map_err(ResponseError::HeaderValue)?;

        headers.insert(name, value);
    }

    let code = parsed.code.ok_or(ResponseError::StatusCode())?;

    let response = builder
        .status(code)
        .body(())
        .map_err(ResponseError::Builder)?;

    Ok(response.into_parts().0)
}

fn err_if_false<E>(value: bool, err: E) -> Result<(), E> {
    if value {
        Ok(())
    } else {
        Err(err)
    }
}
