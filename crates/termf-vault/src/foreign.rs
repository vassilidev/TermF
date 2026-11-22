//! Files other password managers export, read into candidate items: Bitwarden (JSON, CSV), 1Password (`.1pux`,
//! CSV), Chrome, Edge, Firefox and Safari (CSV), KeePassXC (CSV) and KeePass (XML). Recognized by their content,
//! not their name; CSV columns by their header, not their place. An encrypted export is refused with how to make
//! a readable one. What has no field of its own (custom fields, a second TOTP) goes into the notes, never lost.

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use quick_xml::events::Event;
use serde_json::Value;
use zeroize::Zeroizing;

use crate::Secret;
use crate::item::Kind;
use crate::zip;

/// Far above any real export: a file that would fill the memory is refused.
const MAX_SIZE: usize = 64 << 20;

pub(crate) struct Candidate {
    pub kind: Kind,
    pub title: String,
    pub urls: Vec<String>,
    pub username: String,
    pub password: Option<Secret>,
    /// A base32 secret or an `otpauth://` link, as the other manager kept it.
    pub totp: Option<Secret>,
    pub notes: Option<Secret>,
    /// Its folder (group, vault, collection): `""` for none.
    pub folder: String,
    /// In the other manager's trash, which its export still holds.
    pub trash: bool,
}

pub(crate) struct Parsed {
    pub source: &'static str,
    pub candidates: Vec<Candidate>,
    /// Cards, identities, documents…: TermF has no place for them.
    pub unsupported: usize,
}

fn secret(text: &str) -> Option<Secret> {
    let text = text.trim_end_matches(['\r', '\n']);
    (!text.trim().is_empty()).then(|| Secret::new(text.to_owned()))
}

/// The notes, then each extra field as `name: value` on its own line.
fn notes(notes: &str, fields: &[(String, Zeroizing<String>)]) -> Option<Secret> {
    let mut text = Zeroizing::new(notes.trim_end().to_owned());
    for (name, value) in fields.iter().filter(|(_, v)| !v.trim().is_empty()) {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!("{name}: {}", value.as_str()));
    }
    secret(&text)
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.rsplit_once('@').map_or(host, |(_, h)| h)
}

/// A title for a line that has none (Firefox, Chrome on Android): the site, else the user name.
fn title(title: &str, urls: &[String], username: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.into();
    }
    urls.first()
        .map(|u| host(u).trim_start_matches("www.").to_owned())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| username.trim().to_owned())
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Parsed> {
    ensure!(bytes.len() <= MAX_SIZE, "this file is too large to import");
    if zip::is_zip(bytes) {
        let data = zip::read(bytes, "export.data", MAX_SIZE)?;
        return one_password(&json(&data)?);
    }
    ensure!(
        !bytes.starts_with(&[0x03, 0xd9, 0xa2, 0x9a]),
        "this is a KeePass database: export it from KeePassXC as XML or CSV, import that file, then delete it"
    );
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
        .map_err(|_| anyhow::anyhow!("TermF does not recognize this file"))?;
    match text.trim_start().chars().next() {
        Some('{') => {
            let value = json(text.as_bytes())?;
            if value.get("accounts").is_some() {
                one_password(&value)
            } else {
                bitwarden(&value)
            }
        }
        Some('<') => keepass(text),
        _ => csv(text),
    }
}

fn json(bytes: &[u8]) -> Result<Value> {
    // Not the parser's message: it may quote a password.
    serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("TermF does not recognize this file"))
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn bitwarden(v: &Value) -> Result<Parsed> {
    ensure!(
        v["encrypted"] != true,
        "this Bitwarden export is encrypted: export the vault again as .json (not encrypted), import that file, then delete it"
    );
    ensure!(v["items"].is_array(), "TermF does not recognize this file");
    let names = |key: &str| -> BTreeMap<String, String> {
        v[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| (text(&f["id"]), text(&f["name"])))
            .collect()
    };
    let (folders, collections) = (names("folders"), names("collections"));
    let mut parsed = Parsed {
        source: "bitwarden-json",
        candidates: Vec::new(),
        unsupported: 0,
    };
    for item in v["items"].as_array().into_iter().flatten() {
        let kind = match item["type"].as_u64() {
            Some(1) => Kind::Login,
            Some(2) => Kind::Note,
            _ => {
                parsed.unsupported += 1;
                continue;
            }
        };
        let folder = item["folderId"]
            .as_str()
            .and_then(|id| folders.get(id))
            .or_else(|| {
                let id = item["collectionIds"].get(0)?.as_str()?;
                collections.get(id)
            })
            .cloned()
            .unwrap_or_default();
        let login = &item["login"];
        let urls: Vec<String> = login["uris"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|u| text(&u["uri"]))
            .filter(|u| !u.is_empty())
            .collect();
        let fields: Vec<(String, Zeroizing<String>)> = item["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["value"].is_string())
            .map(|f| (text(&f["name"]), Zeroizing::new(text(&f["value"]))))
            .collect();
        let username = text(&login["username"]);
        parsed.candidates.push(Candidate {
            kind,
            title: title(&text(&item["name"]), &urls, &username),
            password: secret(&text(&login["password"])),
            totp: secret(&text(&login["totp"])),
            notes: notes(&text(&item["notes"]), &fields),
            urls,
            username,
            folder,
            trash: false,
        });
    }
    Ok(parsed)
}

fn one_password(v: &Value) -> Result<Parsed> {
    ensure!(
        v["accounts"].is_array(),
        "TermF does not recognize this file"
    );
    let mut parsed = Parsed {
        source: "1password-1pux",
        candidates: Vec::new(),
        unsupported: 0,
    };
    let vaults = v["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|a| a["vaults"].as_array().into_iter().flatten());
    for vault in vaults {
        let folder = text(&vault["attrs"]["name"]);
        for item in vault["items"].as_array().into_iter().flatten() {
            if item["state"] == "trashed" {
                continue;
            }
            let category = item["categoryUuid"].as_str().unwrap_or_default();
            let kind = match category {
                "001" | "005" => Kind::Login,
                "003" => Kind::Note,
                _ => {
                    parsed.unsupported += 1;
                    continue;
                }
            };
            let (overview, details) = (&item["overview"], &item["details"]);
            let mut urls: Vec<String> = overview["urls"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|u| text(&u["url"]))
                .filter(|u| !u.is_empty())
                .collect();
            if urls.is_empty() && !text(&overview["url"]).is_empty() {
                urls.push(text(&overview["url"]));
            }
            let login = |designation: &str| {
                details["loginFields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|f| f["designation"] == designation)
                    .map(|f| text(&f["value"]))
                    .unwrap_or_default()
            };
            let username = login("username");
            let mut password = login("password");
            if category == "005" {
                password = text(&details["password"]);
            }
            let (mut totp, mut fields) = (None, Vec::new());
            let section_fields = details["sections"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|s| s["fields"].as_array().into_iter().flatten());
            for field in section_fields {
                let value = &field["value"];
                let shown = value["totp"]
                    .as_str()
                    .or(value["string"].as_str())
                    .or(value["concealed"].as_str())
                    .or(value["url"].as_str())
                    .or(value["phone"].as_str())
                    .or(value["email"]["email_address"].as_str())
                    .unwrap_or_default();
                if value["totp"].is_string() && totp.is_none() {
                    totp = secret(shown);
                } else {
                    fields.push((text(&field["title"]), Zeroizing::new(shown.to_owned())));
                }
            }
            parsed.candidates.push(Candidate {
                kind,
                title: title(&text(&overview["title"]), &urls, &username),
                password: secret(&password),
                totp,
                notes: notes(&text(&details["notesPlain"]), &fields),
                urls,
                username,
                folder: folder.clone(),
                trash: false,
            });
        }
    }
    Ok(parsed)
}

/// RFC 4180: quoted fields with doubled quotes and line breaks inside, CRLF or LF.
fn csv_rows(text: &str) -> Zeroizing<Vec<Vec<String>>> {
    let mut rows = Zeroizing::new(Vec::new());
    let (mut row, mut field) = (Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn csv(text: &str) -> Result<Parsed> {
    let rows = csv_rows(text);
    let unknown = "TermF does not recognize this file: export it from your password manager as CSV, JSON, .1pux or KeePass XML";
    let Some(header) = rows.first() else {
        bail!(unknown);
    };
    let header: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();
    let has = |name: &str| header.iter().any(|h| h == name);
    let source = if has("login_password") {
        "bitwarden-csv"
    } else if has("httprealm") {
        "firefox-csv"
    } else if has("group") && has("title") {
        "keepassxc-csv"
    } else if has("otpauth") && has("archived") {
        "1password-csv"
    } else if has("title") && has("password") {
        "safari-csv"
    } else if has("name") && has("url") && has("password") {
        "chrome-csv"
    } else {
        bail!(unknown);
    };
    let column = |names: &[&str]| {
        names
            .iter()
            .find_map(|n| header.iter().position(|h| h == n))
    };
    let columns = [
        column(&["name", "title"]),
        column(&["login_uri", "url", "website"]),
        column(&["login_username", "username"]),
        column(&["login_password", "password"]),
        column(&["login_totp", "otpauth", "totp"]),
        column(&["notes", "note"]),
        column(&["folder", "collections", "group"]),
        column(&["type"]),
        column(&["fields"]),
    ];
    let mut parsed = Parsed {
        source,
        candidates: Vec::new(),
        unsupported: 0,
    };
    for row in rows.iter().skip(1) {
        if row.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        let [
            title_at,
            url,
            user,
            password,
            totp,
            note,
            folder,
            kind,
            fields,
        ] = columns.map(|at| at.and_then(|i| row.get(i)).map_or("", String::as_str));
        // Firefox keeps its account's sync keys as a login: not one of the user's.
        if url == "chrome://FirefoxAccounts" {
            continue;
        }
        let kind = match kind {
            "" | "login" => Kind::Login,
            "note" => Kind::Note,
            _ => {
                parsed.unsupported += 1;
                continue;
            }
        };
        let urls: Vec<String> = match source {
            // Bitwarden joins several addresses with commas in one cell.
            "bitwarden-csv" => url.split(',').map(|u| u.trim().to_owned()).collect(),
            _ => vec![url.trim().to_owned()],
        };
        let urls: Vec<String> = urls.into_iter().filter(|u| !u.is_empty()).collect();
        let mut folder = folder.trim().to_owned();
        if source == "keepassxc-csv" {
            // The path starts with the database's root group.
            folder = folder
                .split_once('/')
                .map_or("", |(_, rest)| rest)
                .to_owned();
        }
        if source == "bitwarden-csv" {
            folder = folder.split(',').next().unwrap_or_default().to_owned();
        }
        let extra: Vec<(String, Zeroizing<String>)> = fields
            .lines()
            .filter_map(|line| line.rsplit_once(": "))
            .map(|(n, v)| (n.to_owned(), Zeroizing::new(v.to_owned())))
            .collect();
        parsed.candidates.push(Candidate {
            kind,
            title: title(title_at, &urls, user),
            urls,
            username: user.trim().to_owned(),
            password: secret(password),
            totp: secret(totp),
            notes: notes(note, &extra),
            trash: source == "keepassxc-csv"
                && (folder == "Recycle Bin" || folder.starts_with("Recycle Bin/")),
            folder,
        });
    }
    Ok(parsed)
}

/// A KeePass entry's TOTP as an `otpauth://` link, from whichever of its fields holds it.
fn keepass_totp(fields: &BTreeMap<String, Zeroizing<String>>) -> Option<Secret> {
    let get = |key: &str| {
        fields
            .get(key)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let link = |secret: &str, digits: &str, period: &str, algorithm: &str| {
        Secret::new(format!(
            "otpauth://totp/KeePass?secret={secret}&digits={digits}&period={period}&algorithm={algorithm}"
        ))
    };
    if let Some(seed) = get("TOTP Seed") {
        let settings = get("TOTP Settings").unwrap_or_default();
        let (period, digits) = settings.split_once(';').unwrap_or(("30", "6"));
        let digits = if digits.parse::<u32>().is_ok() {
            digits
        } else {
            "6"
        };
        let period = if period.parse::<u32>().is_ok() {
            period
        } else {
            "30"
        };
        return Some(link(&seed, digits, period, "SHA1"));
    }
    if let Some(otp) = get("otp") {
        if otp.to_lowercase().starts_with("otpauth://") {
            return Some(Secret::new(otp));
        }
        // KeeOtp: key=<base32>&size=6&step=30&otpHashMode=Sha256
        let query: BTreeMap<String, String> = otp
            .split('&')
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.to_lowercase(), v.to_owned()))
            .collect();
        let key = query.get("key")?;
        let algorithm =
            query
                .get("otphashmode")
                .map_or("SHA1", |m| match m.to_lowercase().as_str() {
                    "sha256" => "SHA256",
                    "sha512" => "SHA512",
                    _ => "SHA1",
                });
        let size = query.get("size").map_or("6", String::as_str);
        let step = query.get("step").map_or("30", String::as_str);
        return Some(link(key, size, step, algorithm));
    }
    let seed = get("TimeOtp-Secret-Base32")?;
    let algorithm = match get("TimeOtp-Algorithm").unwrap_or_default().as_str() {
        "HMAC-SHA-256" => "SHA256",
        "HMAC-SHA-512" => "SHA512",
        _ => "SHA1",
    };
    let digits = get("TimeOtp-Length").unwrap_or_else(|| "6".into());
    let period = get("TimeOtp-Period").unwrap_or_else(|| "30".into());
    Some(link(&seed, &digits, &period, algorithm))
}

/// Fields of a KeePass entry that are its TOTP or one of TermF's own fields, so not repeated in the notes.
fn known_field(key: &str) -> bool {
    [
        "Title",
        "UserName",
        "Password",
        "URL",
        "Notes",
        "otp",
        "TOTP Seed",
        "TOTP Settings",
    ]
    .contains(&key)
        || key.starts_with("TimeOtp-")
        || key.starts_with("KP2A_URL")
}

struct Entry {
    groups: Vec<(String, String)>,
    fields: BTreeMap<String, Zeroizing<String>>,
}

fn keepass(xml: &str) -> Result<Parsed> {
    let unknown = "TermF does not recognize this file";
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack: Vec<String> = Vec::new();
    let mut text = Zeroizing::new(String::new());
    let (mut bin, mut bin_on) = (String::new(), true);
    let mut groups: Vec<(String, String)> = Vec::new();
    let mut entry: Option<BTreeMap<String, Zeroizing<String>>> = None;
    let (mut key, mut value) = (String::new(), Zeroizing::new(String::new()));
    let mut entries = Vec::new();
    let mut root = false;
    loop {
        let event = reader.read_event().map_err(|_| anyhow::anyhow!(unknown))?;
        let in_history = stack.iter().any(|s| s == "History");
        match event {
            Event::Start(e) => {
                let name = e.name().as_ref().to_owned();
                if name == "Value"
                    && e.try_get_attribute("Protected")
                        .ok()
                        .flatten()
                        .is_some_and(|a| a.value.eq_ignore_ascii_case("true"))
                {
                    bail!(
                        "this KeePass XML holds protected values: export it again from KeePassXC (Database > Export > XML)"
                    );
                }
                let parent = stack.last().map(String::as_str);
                match (name.as_str(), parent) {
                    ("KeePassFile", None) => root = true,
                    ("Group", Some("Root" | "Group")) => groups.push(Default::default()),
                    ("Entry", Some("Group")) => entry = Some(BTreeMap::new()),
                    _ => {}
                }
                stack.push(name);
                text.clear();
            }
            Event::Empty(e) if e.name().as_ref() == "Value" && !in_history => value.clear(),
            Event::Text(t) => text.push_str(&t.xml10_content()),
            Event::CData(t) => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) => {
                let name = r.xml10_content();
                let c = match name.as_ref() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => r.resolve_char_ref().ok().flatten(),
                };
                text.extend(c);
            }
            Event::End(_) => {
                let name = stack.pop().unwrap_or_default();
                let parent = stack.last().map(String::as_str);
                match (name.as_str(), parent) {
                    ("RecycleBinUUID", Some("Meta")) => bin = text.trim().to_owned(),
                    ("RecycleBinEnabled", Some("Meta")) => {
                        bin_on = text.trim().eq_ignore_ascii_case("true")
                    }
                    ("UUID", Some("Group")) => {
                        if let Some(g) = groups.last_mut() {
                            g.0 = text.trim().to_owned();
                        }
                    }
                    ("Name", Some("Group")) => {
                        if let Some(g) = groups.last_mut() {
                            g.1 = text.trim().to_owned();
                        }
                    }
                    ("Key", Some("String")) => key = text.trim().to_owned(),
                    ("Value", Some("String")) => value = Zeroizing::new(text.to_string()),
                    ("String", Some("Entry")) if !in_history => {
                        if let Some(fields) = entry.as_mut() {
                            fields.insert(std::mem::take(&mut key), std::mem::take(&mut value));
                        }
                    }
                    ("Entry", Some("Group")) => {
                        if let Some(fields) = entry.take() {
                            entries.push(Entry {
                                groups: groups.clone(),
                                fields,
                            });
                        }
                    }
                    ("Group", _) => {
                        groups.pop();
                    }
                    _ => {}
                }
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(root, unknown);
    let mut parsed = Parsed {
        source: "keepass-xml",
        candidates: Vec::new(),
        unsupported: 0,
    };
    for e in entries {
        let get = |k: &str| e.fields.get(k).map_or(String::new(), |v| v.to_string());
        let mut urls: Vec<String> = Some(get("URL"))
            .into_iter()
            .chain(
                e.fields
                    .iter()
                    .filter(|(k, _)| k.starts_with("KP2A_URL"))
                    .map(|(_, v)| v.to_string()),
            )
            .map(|u| u.trim().to_owned())
            .filter(|u| !u.is_empty())
            .collect();
        urls.dedup();
        let extra: Vec<(String, Zeroizing<String>)> = e
            .fields
            .iter()
            .filter(|(k, _)| !known_field(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let username = get("UserName");
        let folder: Vec<&str> = e.groups.iter().skip(1).map(|g| g.1.as_str()).collect();
        parsed.candidates.push(Candidate {
            kind: Kind::Login,
            title: title(&get("Title"), &urls, &username),
            password: secret(&get("Password")),
            totp: keepass_totp(&e.fields),
            notes: notes(&get("Notes"), &extra),
            trash: bin_on && !bin.is_empty() && e.groups.iter().any(|g| g.0 == bin),
            folder: folder.join("/"),
            urls,
            username,
        });
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_csv_as_rfc_4180() {
        let rows = csv_rows("a,\"b, \"\"c\"\"\nd\",\r\nx,y,z");
        assert_eq!(
            *rows,
            vec![
                vec!["a".to_owned(), "b, \"c\"\nd".into(), "".into()],
                vec!["x".into(), "y".into(), "z".into()],
            ]
        );
    }

    #[test]
    fn refuses_what_it_cannot_read_saying_how() {
        let encrypted =
            br#"{"encrypted": true, "passwordProtected": true, "salt": "x", "data": "2.x|y|z"}"#;
        let err = parse(encrypted).err().unwrap().to_string();
        assert!(
            err.contains("export the vault again as .json (not encrypted)"),
            "{err}"
        );
        let err = parse(b"\x03\xd9\xa2\x9a\x67\xfb\x4b\xb5")
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("KeePass database"), "{err}");
        assert!(parse(b"just,some,columns\n1,2,3").is_err());
        let protected = "<KeePassFile><Root><Group><Entry><String><Key>Password</Key><Value Protected=\"True\">eGs=</Value></String></Entry></Group></Root></KeePassFile>";
        assert!(
            parse(protected.as_bytes())
                .err()
                .unwrap()
                .to_string()
                .contains("protected values")
        );
    }

    #[test]
    fn turns_each_keepass_totp_field_into_a_link() {
        let fields = |pairs: &[(&str, &str)]| -> BTreeMap<String, Zeroizing<String>> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), Zeroizing::new(v.to_string())))
                .collect()
        };
        let link = |pairs| keepass_totp(&fields(pairs)).map(|s| s.expose().to_owned());
        assert_eq!(
            link(&[("TOTP Seed", "JBSWY3DPEHPK3PXP"), ("TOTP Settings", "60;8")]).unwrap(),
            "otpauth://totp/KeePass?secret=JBSWY3DPEHPK3PXP&digits=8&period=60&algorithm=SHA1"
        );
        assert_eq!(
            link(&[(
                "otp",
                "key=JBSWY3DPEHPK3PXP&size=6&step=30&otpHashMode=Sha256"
            )])
            .unwrap(),
            "otpauth://totp/KeePass?secret=JBSWY3DPEHPK3PXP&digits=6&period=30&algorithm=SHA256"
        );
        assert_eq!(
            link(&[
                ("TimeOtp-Secret-Base32", "JBSWY3DPEHPK3PXP"),
                ("TimeOtp-Algorithm", "HMAC-SHA-512")
            ])
            .unwrap(),
            "otpauth://totp/KeePass?secret=JBSWY3DPEHPK3PXP&digits=6&period=30&algorithm=SHA512"
        );
        assert!(link(&[("Title", "x")]).is_none());
    }
}
