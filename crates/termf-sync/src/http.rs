//! S3 and WebDAV through the system's `curl`, so TermF carries no HTTP or TLS stack of its own. Credentials go
//! to curl on its standard input (`--config -`), never on a command line another user could read.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Context, Result, ensure};

use crate::objects::{Store, Written};
use crate::remote::Credentials;

/// curl signs S3 requests correctly from 8.3.0: the payload hash header for S3 (8.0.0), the canonical query of
/// a listing and signing over plain HTTP (8.3.0).
pub const S3_CURL: (u32, u32, u32) = (8, 3, 0);

pub struct Response {
    pub status: u16,
    headers: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<String> {
        // With redirects or a 100 Continue, the last block of headers is the response's.
        let last = self
            .headers
            .rsplit("\r\n\r\n")
            .find(|b| !b.trim().is_empty())?;
        last.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }

    fn error(&self, what: &str) -> anyhow::Error {
        let body = String::from_utf8_lossy(&self.body);
        let code = elements(&body, "Code").into_iter().next();
        let message = elements(&body, "Message").into_iter().next();
        let detail = match (code, message) {
            (Some(c), Some(m)) => format!(": {} ({})", unescape(m), unescape(c)),
            (Some(c), None) => format!(": {}", unescape(c)),
            _ => String::new(),
        };
        let hint = match self.status {
            401 | 403 => " (check the credentials and their rights)",
            404 => " (check the address)",
            _ => "",
        };
        anyhow::anyhow!("{what}: the server answered {}{detail}{hint}", self.status)
    }
}

pub struct Request<'a> {
    pub method: &'a str,
    pub url: String,
    pub headers: Vec<String>,
    /// Sent as the body, read by curl from the file.
    pub upload: Option<&'a Path>,
    /// Where the body goes; else in `Response.body`.
    pub output: Option<&'a Path>,
}

pub struct Curl {
    work: PathBuf,
    /// `user:secret`.
    user: Option<String>,
    sigv4: Option<String>,
}

/// A curl config value: quoted, with the escapes curl reads back.
fn quoted(value: &str) -> Result<String> {
    ensure!(
        !value.chars().any(char::is_control),
        "a value sent to curl holds a control character"
    );
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

static SEQ: AtomicU32 = AtomicU32::new(0);

impl Curl {
    pub fn command() -> Command {
        let mut cmd = Command::new("curl");
        cmd.args(["--config", "-"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    /// What curl reads on its standard input for `request`: the only place the credentials go.
    pub fn config(&self, request: &Request, output: &Path, head: &Path) -> Result<String> {
        let mut lines = vec![
            "silent".to_owned(),
            "show-error".to_owned(),
            "globoff".to_owned(),
            format!("proto = {}", quoted("=http,https")?),
            "connect-timeout = 20".to_owned(),
            "max-time = 300".to_owned(),
            format!("url = {}", quoted(&request.url)?),
            format!("request = {}", quoted(request.method)?),
            format!("header = {}", quoted("Expect:")?),
            format!("output = {}", quoted(&output.to_string_lossy())?),
            format!("dump-header = {}", quoted(&head.to_string_lossy())?),
            format!("write-out = {}", quoted("%{http_code}")?),
        ];
        for header in &request.headers {
            lines.push(format!("header = {}", quoted(header)?));
        }
        if let Some(file) = request.upload {
            let file = file.to_string_lossy();
            // curl appends the file's name to a URL ending in `/` that it uploads to: a listing reads it.
            if request.method == "PUT" {
                lines.push(format!("upload-file = {}", quoted(&file)?));
            } else {
                lines.push(format!("data-binary = {}", quoted(&format!("@{file}"))?));
            }
        }
        if let Some(user) = &self.user {
            lines.push(format!("user = {}", quoted(user)?));
        }
        if let Some(sigv4) = &self.sigv4 {
            lines.push(format!("aws-sigv4 = {}", quoted(sigv4)?));
        }
        Ok(lines.join("\n") + "\n")
    }

    pub fn run(&self, request: Request) -> Result<Response> {
        fs::create_dir_all(&self.work)?;
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let tag = format!("{}-{n}", std::process::id());
        let head = self.work.join(format!("head-{tag}"));
        let body = self.work.join(format!("body-{tag}"));
        let output = request.output.map_or(body.clone(), Path::to_path_buf);
        let config = self.config(&request, &output, &head)?;
        let mut child = Self::command()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("cannot run curl: is it installed?")?;
        child
            .stdin
            .take()
            .expect("piped")
            .write_all(config.as_bytes())?;
        let out = child.wait_with_output()?;
        let headers = fs::read_to_string(&head).unwrap_or_default();
        let bytes = if request.output.is_none() {
            fs::read(&body).unwrap_or_default()
        } else {
            Vec::new()
        };
        let _ = fs::remove_file(&head);
        let _ = fs::remove_file(&body);
        ensure!(
            out.status.success(),
            "cannot reach {}: {}",
            request.url,
            String::from_utf8_lossy(&out.stderr).trim()
        );
        let status = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .context("curl gave no HTTP status")?;
        Ok(Response {
            status,
            headers,
            body: bytes,
        })
    }
}

pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let version = text.split_whitespace().nth(1)?;
    let mut parts = version
        .split(|c: char| !c.is_ascii_digit())
        .map(|p| p.parse().ok());
    Some((
        parts.next()??,
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
    ))
}

fn curl_version() -> Result<(u32, u32, u32)> {
    static VERSION: OnceLock<Result<(u32, u32, u32), String>> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let out = Command::new("curl")
                .arg("--version")
                .stdin(Stdio::null())
                .output()
                .map_err(|_| "cannot run curl: is it installed?".to_owned())?;
            parse_version(&String::from_utf8_lossy(&out.stdout))
                .ok_or_else(|| "cannot read the version of curl".to_owned())
        })
        .clone()
        .map_err(anyhow::Error::msg)
}

pub fn check_s3_curl(version: (u32, u32, u32)) -> Result<()> {
    let (a, b, c) = S3_CURL;
    ensure!(
        version >= S3_CURL,
        "S3 needs curl {a}.{b}.{c} or newer to sign its requests, and this system has {}.{}.{}: update curl, \
         or choose WebDAV, git or a folder",
        version.0,
        version.1,
        version.2
    );
    Ok(())
}

fn http_url(url: &str) -> Result<String> {
    let url = url.trim();
    ensure!(
        (url.starts_with("https://") || url.starts_with("http://"))
            && url.len() > "https://".len()
            && !url.chars().any(|c| c.is_whitespace() || c.is_control()),
        "{url:?} is not an http:// or https:// address"
    );
    Ok(url.to_owned())
}

/// RFC 3986 unreserved characters stay, everything else is %XX: what S3 expects in a query.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b => format!("%{b:02X}"),
        })
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The inner text of every element whose local name is `local`, whatever its namespace prefix.
pub fn elements<'a>(xml: &'a str, local: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(open) = xml[at..].find('<').map(|i| at + i) {
        let rest = &xml[open + 1..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let name = &rest[..end];
        let Some(close) = rest.find('>') else { break };
        at = open + 1 + close + 1;
        let self_closing = rest[..close].ends_with('/');
        if name.rsplit(':').next() != Some(local)
            || name.starts_with(['/', '?', '!'])
            || self_closing
        {
            continue;
        }
        let closing = format!("</{name}>");
        if let Some(stop) = xml[at..].find(&closing) {
            found.push(&xml[at..at + stop]);
        }
    }
    found
}

pub fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

pub struct S3 {
    curl: Curl,
    base: String,
    prefix: String,
}

impl S3 {
    pub fn new(
        endpoint: &str,
        bucket: &str,
        region: &str,
        prefix: &str,
        credentials: &Credentials,
        work: PathBuf,
    ) -> Result<Self> {
        check_s3_curl(curl_version()?)?;
        let endpoint = http_url(endpoint)?;
        let name = |s: &str, extra: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
        };
        ensure!(name(bucket, ".-_"), "invalid bucket name {bucket:?}");
        ensure!(name(region, "-"), "invalid region {region:?}");
        let prefix = prefix.trim_matches('/');
        ensure!(
            prefix.is_empty() || name(prefix, "._-/") && !prefix.contains("//"),
            "invalid prefix {prefix:?}: letters, digits, . _ - and / only"
        );
        ensure!(
            !credentials.user.is_empty() && !credentials.secret.is_empty(),
            "the access key and the secret key are needed"
        );
        Ok(Self {
            curl: Curl {
                work,
                user: Some(format!("{}:{}", credentials.user, credentials.secret)),
                sigv4: Some(format!("aws:amz:{region}:s3")),
            },
            base: format!("{}/{bucket}", endpoint.trim_end_matches('/')),
            prefix: if prefix.is_empty() {
                String::new()
            } else {
                format!("{prefix}/")
            },
        })
    }

    fn url(&self, name: &str) -> String {
        format!("{}/{}{name}", self.base, self.prefix)
    }
}

impl Store for S3 {
    fn list(&self) -> Result<BTreeMap<String, String>> {
        let mut found = BTreeMap::new();
        let mut after: Option<String> = None;
        loop {
            let mut query = format!("list-type=2&prefix={}", encode(&self.prefix));
            if let Some(key) = &after {
                query.push_str(&format!("&start-after={}", encode(key)));
            }
            let response = self.curl.run(Request {
                method: "GET",
                url: format!("{}?{query}", self.base),
                headers: Vec::new(),
                upload: None,
                output: None,
            })?;
            if response.status != 200 {
                return Err(response.error("cannot list the bucket"));
            }
            let xml = String::from_utf8_lossy(&response.body);
            let mut last = None;
            for item in elements(&xml, "Contents") {
                let key = elements(item, "Key").first().map(|k| unescape(k));
                let etag = elements(item, "ETag").first().map(|e| unescape(e));
                let (Some(key), Some(etag)) = (key, etag) else {
                    continue;
                };
                if let Some(name) = key.strip_prefix(&self.prefix) {
                    found.insert(name.to_owned(), etag);
                }
                last = Some(key);
            }
            let truncated = elements(&xml, "IsTruncated").first() == Some(&"true");
            match last {
                Some(key) if truncated => after = Some(key),
                _ => return Ok(found),
            }
        }
    }

    fn get(&self, name: &str, to: &Path) -> Result<()> {
        let response = self.curl.run(Request {
            method: "GET",
            url: self.url(name),
            headers: Vec::new(),
            upload: None,
            output: Some(to),
        })?;
        if response.status != 200 {
            let _ = fs::remove_file(to);
            return Err(response.error(&format!("cannot read {name}")));
        }
        Ok(())
    }

    fn put(&self, name: &str, from: &Path, expect: Option<&str>) -> Result<Written> {
        let condition = match expect {
            Some(etag) => format!("If-Match: {etag}"),
            None => "If-None-Match: *".to_owned(),
        };
        let response = self.curl.run(Request {
            method: "PUT",
            url: self.url(name),
            headers: vec![condition],
            upload: Some(from),
            output: None,
        })?;
        match response.status {
            200 | 201 | 204 => Ok(Written::Done(response.header("etag"))),
            // 409: a conditional write raced with another one.
            412 | 409 => Ok(Written::Lost),
            _ => Err(response.error(&format!("cannot write {name}"))),
        }
    }

    fn delete(&self, name: &str) -> Result<()> {
        let response = self.curl.run(Request {
            method: "DELETE",
            url: self.url(name),
            headers: Vec::new(),
            upload: None,
            output: None,
        })?;
        match response.status {
            200 | 204 | 404 => Ok(()),
            _ => Err(response.error(&format!("cannot delete {name}"))),
        }
    }
}

const PROPFIND: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:propfind xmlns:d=\"DAV:\"><d:prop><d:getetag/><d:resourcetype/></d:prop></d:propfind>\n";

pub struct Webdav {
    curl: Curl,
    /// Ends with `/`.
    base: String,
}

impl Webdav {
    pub fn new(url: &str, credentials: Option<&Credentials>, work: PathBuf) -> Result<Self> {
        let url = http_url(url)?;
        Ok(Self {
            curl: Curl {
                work,
                user: credentials
                    .filter(|c| !c.user.is_empty())
                    .map(|c| format!("{}:{}", c.user, c.secret)),
                sigv4: None,
            },
            base: format!("{}/", url.trim_end_matches('/')),
        })
    }

    fn simple(&self, method: &str, url: String, headers: Vec<String>) -> Result<Response> {
        self.curl.run(Request {
            method,
            url,
            headers,
            upload: None,
            output: None,
        })
    }

    /// Creates the collection `dir` (`""`: the base folder itself, `files/`…).
    fn mkcol(&self, dir: &str) -> Result<()> {
        let response = self.simple("MKCOL", format!("{}{dir}", self.base), Vec::new())?;
        match response.status {
            // 405: it already exists.
            200 | 201 | 204 | 405 => Ok(()),
            _ => Err(response.error(&format!("cannot create the folder {}{dir}", self.base))),
        }
    }
}

impl Store for Webdav {
    fn list(&self) -> Result<BTreeMap<String, String>> {
        let body = self
            .curl
            .work
            .join(format!("propfind-{}", std::process::id()));
        fs::create_dir_all(&self.curl.work)?;
        fs::write(&body, PROPFIND)?;
        let mut found = BTreeMap::new();
        for dir in ["", "files/", "devices/"] {
            let url = format!("{}{dir}", self.base);
            let response = self.curl.run(Request {
                method: "PROPFIND",
                url: url.clone(),
                headers: vec![
                    "Depth: 1".into(),
                    "Content-Type: application/xml; charset=utf-8".into(),
                ],
                upload: Some(&body),
                output: None,
            });
            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    let _ = fs::remove_file(&body);
                    return Err(e);
                }
            };
            match response.status {
                207 => {}
                404 => continue,
                _ => {
                    let _ = fs::remove_file(&body);
                    return Err(response.error(&format!("cannot list {url}")));
                }
            }
            let xml = String::from_utf8_lossy(&response.body);
            for item in elements(&xml, "response") {
                let Some(href) = elements(item, "href").first().map(|h| unescape(h)) else {
                    continue;
                };
                let kind = elements(item, "resourcetype");
                if kind.first().is_some_and(|k| k.contains("collection")) {
                    continue;
                }
                let leaf = decode(href.trim_end_matches('/').rsplit('/').next().unwrap_or(""));
                if let Some(etag) = elements(item, "getetag").first().map(|e| unescape(e)) {
                    found.insert(format!("{dir}{leaf}"), etag);
                }
            }
        }
        let _ = fs::remove_file(&body);
        Ok(found)
    }

    fn get(&self, name: &str, to: &Path) -> Result<()> {
        let response = self.curl.run(Request {
            method: "GET",
            url: format!("{}{name}", self.base),
            headers: Vec::new(),
            upload: None,
            output: Some(to),
        })?;
        if response.status != 200 {
            let _ = fs::remove_file(to);
            return Err(response.error(&format!("cannot read {name}")));
        }
        Ok(())
    }

    fn put(&self, name: &str, from: &Path, expect: Option<&str>) -> Result<Written> {
        let condition = match expect {
            Some(etag) => format!("If-Match: {etag}"),
            None => "If-None-Match: *".to_owned(),
        };
        let mut created = false;
        loop {
            let response = self.curl.run(Request {
                method: "PUT",
                url: format!("{}{name}", self.base),
                headers: vec![condition.clone()],
                upload: Some(from),
                output: None,
            })?;
            match response.status {
                200 | 201 | 204 => return Ok(Written::Done(response.header("etag"))),
                412 => return Ok(Written::Lost),
                // 409: the folder is missing.
                409 if !created => {
                    created = true;
                    self.mkcol("")?;
                    if let Some((dir, _)) = name.rsplit_once('/') {
                        self.mkcol(&format!("{dir}/"))?;
                    }
                }
                _ => return Err(response.error(&format!("cannot write {name}"))),
            }
        }
    }

    fn delete(&self, name: &str) -> Result<()> {
        let response = self.simple("DELETE", format!("{}{name}", self.base), Vec::new())?;
        match response.status {
            200 | 204 | 404 => Ok(()),
            _ => Err(response.error(&format!("cannot delete {name}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_go_to_curl_on_its_input_only() {
        let curl = Curl {
            work: PathBuf::from("w"),
            user: Some("AKID:se\"cr\\et".into()),
            sigv4: Some("aws:amz:us-east-1:s3".into()),
        };
        let args: Vec<_> = Curl::command()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--config", "-"]);
        let request = Request {
            method: "PUT",
            url: "https://s3.example.com/b/files/x".into(),
            headers: vec!["If-Match: \"abc\"".into()],
            upload: Some(Path::new("up")),
            output: None,
        };
        let config = curl
            .config(&request, Path::new("o"), Path::new("h"))
            .unwrap();
        assert!(
            config.contains("user = \"AKID:se\\\"cr\\\\et\"\n"),
            "{config}"
        );
        assert!(
            config.contains("header = \"If-Match: \\\"abc\\\"\"\n"),
            "{config}"
        );
        let bad = Curl {
            user: Some("a:b\nurl = \"https://evil\"".into()),
            ..curl
        };
        assert!(
            bad.config(&request, Path::new("o"), Path::new("h"))
                .is_err()
        );
    }

    #[test]
    fn refuses_a_curl_too_old_for_s3() {
        assert_eq!(
            parse_version("curl 8.7.1 (x86_64-apple-darwin25.0) libcurl/8.7.1"),
            Some((8, 7, 1))
        );
        assert_eq!(parse_version("curl 7.88.1-DEV (x)"), Some((7, 88, 1)));
        assert_eq!(parse_version("nonsense"), None);
        assert!(check_s3_curl((8, 3, 0)).is_ok());
        let old = check_s3_curl((8, 2, 1)).unwrap_err().to_string();
        assert!(old.contains("8.3.0") && old.contains("8.2.1"), "{old}");
    }

    #[test]
    fn reads_xml_of_any_namespace() {
        let xml = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:"><d:response><d:href>/dav/files/ab%2Dc</d:href><d:propstat><d:prop><d:getetag>&quot;e1&quot;</d:getetag><d:resourcetype/></d:prop></d:propstat></d:response><D:response><D:href>/dav/</D:href><D:resourcetype><D:collection/></D:resourcetype></D:response></d:multistatus>"#;
        let responses = elements(xml, "response");
        assert_eq!(responses.len(), 2);
        assert_eq!(elements(responses[0], "href"), ["/dav/files/ab%2Dc"]);
        assert_eq!(unescape(elements(responses[0], "getetag")[0]), "\"e1\"");
        assert!(elements(responses[0], "collection").is_empty());
        assert_eq!(decode("ab%2Dc"), "ab-c");
        assert_eq!(encode("files/a b"), "files%2Fa%20b");
    }
}
