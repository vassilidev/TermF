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

// Throwaway keys made by `ssh-keygen -t ecdsa -m PKCS8` of macOS (OpenSSH 10.2 on LibreSSL 3.3.6): the curve
// spelled out instead of named, and, protected by "passphrase", PBKDF2 with HMAC-SHA1 and AES-128-CBC.
const LIBRESSL_EXPLICIT: &str = "-----BEGIN PRIVATE KEY-----
MIIBeQIBADCCAQMGByqGSM49AgEwgfcCAQEwLAYHKoZIzj0BAQIhAP////8AAAAB
AAAAAAAAAAAAAAAA////////////////MFsEIP////8AAAABAAAAAAAAAAAAAAAA
///////////////8BCBaxjXYqjqT57PrvVV2mIa8ZR0GsMxTsPY7zjw+J9JgSwMV
AMSdNgiG5wSTamZ44ROdJreBn36QBEEEaxfR8uEsQkf4vOblY6RA8ncDfYEt6zOg
9KE5RdiYwpZP40Li/hp/m47n60p8D54WK84zV2sxXs7LtkBoN79R9QIhAP////8A
AAAA//////////+85vqtpxeehPO5ysL8YyVRAgEBBG0wawIBAQQgpEFeso6apmqw
fDDIlu+P9pLqlF5NuuJeXE+jttPwDuyhRANCAARqemUqEL3Sk8Cry6JmC5mO28UK
6AIuvDdqKSxWYaKjMKZlO5fZpytGayb5h7xh6IiFKTdS08M9xYuWl1eRjMEv
-----END PRIVATE KEY-----
";
const LIBRESSL_EXPLICIT_PUBLIC: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBGp6ZSoQvdKTwKvLomYLmY7bxQroAi68N2opLFZhoqMwpmU7l9mnK0ZrJvmHvGHoiIUpN1LTwz3Fi5aXV5GMwS8= made";
const LIBRESSL_ENCRYPTED: &str = "-----BEGIN ENCRYPTED PRIVATE KEY-----
MIIBzzBJBgkqhkiG9w0BBQ0wPDAbBgkqhkiG9w0BBQwwDgQI3RLqJwb/O0gCAggA
MB0GCWCGSAFlAwQBAgQQcKxYEF3J9WzjauK+896uzgSCAYBKtrl2zfGs+ww7z4Dk
uyS8HnuHiPAths2MVo4yeollHB/uqbEmsLwMl5ObZCo8iHEcdu9w9+Dq/IXAA4D4
Dows+8uJYJuiU5JVC6m41Qa3ZbUI65HeDzFmeftryzQx0L2POwJI/KOSxKMqTaeg
zzk6pKFmZMJ8kthgiDIFgvvAXwzmYdovDJ0rxvAsqGJostODN2QOfkBsdyqKNdXN
GXHM7m8pduxiQbj4Rrwn20XTiK9ZM4U5q1uwYmC9+Nn8tNz6fWiNm0UcPky5GwT+
4xk1M/KBEoPOAq6SivILgGXmvxgs43NmAlbTzQuPbctc5R5nw1gRLe/hHZWZGdp9
o+vPw2J+a03VQy4d4AB99L9ufPCveGrjwI6K8/aK7MIfu8B02MgSrGu3aAiprAxH
9aSmCIz4KfA/gpGez4FnHKL53HErc6MURxjnwnTslUdZp+/MgYKS93ykhQhQ+BDp
VCFKljbJLPEAnypiFzJQOSt3Sbg2LONTygFKQOn8o0pb8Zk=
-----END ENCRYPTED PRIVATE KEY-----
";
const LIBRESSL_ENCRYPTED_PUBLIC: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBP6iLr7/MDsAHRubyu0AsMXBjQObok60Jy02+My3zGc3bF0v4eZEj9WkaZ9dOdaRLx4bvaaNaF1qTkomXwhvzYA= made";

// A throwaway P-521 key whose scalar `ssh-keygen` wrote without its leading zero, then the same protected by
// "passphrase".
pub(crate) const OPENSSH_SHORT_SCALAR: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAArAAAABNlY2RzYS
1zaGEyLW5pc3RwNTIxAAAACG5pc3RwNTIxAAAAhQQAww5qvpgl+uM9BRuo72P/Q5qCrgFe
dAqD83ViHLBGnrN7SP8W0ylOSkZJT5Tm30DMJS9d7S8gza53WMAgWssQIyoBF6/+EMqSDZ
ACwpXwPtt89TKrG6ZxtIvUeTTbgBNneiC+Rkf0s3H36CWL1UMQ15IILDkgt1z19a6pdLUm
02cWyxEAAAEIl3eBEJd3gRAAAAATZWNkc2Etc2hhMi1uaXN0cDUyMQAAAAhuaXN0cDUyMQ
AAAIUEAMMOar6YJfrjPQUbqO9j/0Oagq4BXnQKg/N1YhywRp6ze0j/FtMpTkpGSU+U5t9A
zCUvXe0vIM2ud1jAIFrLECMqARev/hDKkg2QAsKV8D7bfPUyqxumcbSL1Hk024ATZ3ogvk
ZH9LNx9+gli9VDENeSCCw5ILdc9fWuqXS1JtNnFssRAAAAQUj05W7Ztz3arTO7PS8a40On
zsIGMCXOasqfB0cYhWmqUq706rI0x22h8J0akZzV/0/G4Re7RxoSH3RkCMcFjz11AAAABG
1hZGUBAgMEBQYH
-----END OPENSSH PRIVATE KEY-----
";
pub(crate) const OPENSSH_SHORT_SCALAR_ENCRYPTED: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABBLoTAXIx
8IFoDtX8rwBGzvAAAAGAAAAAEAAACsAAAAE2VjZHNhLXNoYTItbmlzdHA1MjEAAAAIbmlz
dHA1MjEAAACFBADDDmq+mCX64z0FG6jvY/9DmoKuAV50CoPzdWIcsEaes3tI/xbTKU5KRk
lPlObfQMwlL13tLyDNrndYwCBayxAjKgEXr/4QypINkALClfA+23z1MqsbpnG0i9R5NNuA
E2d6IL5GR/SzcffoJYvVQxDXkggsOSC3XPX1rql0tSbTZxbLEQAAARAxlrn2OA/ekXG9fA
S8DL5igshvLFRKwYGC8ZmBFFbeKgVCAHFEgPR0wPfe3paQThyX+nILKfuqw8AQE82Grap6
0P1BJHXN9ZussPYlwCq0VYw13QD9ix+rGCSc4l5XwIZu+TdVdpFSlU+ha5hCLyuhv45iVl
wx7ZYozXqGr/BImO9LfUgBLHuGv2yoUpHAn08QSrnqQx1bt0V3ztiZFOFRmXsV6RwhPTez
qXFmRd5DneyG6WHdw9BBPaCkLWdRg+10dwdoFOyvlUXEW2qK/W0vo69pPj2QcxYnJTwDJi
EOmu1ZqWhQjiPgwtWzm4Wliiqw8nywnjfC2JYcvDjcoVjjbgnP+FODF0kLktko6drBVw==
-----END OPENSSH PRIVATE KEY-----
";
pub(crate) const OPENSSH_SHORT_SCALAR_PUBLIC: &str = "ecdsa-sha2-nistp521 AAAAE2VjZHNhLXNoYTItbmlzdHA1MjEAAAAIbmlzdHA1MjEAAACFBADDDmq+mCX64z0FG6jvY/9DmoKuAV50CoPzdWIcsEaes3tI/xbTKU5KRklPlObfQMwlL13tLyDNrndYwCBayxAjKgEXr/4QypINkALClfA+23z1MqsbpnG0i9R5NNuAE2d6IL5GR/SzcffoJYvVQxDXkggsOSC3XPX1rql0tSbTZxbLEQ== made";

// The same, protected by the two ciphers that carry an authentication tag.
const OPENSSH_SHORT_SCALAR_GCM: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAAFmFlczI1Ni1nY21Ab3BlbnNzaC5jb20AAAAGYmNyeXB0AA
AAGAAAABBsJK9SNfgiONr/oRNAZScIAAAAGAAAAAEAAACsAAAAE2VjZHNhLXNoYTItbmlz
dHA1MjEAAAAIbmlzdHA1MjEAAACFBADDDmq+mCX64z0FG6jvY/9DmoKuAV50CoPzdWIcsE
aes3tI/xbTKU5KRklPlObfQMwlL13tLyDNrndYwCBayxAjKgEXr/4QypINkALClfA+23z1
MqsbpnG0i9R5NNuAE2d6IL5GR/SzcffoJYvVQxDXkggsOSC3XPX1rql0tSbTZxbLEQAAAR
BwIIOMeZ6y+K3nErOvl/DKZMa0IqOr1Ud6nXxTa0FyS8Fhphz8yz+C9dpZoHUyBRbxi8uE
6aoI0CPF/gahtJkI+P+jqtP+QmyvKspfdLGQQn103YQAIIZmuEGC1BLdUggCPq2mWy5dfz
yUKin7OsC4y6Mp5ofJYpJfHiTywxqhT9VvPQrNVm9wJY4/u8L6i1up8rgpAEMjCZ02qX6Q
QyLcMCh2xvW0r95AyZ7s6C122sTcMKBgA2uEWxlqku2Xyzx5n4cn4Acw0JjBL8s6X6ucfr
UatbbAIbm+XRMTIz4KYLvVEZ1+LfzialMLUiRrVUFGaRZkYso7hcRvtaOV0cejYiUeVy05
xNy3Jo13mFi/wKNM6zyA8VyXJHB/TsdFvXQ=
-----END OPENSSH PRIVATE KEY-----
";
const OPENSSH_SHORT_SCALAR_CHACHA: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAAHWNoYWNoYTIwLXBvbHkxMzA1QG9wZW5zc2guY29tAAAABm
JjcnlwdAAAABgAAAAQFVDrfeuy6uu5X5WJoazbxQAAABgAAAABAAAArAAAABNlY2RzYS1z
aGEyLW5pc3RwNTIxAAAACG5pc3RwNTIxAAAAhQQAww5qvpgl+uM9BRuo72P/Q5qCrgFedA
qD83ViHLBGnrN7SP8W0ylOSkZJT5Tm30DMJS9d7S8gza53WMAgWssQIyoBF6/+EMqSDZAC
wpXwPtt89TKrG6ZxtIvUeTTbgBNneiC+Rkf0s3H36CWL1UMQ15IILDkgt1z19a6pdLUm02
cWyxEAAAEIpWcB1RMJxo+8c9nOuBHnNkp0hNklnPd2YUpVVVjfUCf3t0eA6ju7vLlAzhHY
VmPowBJQyOh/424A8DmXjrj3wGfwf2TeaTCUB13B2TJ1o9RVHJyN0JX8VltiyK7ExNKPas
Sn6T0srFXQCaeDiMQP7iQ12dvLfYottq5aTCK4nvGt+gactucKYvjGIJOSR9gwR8sstUQt
WWQA+79Z535YI6N2kv9mP0uBM1ROk59i4CrQO9j48TNTsGvsHQzYOl7fNKrJfdK5e3kMr+
gS/nKRAAowLYsXeqytgdMuM3d/YxGnZlEsqhwwBimJbMhJZrDIRrIgNEJSohwu03GfIqT+
sdSVLMm8k0+uD/B/cUxhqkx+HPJFUnKCIg==
-----END OPENSSH PRIVATE KEY-----
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

/// OpenSSH's own `ssh-keygen`, given nothing of the user's: `-f` names every file, all of them in `dir`.
#[cfg(unix)]
fn ssh_keygen(dir: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("ssh-keygen")
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(unix)]
fn key_part(line: &str) -> String {
    line.split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(unix)]
#[test]
fn ssh_keygen_reads_every_key_termf_writes_and_termf_reads_its() {
    use std::os::unix::fs::PermissionsExt;
    if std::process::Command::new("ssh-keygen")
        .arg("-?")
        .output()
        .is_err()
    {
        eprintln!("no ssh-keygen here: skipped");
        return;
    }
    let (_, dir) = scratch("ssh-keygen");
    let write = |name: &str, text: &str| {
        let path = dir.join(name);
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        path.to_string_lossy().into_owned()
    };
    let pass = "pass phrase é";
    for algo in ALGOS.into_iter().filter(|a| *a != Algo::X25519) {
        let secret = Secret::random(algo).unwrap();
        let ssh_line = secret
            .encode(Format::Openssh, "alex@acme", None, Usage::Ssh)
            .unwrap()[1]
            .text
            .to_string();
        for (format, passphrase) in [
            (Format::Openssh, None),
            (Format::Openssh, Some(pass)),
            (Format::Pkcs8, None),
            (Format::Pkcs8, Some(pass)),
            (Format::Pkcs1, None),
            (Format::Sec1, None),
        ] {
            if !supported(algo, format) {
                continue;
            }
            let outputs = secret
                .encode(format, "alex@acme", passphrase, Usage::Ssh)
                .unwrap();
            let what = format!(
                "{algo:?} as {format:?}, passphrase {}",
                passphrase.is_some()
            );
            let private = write(&format!("k{}", outputs[0].suffix), &outputs[0].text);
            let derived = ssh_keygen(
                &dir,
                &["-y", "-P", passphrase.unwrap_or(""), "-f", &private],
            );
            // An OpenSSH built on LibreSSL reads no Ed25519 PEM: only its own format is required then.
            if derived.is_none() && algo == Algo::Ed25519 && format != Format::Openssh {
                eprintln!("ssh-keygen cannot read {what}: skipped");
                continue;
            }
            let derived = derived.unwrap_or_else(|| panic!("ssh-keygen cannot read {what}"));
            assert_eq!(key_part(&derived), key_part(&ssh_line), "{what}");
            if passphrase.is_some() {
                assert!(
                    ssh_keygen(&dir, &["-y", "-P", "wrong", "-f", &private]).is_none(),
                    "{what}: a wrong passphrase opens it"
                );
            }
            for public in outputs.iter().skip(1) {
                let path = write(&format!("k{}", public.suffix), &public.text);
                let line = if public.suffix == ".pub" {
                    public.text.to_string()
                } else {
                    ssh_keygen(&dir, &["-i", "-m", "PKCS8", "-f", &path]).unwrap_or_else(|| {
                        panic!("ssh-keygen cannot read the public half of {what}")
                    })
                };
                assert_eq!(
                    key_part(&line),
                    key_part(&ssh_line),
                    "{what}{}",
                    public.suffix
                );
            }
        }
    }

    // The other way: keys ssh-keygen makes, protected or not, in each of its formats.
    for (kind, bits) in [
        ("ed25519", ""),
        ("ecdsa", "256"),
        ("ecdsa", "384"),
        ("ecdsa", "521"),
        ("rsa", "2048"),
    ] {
        for (format, passphrase) in [
            ("RFC4716", ""),
            ("RFC4716", pass),
            ("PKCS8", ""),
            ("PKCS8", pass),
            ("PEM", ""),
        ] {
            if kind == "ed25519" && format != "RFC4716" {
                continue;
            }
            let name = format!("made-{kind}{bits}-{format}-{}", passphrase.len());
            let file = dir.join(&name).to_string_lossy().into_owned();
            let mut args = vec![
                "-q", "-t", kind, "-m", format, "-N", passphrase, "-C", "made", "-f", &file,
            ];
            if !bits.is_empty() {
                args.extend(["-b", bits]);
            }
            ssh_keygen(&dir, &args).unwrap_or_else(|| panic!("ssh-keygen {args:?}"));
            let public = fs::read_to_string(format!("{file}.pub")).unwrap();
            let text = fs::read_to_string(&file).unwrap();
            let pass = (!passphrase.is_empty()).then_some(passphrase);
            let parsed =
                parse(&text, pass).unwrap_or_else(|e| panic!("TermF cannot read {name}: {e:#}"));
            assert_eq!(
                parsed.secret.spki().unwrap(),
                parse_public(&public).unwrap(),
                "{name}"
            );
            if pass.is_some() {
                assert!(parse(&text, Some("wrong")).is_err(), "{name}");
            }
        }
    }
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn reads_ecdsa_keys_whose_curve_is_spelled_out() {
    for (text, pass, public) in [
        (LIBRESSL_EXPLICIT, None, LIBRESSL_EXPLICIT_PUBLIC),
        (
            LIBRESSL_ENCRYPTED,
            Some("passphrase"),
            LIBRESSL_ENCRYPTED_PUBLIC,
        ),
    ] {
        let read = parse(text, pass).unwrap();
        assert_eq!(read.secret.spki().unwrap(), parse_public(public).unwrap());
    }
    assert_eq!(
        locked(parse(LIBRESSL_ENCRYPTED, Some("wrong")).err().unwrap()),
        Locked::WrongPassphrase
    );
    // The same key with a public point that is not its own: refused, never read as another curve's.
    let der = pkcs8::SecretDocument::from_pem(LIBRESSL_EXPLICIT)
        .unwrap()
        .1;
    let mut bytes = der.as_bytes().to_vec();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    assert!(formats::from_pkcs8(&bytes).is_err());
}

#[test]
fn reads_openssh_ecdsa_keys_whose_scalar_lost_its_leading_zero() {
    for (text, pass) in [
        (OPENSSH_SHORT_SCALAR, None),
        (OPENSSH_SHORT_SCALAR_ENCRYPTED, Some("passphrase")),
        (OPENSSH_SHORT_SCALAR_GCM, Some("passphrase")),
        (OPENSSH_SHORT_SCALAR_CHACHA, Some("passphrase")),
    ] {
        let read = parse(text, pass).unwrap();
        assert_eq!(
            read.secret.spki().unwrap(),
            parse_public(OPENSSH_SHORT_SCALAR_PUBLIC).unwrap()
        );
        assert_eq!(
            (read.comment.as_str(), read.encrypted),
            ("made", pass.is_some())
        );
    }
    for text in [
        OPENSSH_SHORT_SCALAR_ENCRYPTED,
        OPENSSH_SHORT_SCALAR_GCM,
        OPENSSH_SHORT_SCALAR_CHACHA,
    ] {
        assert_eq!(
            locked(parse(text, Some("wrong")).err().unwrap()),
            Locked::WrongPassphrase
        );
    }
    assert_eq!(
        locked(parse(OPENSSH_SHORT_SCALAR_ENCRYPTED, None).err().unwrap()),
        Locked::NeedsPassphrase
    );
}

/// `inner` (an ECPrivateKey) under the EC algorithm with `params`, as PKCS#8 DER.
fn ec_pkcs8(params: pkcs8::der::asn1::AnyRef<'_>, inner: &[u8]) -> Vec<u8> {
    use pkcs8::der::Encode;
    pkcs8::PrivateKeyInfo {
        algorithm: pkcs8::AlgorithmIdentifierRef {
            oid: pkcs8::ObjectIdentifier::new_unwrap("1.2.840.10045.2.1"),
            parameters: Some(params),
        },
        private_key: inner,
        public_key: None,
    }
    .to_der()
    .unwrap()
}

/// SEQUENCE { 1, d, [1] point } in DER, the point left out when `None`.
fn ec_inner(d: &[u8], point: Option<&[u8]>) -> Vec<u8> {
    let mut body = vec![0x02, 0x01, 0x01, 0x04, d.len() as u8];
    body.extend_from_slice(d);
    if let Some(point) = point {
        body.extend([
            0xa1,
            point.len() as u8 + 3,
            0x03,
            point.len() as u8 + 1,
            0x00,
        ]);
        body.extend_from_slice(point);
    }
    let mut out = vec![0x30, body.len() as u8];
    out.extend(body);
    out
}

#[test]
fn an_explicit_curve_is_read_from_its_parameters_with_any_public_point() {
    use pkcs8::der::Decode;
    let der = pkcs8::SecretDocument::from_pem(LIBRESSL_EXPLICIT)
        .unwrap()
        .1;
    let info = pkcs8::PrivateKeyInfo::from_der(der.as_bytes()).unwrap();
    let params = info.algorithm.parameters.unwrap();
    let Secret::Ec(_, d) = formats::from_pkcs8(der.as_bytes()).unwrap() else {
        panic!("not an EC key");
    };
    let public = parse_public(LIBRESSL_EXPLICIT_PUBLIC).unwrap();
    let spki = pkcs8::spki::SubjectPublicKeyInfoRef::try_from(public.as_slice()).unwrap();
    let point = spki.subject_public_key.raw_bytes();
    let mut compressed = vec![2 + (point[64] & 1)];
    compressed.extend_from_slice(&point[1..33]);
    for inner in [
        ec_inner(&d, None),
        ec_inner(&d, Some(&compressed)),
        ec_inner(&d, Some(point)),
    ] {
        let read = formats::from_pkcs8(&ec_pkcs8(params, &inner)).unwrap();
        assert_eq!(read.spki().unwrap(), public);
    }
    let mut other = point.to_vec();
    other[64] ^= 1;
    assert!(formats::from_pkcs8(&ec_pkcs8(params, &ec_inner(&d, Some(&other)))).is_err());
}

#[test]
fn a_key_that_opens_but_is_not_supported_is_never_a_wrong_passphrase() {
    use pkcs8::der::asn1::AnyRef;
    let secp256k1 = pkcs8::ObjectIdentifier::new_unwrap("1.3.132.0.10");
    let der = ec_pkcs8(AnyRef::from(&secp256k1), &ec_inner(&[7; 32], None));
    let params =
        pkcs8::pkcs5::pbes2::Parameters::pbkdf2_sha256_aes256cbc(2048, &[1; 16], &[2; 16]).unwrap();
    let pem = pkcs8::PrivateKeyInfo::try_from(der.as_slice())
        .unwrap()
        .encrypt_with_params(params, "passphrase")
        .unwrap()
        .to_pem("ENCRYPTED PRIVATE KEY", pkcs8::LineEnding::LF)
        .unwrap();
    let error = parse(&pem, Some("passphrase")).err().unwrap();
    assert!(error.downcast_ref::<Locked>().is_none(), "{error:#}");
    assert!(
        format!("{error:#}").contains("unsupported PKCS#8 key"),
        "{error:#}"
    );
    assert_eq!(
        locked(parse(&pem, Some("wrong")).err().unwrap()),
        Locked::WrongPassphrase
    );
}

#[test]
fn a_key_that_is_not_ecdsa_is_left_to_ssh_key_before_any_derivation() {
    let secret = Secret::random(Algo::Ed25519).unwrap();
    let outputs = secret
        .encode(Format::Openssh, "", Some("passphrase"), Usage::Ssh)
        .unwrap();
    // Without a passphrase: no derivation was even tried, so no `NeedsPassphrase`.
    assert!(
        formats::openssh_ecdsa(&outputs[0].text, None)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        locked(parse(&outputs[0].text, Some("wrong")).err().unwrap()),
        Locked::WrongPassphrase
    );
}
