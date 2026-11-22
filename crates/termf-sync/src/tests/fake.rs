//! A throwaway HTTP server on 127.0.0.1 for the S3 and WebDAV remotes: objects in memory, conditional writes
//! as the real servers do them, and for S3 each request's SigV4 signature checked as AWS checks it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub const ACCESS: &str = "AKIDTERMFTEST";
pub const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
pub const REGION: &str = "eu-west-3";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    S3,
    Webdav,
}

#[derive(Default)]
pub struct Store {
    /// By path without its leading `/`.
    pub files: BTreeMap<String, (Vec<u8>, String)>,
    /// WebDAV collections, ending with `/`.
    pub dirs: BTreeSet<String>,
    version: u64,
    pub ignore_conditions: bool,
    /// Honors `If-None-Match` but not `If-Match`.
    pub ignore_if_match: bool,
    pub bad_signatures: u32,
    pub requests: u32,
}

pub struct Fake {
    pub url: String,
    pub store: Arc<Mutex<Store>>,
}

impl Fake {
    pub fn start(kind: Kind) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let store = Arc::new(Mutex::new(Store::default()));
        // The user's own folder exists on a WebDAV server; what TermF puts in it may not yet.
        store
            .lock()
            .unwrap()
            .dirs
            .extend([String::new(), "dav/".into()]);
        let shared = store.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let store = shared.clone();
                std::thread::spawn(move || {
                    let _ = handle(stream, kind, &store);
                });
            }
        });
        Fake { url, store }
    }
}

struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

fn read(stream: &TcpStream) -> std::io::Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_lowercase(), v.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    Ok(Request {
        method,
        path: path.to_owned(),
        query: query.to_owned(),
        headers,
        body,
    })
}

fn respond(mut stream: TcpStream, status: u16, headers: &[(&str, String)], body: &[u8]) {
    let mut head = format!(
        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn handle(stream: TcpStream, kind: Kind, store: &Mutex<Store>) -> std::io::Result<()> {
    let request = read(&stream)?;
    let mut store = store.lock().unwrap();
    store.requests += 1;
    if kind == Kind::S3
        && let Err(why) = verify(&request, SECRET, REGION)
    {
        store.bad_signatures += 1;
        let xml =
            format!("<Error><Code>SignatureDoesNotMatch</Code><Message>{why}</Message></Error>");
        respond(stream, 403, &[], xml.as_bytes());
        return Ok(());
    }
    let key = request.path.trim_start_matches('/').to_owned();
    match (kind, request.method.as_str()) {
        (Kind::S3, "GET") if request.query.contains("list-type=2") => {
            let params = query_params(&request.query);
            let bucket = format!("{key}/");
            let prefix = format!(
                "{bucket}{}",
                params.get("prefix").cloned().unwrap_or_default()
            );
            let after = params
                .get("start-after")
                .map(|a| format!("{bucket}{a}"))
                .unwrap_or_default();
            // Two keys per page, so that every listing goes through the pagination.
            let page: Vec<_> = store
                .files
                .iter()
                .filter(|(k, _)| k.starts_with(&prefix) && **k > after)
                .take(3)
                .collect();
            let truncated = page.len() > 2;
            let mut xml = String::from("<?xml version=\"1.0\"?><ListBucketResult>");
            for (k, (_, etag)) in page.iter().take(2) {
                let etag = etag.replace('"', "&quot;");
                xml.push_str(&format!(
                    "<Contents><Key>{}</Key><ETag>{etag}</ETag></Contents>",
                    &k[bucket.len()..]
                ));
            }
            xml.push_str(&format!(
                "<IsTruncated>{truncated}</IsTruncated></ListBucketResult>"
            ));
            respond(stream, 200, &[], xml.as_bytes());
        }
        (Kind::Webdav, "PROPFIND") => {
            let dir = if key.is_empty() || key.ends_with('/') {
                key.clone()
            } else {
                format!("{key}/")
            };
            if !store.dirs.contains(&dir) {
                respond(stream, 404, &[], b"");
                return Ok(());
            }
            let mut xml = format!(
                "<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>/{dir}</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat></D:response>"
            );
            for sub in store.dirs.iter().filter(|d| child(d, &dir)) {
                xml.push_str(&format!(
                    "<D:response><D:href>/{sub}</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat></D:response>"
                ));
            }
            for (path, (_, etag)) in store.files.iter().filter(|(p, _)| child(p, &dir)) {
                xml.push_str(&format!(
                    "<D:response><D:href>/{path}</D:href><D:propstat><D:prop><D:getetag>{etag}</D:getetag><D:resourcetype/></D:prop></D:propstat></D:response>"
                ));
            }
            xml.push_str("</D:multistatus>");
            respond(stream, 207, &[], xml.as_bytes());
        }
        (Kind::Webdav, "MKCOL") => {
            let dir = format!("{}/", key.trim_end_matches('/'));
            let parent = parent(dir.trim_end_matches('/'));
            if store.dirs.contains(&dir) {
                respond(stream, 405, &[], b"");
            } else if !store.dirs.contains(&parent) {
                respond(stream, 409, &[], b"");
            } else {
                store.dirs.insert(dir);
                respond(stream, 201, &[], b"");
            }
        }
        (_, "GET") => match store.files.get(&key) {
            Some((bytes, etag)) => respond(stream, 200, &[("ETag", etag.clone())], bytes),
            None => respond(stream, 404, &[], b"<Error><Code>NoSuchKey</Code></Error>"),
        },
        (_, "PUT") => {
            if kind == Kind::Webdav && !store.dirs.contains(&parent(&key)) {
                respond(stream, 409, &[], b"");
                return Ok(());
            }
            let current = store.files.get(&key).map(|(_, e)| e.clone());
            let refused = !store.ignore_conditions
                && (request.header("if-none-match") == Some("*") && current.is_some()
                    || !store.ignore_if_match
                        && request
                            .header("if-match")
                            .is_some_and(|m| Some(m) != current.as_deref()));
            if refused {
                respond(
                    stream,
                    412,
                    &[],
                    b"<Error><Code>PreconditionFailed</Code></Error>",
                );
                return Ok(());
            }
            store.version += 1;
            let etag = format!("\"v{}\"", store.version);
            store.files.insert(key, (request.body, etag.clone()));
            respond(stream, 200, &[("ETag", etag)], b"");
        }
        (_, "HEAD") => match store.files.get(&key) {
            Some((_, etag)) => respond(stream, 200, &[("ETag", etag.clone())], b""),
            None => respond(stream, 404, &[], b""),
        },
        (_, "DELETE") => {
            let current = store.files.get(&key).map(|(_, e)| e.clone());
            if request
                .header("if-match")
                .is_some_and(|m| Some(m) != current.as_deref())
            {
                respond(stream, 412, &[], b"");
                return Ok(());
            }
            let status = if store.files.remove(&key).is_some() {
                204
            } else {
                404
            };
            respond(stream, status, &[], b"");
        }
        _ => respond(stream, 405, &[], b""),
    }
    Ok(())
}

fn parent(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/"),
        None => String::new(),
    }
}

/// `path` is directly in the collection `dir`.
fn child(path: &str, dir: &str) -> bool {
    path != dir
        && path
            .strip_prefix(dir)
            .is_some_and(|rest| !rest.trim_end_matches('/').contains('/'))
}

fn query_params(query: &str) -> BTreeMap<String, String> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect()
}

fn decode(text: &str) -> String {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

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

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap();
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SigV4's canonical request: the query decoded then encoded again and sorted, as AWS reads it.
pub fn canonical_request(
    method: &str,
    path: &str,
    query: &str,
    headers: &[(String, String)],
    signed: &[&str],
    payload: &str,
) -> String {
    let mut params: Vec<(String, String)> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (encode(&decode(k)), encode(&decode(v)))
        })
        .collect();
    params.sort();
    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let canonical_headers: String = signed
        .iter()
        .map(|name| {
            let value = headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            format!("{name}:{value}\n")
        })
        .collect();
    format!(
        "{method}\n{path}\n{query}\n{canonical_headers}\n{}\n{payload}",
        signed.join(";")
    )
}

pub fn string_to_sign(amz_date: &str, scope: &str, canonical: &str) -> String {
    format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex(&Sha256::digest(canonical))
    )
}

pub fn signature(secret: &str, date: &str, region: &str, service: &str, to_sign: &str) -> String {
    let key = hmac(format!("AWS4{secret}").as_bytes(), date);
    let key = hmac(&key, region);
    let key = hmac(&key, service);
    let key = hmac(&key, "aws4_request");
    hex(&hmac(&key, to_sign))
}

fn verify(request: &Request, secret: &str, region: &str) -> Result<(), String> {
    let auth = request
        .header("authorization")
        .ok_or("no Authorization header")?;
    let rest = auth
        .strip_prefix("AWS4-HMAC-SHA256 ")
        .ok_or("not AWS4-HMAC-SHA256")?;
    let field = |name: &str| {
        rest.split(',')
            .map(str::trim)
            .find_map(|f| f.strip_prefix(&format!("{name}=")))
            .ok_or(format!("no {name}"))
    };
    let credential = field("Credential")?;
    let signed: Vec<&str> = field("SignedHeaders")?.split(';').collect();
    let given = field("Signature")?;
    let mut scope = credential.splitn(2, '/');
    let access = scope.next().unwrap_or_default();
    let scope = scope.next().unwrap_or_default();
    if access != ACCESS {
        return Err(format!("unknown access key {access}"));
    }
    let parts: Vec<&str> = scope.split('/').collect();
    if parts.len() != 4 || parts[1] != region || parts[2] != "s3" || parts[3] != "aws4_request" {
        return Err(format!("bad scope {scope}"));
    }
    if !signed.contains(&"host") {
        return Err("host is not signed".into());
    }
    let payload = request
        .header("x-amz-content-sha256")
        .ok_or("no x-amz-content-sha256")?;
    if payload != "UNSIGNED-PAYLOAD" && payload != hex(&Sha256::digest(&request.body)) {
        return Err("the payload hash is wrong".into());
    }
    let amz_date = request.header("x-amz-date").ok_or("no x-amz-date")?;
    let canonical = canonical_request(
        &request.method,
        &request.path,
        &request.query,
        &request.headers,
        &signed,
        payload,
    );
    let expected = signature(
        secret,
        parts[0],
        region,
        "s3",
        &string_to_sign(amz_date, scope, &canonical),
    );
    if expected != given {
        return Err("signature mismatch".into());
    }
    Ok(())
}

#[test]
fn signs_the_official_vector() {
    // aws-signing-test-suite/v4/get-vanilla (awslabs/aws-c-auth): AKIDEXAMPLE, 2015-08-30T12:36:00Z.
    let headers = vec![
        ("host".to_owned(), "example.amazonaws.com".to_owned()),
        ("x-amz-date".to_owned(), "20150830T123600Z".to_owned()),
    ];
    let canonical = canonical_request(
        "GET",
        "/",
        "",
        &headers,
        &["host", "x-amz-date"],
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    assert_eq!(
        canonical,
        "GET\n/\n\nhost:example.amazonaws.com\nx-amz-date:20150830T123600Z\n\nhost;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    let to_sign = string_to_sign(
        "20150830T123600Z",
        "20150830/us-east-1/service/aws4_request",
        &canonical,
    );
    assert!(to_sign.ends_with("bb579772317eb040ac9ed261061d46c1f17a8133879d6129b6e1c25292927e63"));
    assert_eq!(
        signature(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "service",
            &to_sign
        ),
        "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
    );
}
