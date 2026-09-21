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

    let request = types::OutgoingRequest::new(headers);
    request
        .set_scheme(Some(&scheme))
        .map_err(|_| "Failed to set scheme".to_string())?;
    request
        .set_authority(Some(&authority))
        .map_err(|_| "Failed to set authority".to_string())?;
    request
        .set_path_with_query(Some(&path_with_query))
        .map_err(|_| "Failed to set path with query".to_string())?;

    // The (empty) request body must be taken before `handle` consumes the request
    // and finished afterwards for the request to be considered complete.
    let outgoing_body = request
        .body()
        .map_err(|_| "Failed to access request body".to_string())?;
    let future = outgoing_handler::handle(request, None).map_err(|e| e.to_string())?;
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
