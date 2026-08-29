use crate::wasi::http::{outgoing_handler, types};
use crate::wasi::io::poll;

pub struct Response<T> {
    pub status: types::StatusCode,
    pub headers: types::Headers,
    pub body: Option<T>,
}

pub fn get<T>(url: &str, headers: types::Headers) -> Result<Response<T>, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let request = types::OutgoingRequest::new(headers);
    request
        .set_path_with_query(Some(&url))
        .map_err(|_| "Failed to set path with query".to_string())?;
    let future = outgoing_handler::handle(request, None).map_err(|e| e.to_string())?;
    let pollable = future.subscribe();

    // Wait for the future to be ready
    poll::poll(&[&pollable]);

    // Get the result
    let response = future
        .get()
        .ok_or("Future not ready".to_string())?
        .map_err(|_| "Request failed".to_string())?
        .map_err(|e| e.to_string())?;
    Ok(Response::<T> {
        status: response.status(),
        headers: response.headers(),
        body: extract_body::<T>(&response)?,
    })
}

fn extract_body<T>(response: &types::IncomingResponse) -> Result<Option<T>, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let body = response
        .consume()
        .map_err(|_| "Unable to access body".to_string())?;
    let header = response.headers().get("content-length");
    let len = if header.len() > 0 {
        String::from_utf8(header[0].clone())
            .map_err(|_| "Invalid content-length header".to_string())?
            .parse::<u64>()
            .map_err(|_| "Invalid content-length header".to_string())?
    } else {
        u64::MAX
    };
    let stream = body
        .stream()
        .map_err(|_| "Unable to access body stream".to_string())?;
    let bytes = stream.read(len).map_err(|e| e.to_string())?;
    // Convert json string `body` to T using serde_json
    let result: T = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(Some(result))
}
