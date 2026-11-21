use std::collections::HashSet;

use base64ct::Encoding;

use super::*;
use crate::formats::{self, parse, parse_public, thumbprint};

const ALGOS: [Algo; 8] = [
    Algo::Ed25519,
    Algo::P256,
    Algo::P384,
    Algo::P521,
    Algo::Rsa2048,
    Algo::Rsa3072,
    Algo::Rsa4096,
    Algo::X25519,
];
const FORMATS: [Format; 8] = [
    Format::Openssh,
    Format::Pkcs1,
    Format::Sec1,
    Format::Pkcs8,
    Format::Spki,
    Format::Jwk,
    Format::Jwks,
    Format::Ppk,
];

// Throwaway keys made by OpenSSL 3.6 (`ecparam -genkey`, `pkcs8 -topk8` with "fixture", `ec -aes256`).
const OPENSSL_EC: &str = "-----BEGIN EC PARAMETERS-----
BggqhkjOPQMBBw==
-----END EC PARAMETERS-----
-----BEGIN EC PRIVATE KEY-----
MHcCAQEEIPtByn1NZWSJ/wB3ixra0gpSTEmwv1eNazKdZR6DNVZLoAoGCCqGSM49
AwEHoUQDQgAEL7l4GPvgvHWxShjOjsYCl3cWx2pMCo1zmveGlxgAeUSBaxQ3dAUy
B7OcvgHT6ap8W1exJgPgjUoDbwmt95wtaA==
-----END EC PRIVATE KEY-----
";
const OPENSSL_EC_ENCRYPTED: &str = "-----BEGIN ENCRYPTED PRIVATE KEY-----
MIH0MF8GCSqGSIb3DQEFDTBSMDEGCSqGSIb3DQEFDDAkBBAPoUdFd77PGxWAqaJT
e4x0AgIIADAMBggqhkiG9w0CCQUAMB0GCWCGSAFlAwQBKgQQ1uWAbXDZD68Zi5IN
+hBElASBkF3HSP5oKf20DU+usGJrjtTvSRuGxX3Q1Vpuz0+Sbat+4cTT/Ew/a8Oj
CdDEWH+VDmsE1WoAJgX3sDdowBRbfOUTie1x+yvp36dF3FPL9KNzFnvyyaHC2Sb0
LQcHmLyInihKm9siXUMAuaKbvVArszLXC96XxP38TIBJjx4mQ8JFoIyoBIbVkxGZ
DlQMwSlaKw==
-----END ENCRYPTED PRIVATE KEY-----
";
const OPENSSL_EC_PUBLIC: &str = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEL7l4GPvgvHWxShjOjsYCl3cWx2pM
Co1zmveGlxgAeUSBaxQ3dAUyB7OcvgHT6ap8W1exJgPgjUoDbwmt95wtaA==
-----END PUBLIC KEY-----
";
const OPENSSL_ED25519: &str = "-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIFHSl4on9tYrgj3sztjglYJd6KBejZmmK7fQRXSJlu6T
-----END PRIVATE KEY-----
";
const OPENSSL_ED25519_PUBLIC: &str = "-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAD5joU2g3Zb4VY8issGSkEkyx2Z/y3cgPLnp9xU9KRAc=
-----END PUBLIC KEY-----
";
const OPENSSL_LEGACY: &str = "-----BEGIN EC PRIVATE KEY-----
Proc-Type: 4,ENCRYPTED
DEK-Info: AES-256-CBC,E51742400CC3C0C9481E92170E6D9903

AAAA
-----END EC PRIVATE KEY-----
";

fn scratch(name: &str) -> (Ssh, PathBuf) {
    let home = std::env::temp_dir().join(format!("termf-keys-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let out = home.join("out");
    fs::create_dir_all(&out).unwrap();
    (Ssh::new(home), out)
}

fn spec(
    usage: Usage,
    format: Format,
    name: &str,
    passphrase: Option<&str>,
    dir: Option<&Path>,
) -> Spec {
    Spec {
        usage,
        format,
        name: name.into(),
        comment: "alex@acme".into(),
        passphrase: passphrase.map(str::to_owned),
        dir: dir.map(|d| d.to_string_lossy().into_owned()),
    }
}

fn locked(error: anyhow::Error) -> Locked {
    *error
        .downcast_ref::<Locked>()
        .unwrap_or_else(|| panic!("not a passphrase error: {error:#}"))
}

fn supported(algo: Algo, format: Format) -> bool {
    let rsa = matches!(algo, Algo::Rsa2048 | Algo::Rsa3072 | Algo::Rsa4096);
    let ec = matches!(algo, Algo::P256 | Algo::P384 | Algo::P521);
    match format {
        Format::Openssh | Format::Ppk => algo != Algo::X25519,
        Format::Pkcs1 => rsa,
        Format::Sec1 => ec,
        _ => true,
    }
}

#[test]
fn every_algorithm_in_every_format_reads_back() {
    for algo in ALGOS {
        let secret = Secret::random(algo).unwrap();
        let spki = secret.spki().unwrap();
        assert!(secret.self_test().unwrap(), "{algo:?} signs or agrees");
        for format in FORMATS {
            let result = secret.encode(format, "alex@acme", None, Usage::Sign);
            if !supported(algo, format) {
                assert!(result.is_err(), "{algo:?} as {format:?}");
                continue;
            }
            let outputs = result.unwrap_or_else(|e| panic!("{algo:?} as {format:?}: {e:#}"));
            for output in &outputs {
                let read = match output.secret {
                    true => parse(&output.text, None).map(|p| p.secret.spki().unwrap()),
                    false => parse_public(&output.text),
                };
                let read = read
                    .unwrap_or_else(|e| panic!("{algo:?} as {format:?}{}: {e:#}", output.suffix));
                assert_eq!(read, spki, "{algo:?} as {format:?}{}", output.suffix);
            }
            if format == Format::Spki {
                assert!(outputs.iter().all(|o| !o.secret));
            }
            if format == Format::Sec1 {
                let der = pkcs8::SecretDocument::from_pem(&outputs[0].text).unwrap().1;
                let key = sec1::EcPrivateKey::try_from(der.as_bytes()).unwrap();
                assert!(key.parameters.is_some(), "OpenSSL needs the curve's name");
            }
        }
    }
}

#[test]
fn encrypted_formats_open_with_the_passphrase_only() {
    for algo in [Algo::Ed25519, Algo::P384, Algo::X25519] {
        let secret = Secret::random(algo).unwrap();
        for format in [Format::Openssh, Format::Pkcs8, Format::Ppk] {
            if !supported(algo, format) {
                continue;
            }
            let outputs = secret
                .encode(format, "alex@acme", Some("correct horse"), Usage::Ssh)
                .unwrap();
            let private = &outputs[0].text;
            assert!(outputs[0].secret);
            let parsed = parse(private, Some("correct horse")).unwrap();
            assert!(parsed.encrypted, "{algo:?} as {format:?}");
            assert_eq!(parsed.secret.spki().unwrap(), secret.spki().unwrap());
            assert_eq!(
                locked(parse(private, Some("wrong")).err().unwrap()),
                Locked::WrongPassphrase
            );
            assert_eq!(
                locked(parse(private, None).err().unwrap()),
                Locked::NeedsPassphrase
            );
        }
        for format in [
            Format::Pkcs1,
            Format::Sec1,
            Format::Jwk,
            Format::Jwks,
            Format::Spki,
        ] {
            assert!(secret.encode(format, "", Some("pass"), Usage::Ssh).is_err());
        }
    }
    let pkcs8 = Secret::random(Algo::P256)
        .unwrap()
        .encode(Format::Pkcs8, "", Some("p"), Usage::Tls)
        .unwrap();
    assert!(
        pkcs8[0]
            .text
            .starts_with("-----BEGIN ENCRYPTED PRIVATE KEY-----")
    );
}

#[test]
fn reads_keys_made_by_openssl() {
    let ec = parse(OPENSSL_EC, None).unwrap();
    assert_eq!(
        (ec.format, ec.secret.label().as_str()),
        (Format::Sec1, "ECDSA P-256")
    );
    assert_eq!(
        ec.secret.spki().unwrap(),
        parse_public(OPENSSL_EC_PUBLIC).unwrap()
    );
    let encrypted = parse(OPENSSL_EC_ENCRYPTED, Some("fixture")).unwrap();
    assert_eq!(encrypted.secret.spki().unwrap(), ec.secret.spki().unwrap());
    assert_eq!(
        locked(parse(OPENSSL_EC_ENCRYPTED, Some("nope")).err().unwrap()),
        Locked::WrongPassphrase
    );
    let ed = parse(OPENSSL_ED25519, None).unwrap();
    assert_eq!(
        ed.secret.spki().unwrap(),
        parse_public(OPENSSL_ED25519_PUBLIC).unwrap()
    );
    let legacy = format!("{:#}", parse(OPENSSL_LEGACY, Some("x")).err().unwrap());
    assert!(legacy.contains("PKCS#8"), "{legacy}");
}

#[test]
fn jwk_follows_the_rfcs() {
    // RFC 7638 §3.1.
    let n = "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw";
    let members = [
        ("e", "AQAB".to_owned()),
        ("kty", "RSA".to_owned()),
        ("n", n.to_owned()),
    ];
    assert_eq!(
        thumbprint(&members),
        "NzbLsXh8uDCcd-6MNwXF4W_7noWXFZAfHkxZsRGC9Xs"
    );

    // RFC 8037 A.1 and A.3.
    let ed = r#"{"kty":"OKP","crv":"Ed25519","d":"nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A","x":"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"}"#;
    let secret = parse(ed, None).unwrap().secret;
    assert_eq!(
        secret.thumbprint().unwrap(),
        "kPrK_qmxVWaYVA9wwBF6Iuo3vVzz7TxHCTwXBygrS4k"
    );
    let jwk: serde_json::Value =
        serde_json::from_str(&secret.encode(Format::Jwk, "", None, Usage::Sign).unwrap()[0].text)
            .unwrap();
    assert_eq!(jwk["kty"], "OKP");
    assert_eq!(jwk["crv"], "Ed25519");
    assert_eq!(jwk["x"], "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo");
    assert_eq!(jwk["d"], "nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A");
    assert_eq!(
        (jwk["use"].as_str(), jwk["alg"].as_str()),
        (Some("sig"), Some("EdDSA"))
    );
    assert_eq!(jwk["kid"], "kPrK_qmxVWaYVA9wwBF6Iuo3vVzz7TxHCTwXBygrS4k");

    // RFC 8037 A.6: the ephemeral X25519 key, whose public half must match its private one.
    let x = r#"{"kty":"OKP","crv":"X25519","d":"dwdtCnMYpX08FsFyUbJmRd9ML4frwJkqsXf7pR25LCo","x":"hSDwCYkwp1R0i33ctD73Wg2_Og0mOBr066SpjqqbTmo"}"#;
    assert_eq!(parse(x, None).unwrap().secret.label(), "X25519");
    let mismatch = x.replace("hSDw", "hSDx");
    assert!(format!("{:#}", parse(&mismatch, None).err().unwrap()).contains("do not match"));

    // Members and lengths of RFC 7518 §6.
    for (algo, size) in [(Algo::P256, 32), (Algo::P384, 48), (Algo::P521, 66)] {
        let secret = Secret::random(algo).unwrap();
        let set = &secret
            .encode(Format::Jwks, "", None, Usage::Encrypt)
            .unwrap();
        let private: serde_json::Value = serde_json::from_str(&set[0].text).unwrap();
        let key = &private["keys"][0];
        assert_eq!(
            (
                key["kty"].as_str(),
                key["use"].as_str(),
                key["alg"].as_str()
            ),
            (Some("EC"), Some("enc"), Some("ECDH-ES"))
        );
        for member in ["x", "y", "d"] {
            let bytes =
                base64ct::Base64UrlUnpadded::decode_vec(key[member].as_str().unwrap()).unwrap();
            assert_eq!(bytes.len(), size, "{algo:?} {member}");
        }
        let public: serde_json::Value = serde_json::from_str(&set[1].text).unwrap();
        assert!(public["keys"][0].get("d").is_none());
        assert_eq!(public["keys"][0]["kid"], key["kid"]);
    }
    let rsa = Secret::random(Algo::Rsa2048).unwrap();
    let jwk: serde_json::Value =
        serde_json::from_str(&rsa.encode(Format::Jwk, "", None, Usage::Sign).unwrap()[0].text)
            .unwrap();
    for member in ["n", "e", "d", "p", "q", "dp", "dq", "qi"] {
        assert!(jwk[member].is_string(), "{member}");
    }
    assert_eq!(
        (jwk["e"].as_str(), jwk["alg"].as_str()),
        (Some("AQAB"), Some("RS256"))
    );
    let tls: serde_json::Value =
        serde_json::from_str(&rsa.encode(Format::Jwk, "", None, Usage::Tls).unwrap()[1].text)
            .unwrap();
    assert!(tls.get("alg").is_none() && tls.get("d").is_none());
}

#[test]
fn errors_never_quote_the_key() {
    let secret = Secret::random(Algo::P256).unwrap();
    let jwk = secret
        .encode(Format::Jwk, "", None, Usage::Sign)
        .unwrap()
        .remove(0)
        .text;
    let value: serde_json::Value = serde_json::from_str(&jwk).unwrap();
    let d = value["d"].as_str().unwrap().to_owned();
    let pem = secret
        .encode(Format::Pkcs8, "", None, Usage::Sign)
        .unwrap()
        .remove(0)
        .text;
    let body = pem.lines().nth(1).unwrap().to_owned();
    let broken = [
        jwk.replace("\"kty\": \"EC\"", "\"kty\": 7"),
        jwk.replace(&d, &format!("{d}!")),
        jwk.replace("\"crv\": \"P-256\"", "\"crv\": \"P-384\""),
        jwk.replace(",\n  \"d\"", "\n  \"d\""),
        pem.replace(&body, &body[4..]),
        pem.replace("PRIVATE KEY", "DSA PRIVATE KEY"),
        pem.replace("-----END", "Proc-Type: 4,ENCRYPTED\n-----END"),
        format!("{d}\n{body}"),
    ];
    for text in broken {
        let error = format!(
            "{:#} {:?}",
            parse(&text, Some("p")).err().unwrap(),
            parse_public(&text).err()
        );
        assert!(!error.contains(&d) && !error.contains(&body), "{error}");
    }
}

#[test]
fn exports_where_the_user_said_and_nowhere_else() {
    let (ssh, out) = scratch("export");
    // An SSH key in OpenSSH format goes to ~/.ssh, whatever folder was passed.
    let made = ssh
        .create(
            Algo::P521,
            &spec(
                Usage::Ssh,
                Format::Openssh,
                "id_p521",
                Some("pw"),
                Some(&out),
            ),
        )
        .unwrap();
    assert_eq!(made.files.len(), 2);
    assert!(
        made.files
            .iter()
            .all(|f| Path::new(f.path.as_ref().unwrap()).starts_with(ssh.dir()))
    );
    assert!(
        made.files[0].secret && made.files[0].text.is_none(),
        "a written private key is not returned"
    );
    assert!(
        made.files[1]
            .text
            .as_ref()
            .unwrap()
            .starts_with("ecdsa-sha2-nistp521 ")
    );
    assert!(made.fingerprint.as_ref().unwrap().starts_with("SHA256:"));
    assert_eq!(fs::read_dir(&out).unwrap().count(), 0);

    // Anything else goes to the chosen folder, with private files readable by the owner only.
    let made = ssh
        .create(
            Algo::P256,
            &spec(Usage::Sign, Format::Jwk, "jwt", None, Some(&out)),
        )
        .unwrap();
    let names: Vec<_> = made.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["jwt.jwk.json", "jwt.pub.jwk.json"]);
    assert!(out.join("jwt.jwk.json").is_file() && out.join("jwt.pub.jwk.json").is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |f: &str| fs::metadata(out.join(f)).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            (mode("jwt.jwk.json"), mode("jwt.pub.jwk.json")),
            (0o600, 0o644)
        );
    }
    // Never over an existing file, and never half written.
    fs::write(out.join("tls.pub.pem"), "mine").unwrap();
    let error = ssh
        .create(
            Algo::P256,
            &spec(Usage::Tls, Format::Pkcs8, "tls", None, Some(&out)),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("already exists"));
    assert!(!out.join("tls.pem").exists());
    assert_eq!(fs::read_to_string(out.join("tls.pub.pem")).unwrap(), "mine");

    // Without a folder nothing is written: the private key comes back to be shown.
    let before = fs::read_dir(&out).unwrap().count();
    let shown = ssh
        .create(
            Algo::X25519,
            &spec(Usage::Encrypt, Format::Pkcs8, "box", None, None),
        )
        .unwrap();
    assert_eq!(fs::read_dir(&out).unwrap().count(), before);
    assert!(shown.fingerprint.is_none(), "X25519 is no SSH key");
    assert!(
        shown
            .files
            .iter()
            .all(|f| f.path.is_none() && f.text.is_some())
    );
    assert!(
        ssh.create(
            Algo::P256,
            &spec(Usage::Sign, Format::Jwk, "../evil", None, Some(&out))
        )
        .is_err()
    );
    assert!(
        ssh.create(
            Algo::P256,
            &spec(
                Usage::Sign,
                Format::Jwk,
                "rel",
                None,
                Some(Path::new("relative"))
            )
        )
        .is_err()
    );
}

/// Remembers what `made` wrote, as the app does: those files become readable.
fn remember(made: &Generated, allowed: &mut HashSet<PathBuf>) {
    for path in made.files.iter().filter_map(|f| f.path.as_ref()) {
        allowed.insert(Path::new(path).canonicalize().unwrap());
    }
}

#[test]
fn converts_reveals_and_tests_a_key() {
    let (ssh, out) = scratch("convert");
    let mut allowed = HashSet::new();
    let key = ssh
        .generate(
            "id_ed25519",
            crate::KeyType::Ed25519,
            "alex@laptop",
            Some("pw"),
        )
        .unwrap();
    let key = key.to_string_lossy().into_owned();

    // Reveal: an encrypted key needs its passphrase, and comes out decrypted in its own format.
    assert_eq!(
        locked(ssh.reveal(&key, None, &allowed).unwrap_err()),
        Locked::NeedsPassphrase
    );
    assert_eq!(
        locked(ssh.reveal(&key, Some("no"), &allowed).unwrap_err()),
        Locked::WrongPassphrase
    );
    let clear = ssh.reveal(&key, Some("pw"), &allowed).unwrap();
    assert!(clear.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
    let parsed = parse(&clear, None).unwrap();
    assert!(!parsed.encrypted);
    assert_eq!(parsed.comment, "alex@laptop");

    // Convert to PuTTY, then test the result against its .pub.
    let made = ssh
        .convert(
            &key,
            Some("pw"),
            &Spec {
                comment: String::new(),
                ..spec(Usage::Ssh, Format::Ppk, "laptop", None, Some(&out))
            },
            &allowed,
        )
        .unwrap();
    remember(&made, &mut allowed);
    let ppk = made.files[0].path.clone().unwrap();
    assert!(ppk.ends_with("laptop.ppk"));
    assert!(
        made.files[1]
            .text
            .as_ref()
            .unwrap()
            .ends_with(" alex@laptop\n"),
        "the key's comment is kept"
    );
    let unencrypted = ssh.reveal(&ppk, None, &allowed).unwrap();
    assert_eq!(
        *unencrypted,
        fs::read_to_string(&ppk).unwrap(),
        "a key without passphrase is shown as it is"
    );
    let checks = ssh.test(&ppk, None, None, &allowed).unwrap();
    let kinds: Vec<_> = checks.iter().map(|c| (c.kind, c.ok)).collect();
    assert_eq!(
        kinds,
        [
            (CheckKind::Read, true),
            (CheckKind::Pair, true),
            (CheckKind::Signature, true)
        ]
    );
    assert_eq!(checks[1].detail.as_deref(), Some("laptop.pub"));

    let checks = ssh.test(&key, Some("pw"), None, &allowed).unwrap();
    let kinds: Vec<_> = checks.iter().map(|c| (c.kind, c.ok)).collect();
    assert_eq!(
        kinds,
        [
            (CheckKind::Read, true),
            (CheckKind::Passphrase, true),
            (CheckKind::Pair, true),
            (CheckKind::Signature, true)
        ]
    );
    assert_eq!(checks[0].detail.as_deref(), Some("Ed25519"));
    assert_eq!(checks[2].detail.as_deref(), Some("id_ed25519.pub"));
    assert_eq!(
        ssh.test(&key, Some("no"), None, &allowed).unwrap(),
        [check(
            CheckKind::Passphrase,
            false,
            None,
            Some("wrong passphrase".into())
        )]
    );

    // A .pub of another key is reported, not trusted.
    let other = ssh
        .create(
            Algo::Ed25519,
            &spec(Usage::Sign, Format::Pkcs8, "other", None, Some(&out)),
        )
        .unwrap();
    remember(&other, &mut allowed);
    let public = other.files[1].path.clone().unwrap();
    let checks = ssh.test(&key, Some("pw"), Some(&public), &allowed).unwrap();
    assert_eq!(
        checks[2],
        check(CheckKind::Pair, false, Some("other.pub.pem".into()), None)
    );
    let pem = other.files[0].path.clone().unwrap();
    let checks = ssh.test(&pem, None, None, &allowed).unwrap();
    assert!(checks.iter().all(|c| c.ok), "{checks:?}");
    assert_eq!(
        checks[1].detail.as_deref(),
        Some("other.pub.pem"),
        "found next to the key"
    );

    let x = ssh
        .create(
            Algo::X25519,
            &spec(Usage::Encrypt, Format::Jwk, "box", None, Some(&out)),
        )
        .unwrap();
    remember(&x, &mut allowed);
    let checks = ssh
        .test(x.files[0].path.as_ref().unwrap(), None, None, &allowed)
        .unwrap();
    assert_eq!(checks.last().unwrap().kind, CheckKind::Agreement);
    assert!(checks.iter().all(|c| c.ok), "{checks:?}");

    // Something that is not a key: a clear failure, without its content.
    let notes = out.join("notes.txt");
    fs::write(&notes, "dear diary").unwrap();
    allowed.insert(notes.canonicalize().unwrap());
    let checks = ssh
        .test(&notes.to_string_lossy(), None, None, &allowed)
        .unwrap();
    assert_eq!(
        (checks.len(), checks[0].kind, checks[0].ok),
        (1, CheckKind::Read, false)
    );
    assert!(!checks[0].error.as_ref().unwrap().contains("diary"));
    assert!(ssh.reveal("relative/key", None, &allowed).is_err());

    // A key TermF cannot read is still shown when it is in clear, never when it is encrypted.
    let dsa = ssh.dir().join("id_dsa");
    fs::write(
        &dsa,
        "-----BEGIN DSA PRIVATE KEY-----\nMIIBuw==\n-----END DSA PRIVATE KEY-----\n",
    )
    .unwrap();
    let dsa = dsa.to_string_lossy().into_owned();
    assert!(
        ssh.reveal(&dsa, None, &allowed)
            .unwrap()
            .contains("DSA PRIVATE KEY")
    );
    let encrypted = fs::read_to_string(&key).unwrap();
    assert!(!formats::plain_private(&encrypted));
    assert!(formats::plain_private(&clear));
}

#[test]
fn reads_only_keys_in_ssh_written_or_picked() {
    let (ssh, out) = scratch("allowed");
    let mut allowed = HashSet::new();
    let made = ssh
        .create(
            Algo::P256,
            &spec(Usage::Tls, Format::Pkcs8, "tls", None, Some(&out)),
        )
        .unwrap();
    let private = made.files[0].path.clone().unwrap();
    let refused = |e: anyhow::Error| format!("{e:#}").contains("was not picked");
    // Outside ~/.ssh and neither written nor picked this session: refused by every command.
    assert!(refused(ssh.reveal(&private, None, &allowed).unwrap_err()));
    assert!(refused(
        ssh.test(&private, None, None, &allowed).unwrap_err()
    ));
    let jwk = spec(Usage::Sign, Format::Jwk, "copy", None, None);
    assert!(refused(
        ssh.convert(&private, None, &jwk, &allowed).unwrap_err()
    ));
    remember(&made, &mut allowed);
    assert!(ssh.reveal(&private, None, &allowed).is_ok());
    assert!(ssh.convert(&private, None, &jwk, &allowed).is_ok());

    // Inside ~/.ssh nothing needs picking; the public file of a test goes through the same gate.
    let key = ssh
        .generate("id", crate::KeyType::Ed25519, "", None)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let none = HashSet::new();
    assert!(ssh.reveal(&key, None, &none).is_ok());
    let elsewhere = made.files[1].path.clone().unwrap();
    assert!(refused(
        ssh.test(&key, None, Some(&elsewhere), &none).unwrap_err()
    ));

    // A symlink in ~/.ssh leading out counts as the file it leads to.
    #[cfg(unix)]
    {
        let link = ssh.dir().join("outside");
        std::os::unix::fs::symlink(&private, &link).unwrap();
        assert!(refused(
            ssh.reveal(&link.to_string_lossy(), None, &none)
                .unwrap_err()
        ));
    }
}

#[test]
fn a_failed_write_leaves_no_file() {
    let (ssh, out) = scratch("half");
    let secret = Secret::random(Algo::Ed25519).unwrap();
    let outputs = secret.encode(Format::Pkcs8, "", None, Usage::Sign).unwrap();
    let calls = std::cell::Cell::new(0);
    let error = write_all(&out, "k", &outputs, |file, bytes| {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            file.write_all(&bytes[..10])?;
            return Err(std::io::Error::other("disk full"));
        }
        fill(file, bytes)
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("disk full"));
    assert_eq!(
        fs::read_dir(&out).unwrap().count(),
        0,
        "neither the written file nor the half-written one is left"
    );
    // Nothing that can fail runs after the files are written.
    let made = ssh
        .create(
            Algo::Ed25519,
            &spec(Usage::Sign, Format::Jwk, "k", None, Some(&out)),
        )
        .unwrap();
    assert_eq!(made.files.iter().filter(|f| f.path.is_some()).count(), 2);
}

#[test]
fn finds_the_public_file_next_to_a_key() {
    let (_, out) = scratch("sibling");
    for name in [
        "id",
        "id.pub",
        "a.pem",
        "a.pub.pem",
        "k.jwk.json",
        "k.pub.jwk.json",
        "p.ppk",
        "p.pub",
        "api.prod",
        "api.prod.pub",
        "api.prod.pem",
        "api.prod.pub.pem",
        "api.prod.jwks.json",
        "api.prod.pub.jwks.json",
    ] {
        fs::write(out.join(name), "").unwrap();
    }
    let sibling = |name: &str| sibling_public(&out.join(name)).map(|p| file_name(&p));
    assert_eq!(sibling("id").as_deref(), Some("id.pub"));
    assert_eq!(sibling("a.pem").as_deref(), Some("a.pub.pem"));
    assert_eq!(sibling("k.jwk.json").as_deref(), Some("k.pub.jwk.json"));
    assert_eq!(sibling("p.ppk").as_deref(), Some("p.pub"));
    assert_eq!(sibling("api.prod").as_deref(), Some("api.prod.pub"));
    assert_eq!(sibling("api.prod.pem").as_deref(), Some("api.prod.pub.pem"));
    assert_eq!(
        sibling("api.prod.jwks.json").as_deref(),
        Some("api.prod.pub.jwks.json")
    );
    assert_eq!(sibling("id.pub"), None);
}

#[test]
fn a_short_jwk_d_is_padded() {
    let mut d = Zeroizing::new(vec![0u8; 32]);
    for (i, b) in d.iter_mut().enumerate() {
        *b = i as u8;
    }
    let secret = Secret::Ec(formats::Curve::P256, d);
    let jwk = secret
        .encode(Format::Jwk, "", None, Usage::Sign)
        .unwrap()
        .remove(0)
        .text;
    let full = base64ct::Base64UrlUnpadded::encode_string(&(0..32u8).collect::<Vec<_>>());
    let short = base64ct::Base64UrlUnpadded::encode_string(&(1..32u8).collect::<Vec<_>>());
    let parsed = parse(&jwk.replace(&full, &short), None).unwrap().secret;
    assert_eq!(parsed.spki().unwrap(), secret.spki().unwrap());
    let again = parsed
        .encode(Format::Jwk, "", None, Usage::Sign)
        .unwrap()
        .remove(0)
        .text;
    assert!(again.contains(&full), "written back at full length");
}

#[test]
fn clipboard_digest_ignores_line_endings() {
    let digest = crate::tools::clipboard_digest("-----BEGIN-----\nabc\n-----END-----\n");
    assert_eq!(
        crate::tools::clipboard_digest("-----BEGIN-----\r\nabc\r\n-----END-----"),
        digest
    );
    assert_ne!(
        crate::tools::clipboard_digest("-----BEGIN-----\nabd\n-----END-----"),
        digest
    );
}
