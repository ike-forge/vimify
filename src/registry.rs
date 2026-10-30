use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

use crate::port::{self, Port};
use crate::{fsx, paths};

const URL: &str = "https://raw.githubusercontent.com/ike-forge/vimify/ports-bundle/ports.toml";
/// Bounds the pause at startup: a slow network falls back to what is cached.
const TIMEOUT: Duration = Duration::from_secs(2);
const LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Bundle {
    port: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    tool: String,
    text: String,
}

/// Every port in one file: a single request, with an ETag keeping later runs cheap.
pub fn bundle(sources: Vec<(String, String)>) -> Result<String> {
    let port = sources
        .into_iter()
        .map(|(tool, text)| Entry { tool, text })
        .collect();
    Ok(toml::to_string(&Bundle { port })?)
}

/// Ports this build cannot read are skipped, so a newer schema does not break it.
fn parse(text: &str) -> Vec<Port> {
    let Ok(bundle) = toml::from_str::<Bundle>(text) else {
        return Vec::new();
    };
    bundle
        .port
        .iter()
        .filter_map(|entry| port::parse(&entry.tool, &entry.text).ok())
        .collect()
}

/// `remote` is newer for every tool it covers. Embedded ports fill in the rest.
fn merge(embedded: Vec<Port>, remote: Vec<Port>) -> Vec<Port> {
    let mut ports: Vec<Port> = embedded
        .into_iter()
        .filter(|p| !remote.iter().any(|r| r.name == p.name))
        .chain(remote.iter().cloned())
        .collect();
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    ports
}

/// Never fails: offline or unreachable, vimify runs on what it already has.
/// `VIMIFY_PORTS_URL` replaces the URL; empty turns updates off.
pub fn refresh(embedded: Vec<Port>) -> Vec<Port> {
    let Some(cache) = paths::expand("$XDG_CACHE_HOME/vimify/ports.toml") else {
        return embedded;
    };
    let url = std::env::var("VIMIFY_PORTS_URL").unwrap_or_else(|_| URL.to_owned());
    if !url.is_empty() {
        let _ = fetch(&url, &cache);
    }
    let cached = fs::read_to_string(&cache).unwrap_or_default();
    merge(embedded, parse(&cached))
}

fn fetch(url: &str, cache: &Path) -> Result<()> {
    let config = Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        // TLS-intercepting proxies only work with the system's certificates.
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build();
    let agent = Agent::new_with_config(config);
    let etag_path = cache.with_extension("etag");
    let mut request = agent.get(url);
    if let Ok(etag) = fs::read_to_string(&etag_path)
        && cache.exists()
    {
        request = request.header("If-None-Match", etag.trim());
    }
    let mut response = request.call().with_context(|| url.to_owned())?;
    if response.status() == 304 {
        return Ok(());
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let text = response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_string()?;
    // A failed deploy must not replace a working cache.
    if parse(&text).is_empty() {
        return Ok(());
    }
    fsx::write_atomic(cache, &text)?;
    match etag {
        Some(etag) => fsx::write_atomic(&etag_path, &etag)?,
        None => {
            let _ = fs::remove_file(&etag_path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;
    use tempfile::TempDir;

    const YAML: &str = "schema = 1\nversion = \"1\"\nbinaries = []\n";

    fn port(name: &str, version: &str) -> Port {
        let text = YAML.replace("version = \"1\"", &format!("version = \"{version}\""));
        port::parse(name, &text).unwrap()
    }

    fn names(ports: &[Port]) -> Vec<(&str, &str)> {
        ports
            .iter()
            .map(|p| (p.name.as_str(), p.version.as_str()))
            .collect()
    }

    /// Answers each connection with the next response and returns the requests seen.
    fn serve(responses: Vec<String>) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/ports.toml", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                while reader.read_line(&mut request).unwrap() > 2 && !request.ends_with("\r\n\r\n")
                {
                }
                requests.push(request);
                stream.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        (url, server)
    }

    const BODY: &str = "[[port]]\ntool = \"a\"\ntext = \"\"\"\nschema = 1\nversion = \"1\"\nbinaries = []\n\"\"\"\n";
    const NOT_MODIFIED: &str = "HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n";
    const NOT_FOUND: &str =
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    fn ok(body: &str) -> String {
        let length = body.len();
        format!(
            "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}"
        )
    }

    #[test]
    fn caches_the_bundle_and_revalidates_with_its_etag() {
        let dir = TempDir::new().unwrap();
        let cache = dir.path().join("ports.toml");
        let (url, server) = serve(vec![ok(BODY), NOT_MODIFIED.to_owned()]);

        fetch(&url, &cache).unwrap();
        let cached = fs::read_to_string(&cache).unwrap();
        fetch(&url, &cache).unwrap();

        let requests = server.join().unwrap();
        assert!(!requests[0].to_lowercase().contains("if-none-match"));
        assert!(requests[1].to_lowercase().contains("if-none-match: \"v1\""));
        assert_eq!(fs::read_to_string(&cache).unwrap(), cached);
        assert_eq!(names(&parse(&cached)), [("a", "1")]);
    }

    #[test]
    fn a_bad_deploy_keeps_the_cache() {
        let dir = TempDir::new().unwrap();
        let cache = dir.path().join("ports.toml");
        let (url, server) = serve(vec![ok(BODY), ok("oops")]);

        fetch(&url, &cache).unwrap();
        let cached = fs::read_to_string(&cache).unwrap();
        fs::remove_file(cache.with_extension("etag")).unwrap();
        fetch(&url, &cache).unwrap();

        server.join().unwrap();
        assert_eq!(fs::read_to_string(&cache).unwrap(), cached);
    }

    #[test]
    fn a_failed_request_keeps_the_cache() {
        let dir = TempDir::new().unwrap();
        let cache = dir.path().join("ports.toml");
        fs::write(&cache, "kept").unwrap();
        let (url, server) = serve(vec![NOT_FOUND.to_owned()]);

        assert!(fetch(&url, &cache).is_err());

        server.join().unwrap();
        assert_eq!(fs::read_to_string(&cache).unwrap(), "kept");
    }

    #[test]
    fn bundle_round_trips() {
        let text = bundle(vec![("a".into(), YAML.into())]).unwrap();
        assert_eq!(names(&parse(&text)), [("a", "1")]);
    }

    #[test]
    fn remote_replaces_embedded_per_tool() {
        let out = merge(
            vec![port("a", "1"), port("a", "2"), port("b", "1")],
            vec![port("a", "3")],
        );
        assert_eq!(names(&out), [("a", "3"), ("b", "1")]);
    }

    #[test]
    fn a_port_with_a_newer_schema_is_skipped_alone() {
        let newer = YAML.replace("schema = 1", "schema = 99");
        let text = bundle(vec![("a".into(), newer), ("b".into(), YAML.into())]).unwrap();
        assert_eq!(names(&parse(&text)), [("b", "1")]);
    }

    #[test]
    fn garbage_is_no_ports() {
        assert!(parse("<html>404</html>").is_empty());
    }
}
