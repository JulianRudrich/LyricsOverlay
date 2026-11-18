//! Just enough HTTP/1.1 for the extension: one small request per connection,
//! one JSON answer, then the connection closes.

use std::io::{self, Read, Write};

/// Request line and headers together.
pub(crate) const MAX_HEAD: usize = 8 * 1024;
/// The largest body; an update with every field at its limit stays well below.
pub(crate) const MAX_BODY: usize = 64 * 1024;
const MAX_HEADERS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    pub method: String,
    pub path: String,
    /// Names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadError {
    /// The connection closed before a request arrived.
    Closed,
    Malformed,
    TooLarge,
    LengthRequired,
}

/// Reads one request, refusing anything larger than the limits above.
pub(crate) fn read_request(reader: &mut impl Read) -> Result<Request, ReadError> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    let (head_length, mut request) = loop {
        let read = reader.read(&mut chunk).map_err(|_| ReadError::Closed)?;
        if read == 0 {
            return Err(if buffer.is_empty() {
                ReadError::Closed
            } else {
                ReadError::Malformed
            });
        }
        buffer.extend_from_slice(&chunk[..read]);
        let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
        let mut parsed = httparse::Request::new(&mut headers);
        match parsed.parse(&buffer) {
            Ok(httparse::Status::Complete(length)) => {
                let request = Request {
                    method: parsed.method.unwrap_or_default().to_owned(),
                    path: parsed.path.unwrap_or_default().to_owned(),
                    headers: parsed
                        .headers
                        .iter()
                        .map(|header| {
                            (
                                header.name.to_ascii_lowercase(),
                                String::from_utf8_lossy(header.value).trim().to_owned(),
                            )
                        })
                        .collect(),
                    body: Vec::new(),
                };
                break (length, request);
            }
            Ok(httparse::Status::Partial) if buffer.len() < MAX_HEAD => {}
            Ok(httparse::Status::Partial) | Err(httparse::Error::TooManyHeaders) => {
                return Err(ReadError::TooLarge)
            }
            Err(_) => return Err(ReadError::Malformed),
        }
    };
    if head_length > MAX_HEAD {
        return Err(ReadError::TooLarge);
    }
    if request.header("transfer-encoding").is_some() {
        return Err(ReadError::LengthRequired);
    }
    let length = match request.header("content-length") {
        Some(value) => value.parse::<usize>().map_err(|_| ReadError::Malformed)?,
        None if request.method == "POST" => return Err(ReadError::LengthRequired),
        None => 0,
    };
    if length > MAX_BODY {
        return Err(ReadError::TooLarge);
    }
    let mut body = buffer.split_off(head_length);
    if body.len() > length {
        return Err(ReadError::Malformed);
    }
    let missing = length - body.len();
    if missing > 0 {
        let start = body.len();
        body.resize(length, 0);
        reader
            .read_exact(&mut body[start..])
            .map_err(|_| ReadError::Malformed)?;
    }
    request.body = body;
    Ok(request)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Response {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

impl Response {
    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: value.to_string(),
        }
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: String::new(),
        }
    }

    pub fn error(status: u16, code: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": code }))
    }

    pub fn with_header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }

    pub fn write_to(&self, writer: &mut impl Write) -> io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            411 => "Length Required",
            413 => "Content Too Large",
            415 => "Unsupported Media Type",
            503 => "Service Unavailable",
            _ => "Error",
        };
        let mut head = format!("HTTP/1.1 {} {reason}\r\n", self.status);
        if !self.body.is_empty() {
            head.push_str("Content-Type: application/json\r\n");
        }
        head.push_str(&format!(
            "Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
            self.body.len()
        ));
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        writer.write_all(head.as_bytes())?;
        writer.write_all(self.body.as_bytes())?;
        writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hands out its input in small pieces, like a slow connection.
    struct Trickle<'a> {
        data: &'a [u8],
        step: usize,
    }

    impl Read for Trickle<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = self.step.min(self.data.len()).min(buffer.len());
            buffer[..count].copy_from_slice(&self.data[..count]);
            self.data = &self.data[count..];
            Ok(count)
        }
    }

    fn read(raw: &str, step: usize) -> Result<Request, ReadError> {
        read_request(&mut Trickle {
            data: raw.as_bytes(),
            step,
        })
    }

    #[test]
    fn reads_a_request_in_pieces() {
        let raw = "POST /v1/update HTTP/1.1\r\nHost: 127.0.0.1:17464\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"tabs\": []} ";
        for step in [1, 7, 4096] {
            let request = read(raw, step).expect("a valid request");
            assert_eq!(
                (request.method.as_str(), request.path.as_str()),
                ("POST", "/v1/update")
            );
            assert_eq!(request.header("host"), Some("127.0.0.1:17464"));
            assert_eq!(request.header("content-type"), Some("application/json"));
            assert_eq!(request.body, b"{\"tabs\": []} ");
        }
        let get = read("GET /v1/hello HTTP/1.1\r\nHost: localhost\r\n\r\n", 5).expect("GET");
        assert!(get.body.is_empty());
    }

    #[test]
    fn refuses_what_is_too_large_or_unclear() {
        let huge_header = format!("GET / HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(MAX_HEAD));
        assert_eq!(read(&huge_header, 4096), Err(ReadError::TooLarge));
        let huge_body = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert_eq!(read(&huge_body, 4096), Err(ReadError::TooLarge));
        let many = format!(
            "GET / HTTP/1.1\r\n{}\r\n",
            (0..40).map(|i| format!("X-{i}: 1\r\n")).collect::<String>()
        );
        assert_eq!(read(&many, 4096), Err(ReadError::TooLarge));
        assert_eq!(
            read("POST / HTTP/1.1\r\nHost: x\r\n\r\n", 4096),
            Err(ReadError::LengthRequired)
        );
        assert_eq!(
            read(
                "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
                4096
            ),
            Err(ReadError::LengthRequired)
        );
        assert_eq!(
            read("POST / HTTP/1.1\r\nContent-Length: 10\r\n\r\n{}", 4096),
            Err(ReadError::Malformed),
            "the body ended early"
        );
        assert_eq!(read("NOT HTTP\r\n\r\n", 4096), Err(ReadError::Malformed));
        assert_eq!(read("", 4096), Err(ReadError::Closed));
    }

    #[test]
    fn writes_json_answers_that_close_the_connection() {
        let mut out = Vec::new();
        Response::json(200, &serde_json::json!({ "ok": true }))
            .with_header("Vary", "Origin")
            .write_to(&mut out)
            .expect("written");
        let text = String::from_utf8(out).expect("utf-8");
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: application/json\r\nContent-Length: 11\r\n"));
        assert!(text.contains("Connection: close\r\n"));
        assert!(text.contains("Vary: Origin\r\n"));
        assert!(text.ends_with("\r\n\r\n{\"ok\":true}"));
    }
}
