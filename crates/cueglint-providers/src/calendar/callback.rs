use super::{CalendarError, CalendarResult, Secret};
use std::{str, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Instant, timeout, timeout_at},
};
use url::form_urlencoded;
use zeroize::Zeroizing;

const REQUEST_BYTES: usize = 8192;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) async fn receive(
    listener: TcpListener,
    host: &str,
    state: &Secret,
    deadline: Instant,
) -> CalendarResult<Secret> {
    timeout_at(deadline, async {
        loop {
            let (mut stream, _) = listener
                .accept()
                .await
                .map_err(|_| CalendarError::LoopbackUnavailable)?;
            let outcome = timeout(REQUEST_TIMEOUT, request(&mut stream, host, state))
                .await
                .ok()
                .flatten();
            let (status, body) = if outcome.is_some() {
                ("200 OK", "Return to Cueglint to finish connecting Calendar.")
            } else {
                ("400 Bad Request", "This Calendar authorization request is invalid.")
            };
            let reply = format!(
                "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\r\n{body}",
                body.len(),
            );
            let _ = timeout(REQUEST_TIMEOUT, stream.write_all(reply.as_bytes())).await;
            if let Some(outcome) = outcome {
                return outcome;
            }
        }
    })
    .await
    .map_err(|_| CalendarError::AuthorizationTimeout)?
}

async fn request(
    stream: &mut TcpStream,
    host: &str,
    state: &Secret,
) -> Option<CalendarResult<Secret>> {
    let mut bytes = Zeroizing::new([0_u8; REQUEST_BYTES]);
    let mut len = 0;
    while len < bytes.len() {
        let read = stream.read(&mut bytes[len..]).await.ok()?;
        if read == 0 {
            return None;
        }
        len += read;
        if let Some(end) = bytes[..len].windows(4).position(|part| part == b"\r\n\r\n") {
            return parse(str::from_utf8(&bytes[..end]).ok()?, host, state);
        }
    }
    None
}

fn parse(headers: &str, host: &str, state: &Secret) -> Option<CalendarResult<Secret>> {
    let mut lines = headers.split("\r\n");
    let query = lines
        .next()?
        .strip_prefix("GET /?")?
        .strip_suffix(" HTTP/1.1")?;
    if query.bytes().any(|b| !b.is_ascii_graphic() || b == b'#') {
        return None;
    }
    let mut hosts = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"));
    if hosts.next()?.1.trim() != host || hosts.next().is_some() {
        return None;
    }
    let mut returned_state = None;
    let mut code = None;
    let mut error = None;
    for (name, value) in form_urlencoded::parse(query.as_bytes()) {
        let field = match name.as_ref() {
            "state" => &mut returned_state,
            "code" => &mut code,
            "error" => &mut error,
            _ => continue,
        };
        if field.replace(value).is_some() {
            return None;
        }
    }
    if returned_state.as_deref()? != state.expose() {
        return None;
    }
    match (code, error.as_deref()) {
        (Some(code), None) => Secret::new(code.into_owned()).ok().map(Ok),
        (None, Some("access_denied")) => Some(Err(CalendarError::AuthorizationDenied)),
        (None, Some(error)) if !error.is_empty() => Some(Err(CalendarError::AuthorizationFailed)),
        _ => None,
    }
}
