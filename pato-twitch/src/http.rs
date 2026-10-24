use crate::wasi::http::{outgoing_handler, types};
use crate::wasi::io::{poll, streams};

pub struct Response {
    pub status: types::StatusCode,
    pub body: Vec<u8>,
}

impl Response {
    // Whether the response has a 2xx status code
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    // Deserialize the response body as JSON
    pub fn json<T>(&self) -> Result<T, String>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        serde_json::from_slice(&self.body).map_err(|e| e.to_string())
    }

    // The response body as a UTF-8 string (lossy)
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

// Perform a GET request against an absolute URL
pub fn get(url: &str, headers: types::Headers) -> Result<Response, String> {
    request(types::Method::Get, url, headers, None)
}

// Perform a POST request with a body against an absolute URL
pub fn post(url: &str, headers: types::Headers, body: Vec<u8>) -> Result<Response, String> {
    request(types::Method::Post, url, headers, Some(body))
}

// Perform a PATCH request with a body against an absolute URL
pub fn patch(url: &str, headers: types::Headers, body: Vec<u8>) -> Result<Response, String> {
    request(types::Method::Patch, url, headers, Some(body))
}

// Perform a DELETE request against an absolute URL
pub fn delete(url: &str, headers: types::Headers) -> Result<Response, String> {
    request(types::Method::Delete, url, headers, None)
}

fn request(
    method: types::Method,
    url: &str,
    headers: types::Headers,
    body: Option<Vec<u8>>,
) -> Result<Response, String> {
    let url = url::Url::parse(url).map_err(|e| e.to_string())?;

    let scheme = match url.scheme() {
        "http" => types::Scheme::Http,
        "https" => types::Scheme::Https,
        other => types::Scheme::Other(other.to_string()),
    };
    let authority = match url.port() {
        Some(port) => format!("{}:{}", url.host_str().unwrap_or_default(), port),
        None => url.host_str().unwrap_or_default().to_string(),
    };
    let path_with_query = match url.query() {
        Some(query) => format!("{}?{}", url.path(), query),
        None => url.path().to_string(),
    };

    // Send an exact Content-Length rather than chunked encoding: Twitch's API
    // stalls on a chunked request body instead of answering it.
    if let Some(body) = &body {
        headers
            .set("content-length", &[body.len().to_string().into_bytes()])
            .map_err(|e| format!("Failed to set content-length: {e:?}"))?;
    }

    let request = types::OutgoingRequest::new(headers);
    request
        .set_method(&method)
        .map_err(|_| "Failed to set method".to_string())?;
    request
        .set_scheme(Some(&scheme))
        .map_err(|_| "Failed to set scheme".to_string())?;
    request
        .set_authority(Some(&authority))
        .map_err(|_| "Failed to set authority".to_string())?;
    request
        .set_path_with_query(Some(&path_with_query))
        .map_err(|_| "Failed to set path with query".to_string())?;

    // The request body must be taken before `handle` consumes the request,
    // written once the request is in flight, and finished afterwards for the
    // request to be considered complete.
    let outgoing_body = request
        .body()
        .map_err(|_| "Failed to access request body".to_string())?;
    // A stalled request must fail on its own well inside the host's per-call
    // limit for plugin code (10s); hitting that limit kills the plugin.
    let options = types::RequestOptions::new();
    let ns = |secs: u64| Some(secs * 1_000_000_000);
    let _ = options.set_connect_timeout(ns(3));
    let _ = options.set_first_byte_timeout(ns(5));
    let _ = options.set_between_bytes_timeout(ns(5));
    let future = outgoing_handler::handle(request, Some(options)).map_err(|e| e.to_string())?;
    if let Some(body) = body {
        // The stream borrows the body; it must be dropped before `finish`.
        let stream = outgoing_body
            .write()
            .map_err(|_| "Failed to access request body stream".to_string())?;
        for chunk in body.chunks(4096) {
            stream
                .blocking_write_and_flush(chunk)
                .map_err(|e| format!("Failed to write request body: {e:?}"))?;
        }
    }
    types::OutgoingBody::finish(outgoing_body, None).map_err(|e| e.to_string())?;

    // Wait for the response to be ready
    let pollable = future.subscribe();
    poll::poll(&[&pollable]);

    let response = future
        .get()
        .ok_or("Response future not ready".to_string())?
        .map_err(|_| "Response future already consumed".to_string())?
        .map_err(|e| e.to_string())?;

    Ok(Response {
        status: response.status(),
        body: read_body(&response)?,
    })
}

// Read an incoming response body to completion
fn read_body(response: &types::IncomingResponse) -> Result<Vec<u8>, String> {
    let body = response
        .consume()
        .map_err(|_| "Unable to access body".to_string())?;
    let stream = body
        .stream()
        .map_err(|_| "Unable to access body stream".to_string())?;

    let mut bytes = Vec::new();
    loop {
        match stream.blocking_read(8192) {
            Ok(chunk) => bytes.extend_from_slice(&chunk),
            Err(streams::StreamError::Closed) => break,
            Err(streams::StreamError::LastOperationFailed(e)) => {
                return Err(e.to_debug_string());
            }
        }
    }
    Ok(bytes)
}
