use super::*;
use std::{
    collections::HashMap,
    future::Future,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use url::form_urlencoded;

fn run(test: impl Future<Output = ()>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async { timeout(Duration::from_secs(12), test).await.unwrap() });
}

async fn mock_calendar() -> (GoogleCalendar, TcpListener) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let mut calendar = GoogleCalendar::new(
        "test-desktop-client".to_owned(),
        Some(Secret::new("test-client-secret".to_owned()).unwrap()),
    )
    .unwrap();
    assert_eq!(calendar.endpoints.authorization.scheme(), "https");
    assert_eq!(calendar.endpoints.token.scheme(), "https");
    assert_eq!(calendar.endpoints.events.scheme(), "https");
    assert_eq!(calendar.endpoints.revoke.scheme(), "https");
    calendar.endpoints = Endpoints {
        authorization: base.join("authorize").unwrap(),
        token: base.join("token").unwrap(),
        revoke: base.join("revoke").unwrap(),
        events: base.join("events").unwrap(),
    };
    calendar.http = calendar_http().build().unwrap();
    (calendar, listener)
}

struct Request {
    headers: String,
    body: Vec<u8>,
}

impl Request {
    fn form(&self) -> HashMap<String, String> {
        assert!(
            self.headers
                .lines()
                .any(|line| line == "content-type: application/x-www-form-urlencoded")
        );
        form_urlencoded::parse(&self.body).into_owned().collect()
    }
}

async fn respond(listener: &TcpListener, status: &str, headers: &str, body: &str) -> Request {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut buffer = Vec::new();
    let (headers_end, content_length) = loop {
        let read = stream.read_buf(&mut buffer).await.unwrap();
        assert!(read > 0 && buffer.len() < 64 * 1024);
        if let Some(end) = buffer.windows(4).position(|part| part == b"\r\n\r\n") {
            let header = std::str::from_utf8(&buffer[..end]).unwrap();
            let length: usize = header
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .unwrap_or("0")
                .parse()
                .unwrap();
            break (end, length);
        }
    };
    let body_start = headers_end + 4;
    while buffer.len() < body_start + content_length {
        assert!(stream.read_buf(&mut buffer).await.unwrap() > 0);
    }

    // Oversized responses are abandoned by the client as soon as it sees the headers.
    let _ = stream
        .write_all(
            format!(
                "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}",
                body.len(),
            )
            .as_bytes(),
        )
        .await;
    Request {
        headers: String::from_utf8(buffer[..headers_end].to_vec()).unwrap(),
        body: buffer[body_start..body_start + content_length].to_vec(),
    }
}

fn callback_url(pending: &PendingConnect, fields: &[(&str, &str)]) -> Url {
    let mut url = Url::parse(&pending.redirect_uri).unwrap();
    url.query_pairs_mut().extend_pairs(fields);
    url
}

fn window_start() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_709_942_400)
}

const EVENTS: &str = r#"{
    "timeZone": "America/New_York",
    "nextPageToken": "another-page",
    "items": [
        {
            "id": "timed",
            "summary": "Synthetic meeting",
            "start": {"dateTime": "2024-03-10T01:30:00-05:00"},
            "end": {"dateTime": "2024-03-10T03:30:00-04:00"}
        },
        {
            "id": "all-day",
            "start": {"date": "2024-03-10"},
            "end": {"date": "2024-03-12"}
        },
        {
            "id": "unspecified-end",
            "start": {"dateTime": "2024-03-11T08:00:00Z"},
            "end": {"dateTime": "2024-03-11T09:00:00Z"},
            "endTimeUnspecified": true
        },
        {"id": "removed", "status": "cancelled"}
    ]
}"#;

#[test]
fn desktop_pkce_primary_events_refresh_and_revoke_are_real_loopback_requests() {
    run(async {
        let (calendar, listener) = mock_calendar().await;
        let pending = calendar.begin_connect().await.unwrap();
        let authorization: HashMap<_, _> = pending
            .authorization_url()
            .query_pairs()
            .into_owned()
            .collect();
        assert_eq!(authorization["client_id"], "test-desktop-client");
        assert_eq!(authorization["scope"], SCOPE);
        assert_eq!(authorization["response_type"], "code");
        assert_eq!(authorization["code_challenge_method"], "S256");
        assert_eq!(authorization["access_type"], "offline");
        assert_eq!(authorization["prompt"], "consent");
        assert!(!authorization.contains_key("client_secret"));
        assert!(authorization["redirect_uri"].starts_with("http://127.0.0.1:"));

        let refused = callback_url(&pending, &[("code", "unexpected"), ("state", "different")]);
        let accepted = callback_url(
            &pending,
            &[("code", "test+code"), ("state", &authorization["state"])],
        );
        let browser = tokio::spawn(async move {
            let http = calendar_http().build().unwrap();
            assert_eq!(
                http.get(refused).send().await.unwrap().status(),
                StatusCode::BAD_REQUEST
            );
            let page = http.get(accepted).send().await.unwrap();
            assert_eq!(page.status(), StatusCode::OK);
            assert!(!page.text().await.unwrap().contains("test+code"));
        });

        let challenge = authorization["code_challenge"].clone();
        let state = authorization["state"].clone();
        let redirect = authorization["redirect_uri"].clone();
        let server = tokio::spawn(async move {
            let token = format!(
                r#"{{"access_token":"test-access","token_type":"Bearer","scope":"{SCOPE}","expires_in":3600,"refresh_token":"original-refresh"}}"#,
            );
            let request = respond(&listener, "200 OK", "", &token).await;
            assert_eq!(request.headers.lines().next(), Some("POST /token HTTP/1.1"));
            let form = request.form();
            assert_eq!(form["code"], "test+code");
            assert_eq!(form["client_id"], "test-desktop-client");
            assert_eq!(form["client_secret"], "test-client-secret");
            assert_eq!(form["grant_type"], "authorization_code");
            assert_eq!(form["redirect_uri"], redirect);
            assert_ne!(form["code_verifier"], state);
            assert_eq!(
                URL_SAFE_NO_PAD
                    .decode(&form["code_verifier"])
                    .unwrap()
                    .len(),
                32
            );
            assert_eq!(
                URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes())),
                challenge
            );

            let request = respond(&listener, "200 OK", "", EVENTS).await;
            let line = request.headers.lines().next().unwrap();
            let query = line
                .strip_prefix("GET /events?")
                .unwrap()
                .strip_suffix(" HTTP/1.1")
                .unwrap();
            let query: HashMap<_, _> = form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect();
            assert_eq!(query["timeMin"], "2024-03-09T00:00:00Z");
            assert_eq!(query["timeMax"], "2024-04-08T00:00:00Z");
            assert_eq!(query["maxResults"], "50");
            assert_eq!(query["orderBy"], "startTime");
            assert_eq!(query["singleEvents"], "true");
            assert_eq!(query["showDeleted"], "false");
            assert!(query["fields"].contains("start(date,dateTime)"));
            assert!(!query.contains_key("timeZone"));
            assert!(!query.contains_key("access_token"));
            assert!(
                request
                    .headers
                    .lines()
                    .any(|line| line == "authorization: Bearer test-access")
            );

            let reply = r#"{"access_token":"next-access","token_type":"Bearer","expires_in":3600}"#;
            let request = respond(&listener, "200 OK", "", reply).await;
            assert_eq!(request.headers.lines().next(), Some("POST /token HTTP/1.1"));
            let form = request.form();
            assert_eq!(form["grant_type"], "refresh_token");
            assert_eq!(form["refresh_token"], "original-refresh");

            let reply = r#"{"access_token":"last-access","token_type":"Bearer","expires_in":3600,"refresh_token":"rotated-refresh"}"#;
            let request = respond(&listener, "200 OK", "", reply).await;
            assert_eq!(request.form()["refresh_token"], "original-refresh");

            let reply = r#"{"error":"invalid_grant","error_description":"reflected-credential"}"#;
            let request = respond(&listener, "400 Bad Request", "", reply).await;
            assert_eq!(request.form()["refresh_token"], "rotated-refresh");

            let request = respond(&listener, "200 OK", "", "").await;
            assert_eq!(
                request.headers.lines().next(),
                Some("POST /revoke HTTP/1.1")
            );
            assert_eq!(request.form()["token"], "rotated-refresh");
        });

        let mut session = calendar.finish_connect(pending).await.unwrap();
        browser.await.unwrap();
        let page = calendar.upcoming(&session, window_start()).await.unwrap();
        assert!(page.has_more);
        assert_eq!(page.calendar_time_zone.as_deref(), Some("America/New_York"));
        assert_eq!(page.events.len(), 3);
        assert_eq!(page.events[0].title.as_deref(), Some("Synthetic meeting"));
        let EventTime::Timed {
            starts_at,
            ends_at: Some(ends_at),
        } = page.events[0].time
        else {
            panic!("expected a timed event");
        };
        assert_eq!(starts_at.utc.unix_seconds, 1_710_052_200);
        assert_eq!(ends_at.utc.unix_seconds - starts_at.utc.unix_seconds, 3600);
        assert_eq!(
            (starts_at.offset_minutes, ends_at.offset_minutes),
            (Some(-300), Some(-240))
        );
        assert_eq!(
            page.events[1].time,
            EventTime::AllDay {
                starts_on: "2024-03-10".to_owned(),
                ends_before: "2024-03-12".to_owned()
            }
        );
        assert!(matches!(
            page.events[2].time,
            EventTime::Timed { ends_at: None, .. }
        ));

        session.expires_at = Instant::now();
        assert!(matches!(
            calendar.upcoming(&session, window_start()).await,
            Err(CalendarError::RefreshRequired)
        ));
        let grant_scope = session.refresh_token().grant_scope;
        let session = calendar.refresh(session.refresh_token()).await.unwrap();
        assert_eq!(session.refresh_token().grant_scope, grant_scope);
        let session = calendar.refresh(session.refresh_token()).await.unwrap();
        assert_eq!(session.refresh_token().grant_scope, grant_scope);
        let Err(error) = calendar.refresh(session.refresh_token()).await else {
            panic!("expected the revoked grant to be reported");
        };
        assert_eq!(error, CalendarError::ReconnectRequired);
        assert!(!error.to_string().contains("reflected-credential"));
        calendar.revoke(session.refresh_token()).await.unwrap();
        server.await.unwrap();
    });
}

#[test]
fn declined_cancelled_and_expired_connections_stop_before_exchange() {
    run(async {
        let (calendar, listener) = mock_calendar().await;
        let pending = calendar.begin_connect().await.unwrap();
        let denied = callback_url(
            &pending,
            &[
                ("state", pending.state.expose()),
                ("error", "access_denied"),
            ],
        );
        let browser = tokio::spawn(async move {
            assert_eq!(
                calendar_http()
                    .build()
                    .unwrap()
                    .get(denied)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        });
        assert!(matches!(
            calendar.finish_connect(pending).await,
            Err(CalendarError::AuthorizationDenied)
        ));
        browser.await.unwrap();

        let pending = calendar.begin_connect().await.unwrap();
        let host = pending.listener.local_addr().unwrap();
        {
            let mut operation = std::pin::pin!(calendar.finish_connect(pending));
            assert!(futures_util::poll!(&mut operation).is_pending());
        }
        assert!(TcpStream::connect(host).await.is_err());

        let mut pending = calendar.begin_connect().await.unwrap();
        pending.deadline = tokio::time::Instant::now();
        assert!(matches!(
            calendar.finish_connect(pending).await,
            Err(CalendarError::AuthorizationTimeout)
        ));
        assert!(
            timeout(Duration::from_millis(30), listener.accept())
                .await
                .is_err()
        );
    });
}

#[test]
fn event_requests_report_http_denial_redirect_and_oversize_without_body_details() {
    run(async {
        let (calendar, listener) = mock_calendar().await;
        let session = CalendarSession {
            access: Secret::new("test-access".to_owned()).unwrap(),
            expires_at: Instant::now() + Duration::from_secs(60),
            refresh: CalendarRefreshToken {
                client_key: calendar.client_key.clone(),
                secret: Secret::new("test-refresh".to_owned()).unwrap(),
                grant_scope: [1; 32],
            },
        };
        let server = tokio::spawn(async move {
            respond(&listener, "401 Unauthorized", "", "echoed-private-value").await;
            respond(&listener, "403 Forbidden", "", "echoed-private-value").await;
            respond(
                &listener,
                "307 Temporary Redirect",
                "Location: /other\r\n",
                "",
            )
            .await;
            let oversized = " ".repeat(256 * 1024 + 1);
            respond(&listener, "200 OK", "", &oversized).await;
        });
        for expected in [
            CalendarError::RefreshRequired,
            CalendarError::HttpStatus(403),
            CalendarError::RedirectBlocked,
            CalendarError::ResponseTooLarge,
        ] {
            let Err(error) = calendar.upcoming(&session, window_start()).await else {
                panic!("expected a failed response");
            };
            assert_eq!(error, expected);
            assert!(!error.to_string().contains("echoed-private-value"));
        }
        server.await.unwrap();
    });
}
