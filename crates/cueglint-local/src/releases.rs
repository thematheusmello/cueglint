//! Public release metadata, requested explicitly from the desktop or CLI.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const FEED: &str = "https://api.github.com/repos/thematheusmello/cueglint/releases?per_page=1";

#[derive(Debug, Serialize)]
pub struct PublishedRelease {
    pub tag: String,
    pub prerelease: bool,
    pub url: String,
}

#[derive(Deserialize)]
struct Entry {
    tag_name: String,
    prerelease: bool,
    draft: bool,
}

pub fn check() -> Result<Option<PublishedRelease>> {
    fetch(FEED, true)
}

fn fetch(url: &str, https_only: bool) -> Result<Option<PublishedRelease>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(https_only)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "Cueglint-release-check")
        .call()
        .context("Could not reach GitHub")?;
    ensure!(
        response.status().as_u16() == 200,
        "GitHub returned HTTP {}",
        response.status().as_u16()
    );
    let bytes = response
        .body_mut()
        .with_config()
        .limit(128 * 1024)
        .read_to_vec()
        .context("Could not read the release response")?;
    let mut entries: Vec<Entry> =
        serde_json::from_slice(&bytes).context("Invalid release response")?;
    ensure!(entries.len() <= 1, "Unexpected number of releases");
    let Some(entry) = entries.pop() else {
        return Ok(None);
    };
    let tag = entry.tag_name;
    ensure!(
        !entry.draft
            && tag.starts_with('v')
            && tag.len() <= 64
            && tag.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
            && tag
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b)),
        "Invalid published Cueglint release"
    );
    Ok(Some(PublishedRelease {
        url: format!("https://github.com/thematheusmello/cueglint/releases/tag/{tag}"),
        tag,
        prerelease: entry.prerelease,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    fn reply(status: &str, headers: &str, body: &[u8]) -> Result<Option<PublishedRelease>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
            body.len()
        );
        thread::scope(|scope| {
            let server = scope.spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let request: Vec<_> = BufReader::new(&stream)
                    .lines()
                    .map(Result::unwrap)
                    .take_while(|line| !line.is_empty())
                    .collect();
                assert_eq!(request[0], "GET /releases?per_page=1 HTTP/1.1");
                let headers = request.join("\n").to_ascii_lowercase();
                assert!(headers.contains("user-agent: cueglint-release-check"));
                assert!(!headers.contains("authorization:") && !headers.contains("cookie:"));
                stream.write_all(response.as_bytes()).unwrap();
                let _ = stream.write_all(body);
            });
            let result = fetch(&format!("http://{address}/releases?per_page=1"), false);
            server.join().unwrap();
            result
        })
    }

    #[test]
    fn anonymous_release_check_bounds_input_and_builds_an_official_link() -> Result<()> {
        let entry = br#"[{"tag_name":"v0.1.0-dev.3","prerelease":true,"draft":false,"html_url":"https://example.invalid"}]"#;
        let release = reply("200 OK", "", entry)?.unwrap();
        assert_eq!(release.tag, "v0.1.0-dev.3");
        assert!(release.prerelease);
        assert_eq!(
            release.url,
            "https://github.com/thematheusmello/cueglint/releases/tag/v0.1.0-dev.3"
        );
        assert!(reply("200 OK", "", b"[]")?.is_none());
        for body in [
            &br#"[{"tag_name":"v1","draft":true,"prerelease":false}]"#[..],
            br#"[{"tag_name":"v1/../../other","draft":false,"prerelease":false}]"#,
            br#"[{"tag_name":"v1"}]"#,
            b"invalid",
            &vec![b' '; 128 * 1024 + 1],
        ] {
            assert!(reply("200 OK", "", body).is_err());
        }
        assert!(
            reply("403 Forbidden", "", b"private error body")
                .unwrap_err()
                .to_string()
                .contains("HTTP 403")
        );
        assert!(reply("302 Found", "Location: https://example.invalid/\r\n", b"").is_err());
        Ok(())
    }
}
