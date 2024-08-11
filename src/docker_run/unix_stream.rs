use std::fmt;
use std::io;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub path: PathBuf,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
}

pub fn with_stream<F, T, E, ErrorTagger>(
    config: &Config,
    to_error: ErrorTagger,
    f: F,
) -> Result<T, E>
where
    F: FnOnce(&mut UnixStream) -> Result<T, E>,
    ErrorTagger: Copy,
    ErrorTagger: FnOnce(Error) -> E,
{
    let mut stream = UnixStream::connect(&config.path)
        .map_err(Error::Connect)
        .map_err(to_error)?;

    stream
        .set_read_timeout(Some(config.read_timeout))
        .map_err(Error::SetStreamTimeout)
        .map_err(to_error)?;

    stream
        .set_write_timeout(Some(config.write_timeout))
        .map_err(Error::SetStreamTimeout)
        .map_err(to_error)?;

    let result = f(&mut stream)?;

    let _ = stream.shutdown(Shutdown::Both);

    Ok(result)
}

#[derive(Debug)]
pub enum Error {
    Connect(io::Error),
    SetStreamTimeout(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Connect(err) => {
                write!(f, "Failed to connect to docker unix socket: {}", err)
            }

            Error::SetStreamTimeout(err) => {
                write!(f, "Failed set timeout on unix socket: {}", err)
            }
        }
    }
}
