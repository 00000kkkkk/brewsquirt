use super::{arg, method};
use crate::object::Obj;
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use pkcs8::der::{Decode, Encode};
use pkcs8::{DecodePrivateKey, EncodePrivateKey};
use rand_core::OsRng;
use rsa::pkcs1::{
    DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPrivateKey, EncodeRsaPublicKey,
};
use rsa::traits::{PrivateKeyParts, PublicKeyParts};
use rsa::{BigUint, RsaPrivateKey, RsaPublicKey};
use rustc_hash::FxHashMap;
use spki::{DecodePublicKey, EncodePublicKey};
use std::cell::RefCell;
use std::rc::Rc;

type R = Result<Value, Value>;

const OID_RSA: &str = "1.2.840.113549.1.1.1";
const OID_RSA_PSS: &str = "1.2.840.113549.1.1.10";
const OID_EC: &str = "1.2.840.10045.2.1";
const OID_P256: &str = "1.2.840.10045.3.1.7";
const OID_P384: &str = "1.3.132.0.34";
const OID_ED25519: &str = "1.3.101.112";
const OID_X25519: &str = "1.3.101.110";

fn crypto_error(vm: &mut Vm, code: &str, msg: &str) -> Value {
    let e = vm.make_error(ErrorKind::Error, msg);
    if let Value::Object(o) = &e {
        o.set_own("code", code.into());
    }
    e
}

fn bytes_arg(vm: &mut Vm, v: &Value) -> Result<Vec<u8>, Value> {
    if let Some(b) = super::typed::buffer_source_bytes(v) {
        return Ok(b);
    }
    match v {
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        Value::Undefined | Value::Null => Ok(Vec::new()),
        _ => Err(vm.type_error("Expected a Buffer, TypedArray, DataView or string")),
    }
}

fn str_arg(vm: &mut Vm, args: &[Value], i: usize) -> Result<String, Value> {
    match arg(args, i) {
        Value::Undefined | Value::Null => Ok(String::new()),
        v => Ok(vm.to_string(&v)?.to_string()),
    }
}

fn out_bytes(vm: &Vm, bytes: Vec<u8>) -> Value {
    let len = bytes.len();
    let buf = super::typed::new_buffer(vm, bytes);
    super::typed::new_typed(vm, crate::object::TypedKind::Uint8, buf, 0, len)
}

enum Private {
    Rsa(RsaPrivateKey, bool),
    P256(p256::SecretKey),
    P384(p384::SecretKey),
    Ed25519(ed25519_dalek::SigningKey),
    X25519([u8; 32]),
}

enum Public {
    Rsa(RsaPublicKey, bool),
    P256(p256::PublicKey),
    P384(p384::PublicKey),
    Ed25519(ed25519_dalek::VerifyingKey),
    X25519([u8; 32]),
}

fn curve_of(params: Option<pkcs8::der::asn1::AnyRef<'_>>) -> Option<&'static str> {
    let oid = params?.decode_as::<pkcs8::ObjectIdentifier>().ok()?;
    match oid.to_string().as_str() {
        OID_P256 => Some("P-256"),
        OID_P384 => Some("P-384"),
        _ => None,
    }
}

fn parse_private(der: &[u8]) -> Result<Private, String> {
    let info = pkcs8::PrivateKeyInfo::from_der(der).map_err(|e| e.to_string())?;
    let oid = info.algorithm.oid.to_string();
    match oid.as_str() {
        OID_RSA => RsaPrivateKey::from_pkcs8_der(der)
            .map(|k| Private::Rsa(k, false))
            .map_err(|e| e.to_string()),
        OID_RSA_PSS => RsaPrivateKey::from_pkcs1_der(info.private_key)
            .map(|k| Private::Rsa(k, true))
            .map_err(|e| e.to_string()),
        OID_EC => match curve_of(info.algorithm.parameters) {
            Some("P-256") => p256::SecretKey::from_pkcs8_der(der)
                .map(Private::P256)
                .map_err(|e| e.to_string()),
            Some("P-384") => p384::SecretKey::from_pkcs8_der(der)
                .map(Private::P384)
                .map_err(|e| e.to_string()),
            _ => Err("Unsupported elliptic curve".into()),
        },
        OID_ED25519 => ed25519_dalek::SigningKey::from_pkcs8_der(der)
            .map(Private::Ed25519)
            .map_err(|e| e.to_string()),
        OID_X25519 => {
            let inner = pkcs8::der::asn1::OctetStringRef::from_der(info.private_key)
                .map_err(|e| e.to_string())?;
            let raw: [u8; 32] = inner
                .as_bytes()
                .try_into()
                .map_err(|_| "Invalid X25519 key".to_string())?;
            Ok(Private::X25519(raw))
        }
        _ => Err(format!("Unsupported key algorithm {oid}")),
    }
}

fn parse_public(der: &[u8]) -> Result<Public, String> {
    let info = spki::SubjectPublicKeyInfoRef::from_der(der).map_err(|e| e.to_string())?;
    let oid = info.algorithm.oid.to_string();
    let bits = info.subject_public_key.raw_bytes();
    match oid.as_str() {
        OID_RSA => RsaPublicKey::from_public_key_der(der)
            .map(|k| Public::Rsa(k, false))
            .map_err(|e| e.to_string()),
        OID_RSA_PSS => RsaPublicKey::from_pkcs1_der(bits)
            .map(|k| Public::Rsa(k, true))
            .map_err(|e| e.to_string()),
        OID_EC => match curve_of(info.algorithm.parameters) {
            Some("P-256") => p256::PublicKey::from_sec1_bytes(bits)
                .map(Public::P256)
                .map_err(|e| e.to_string()),
            Some("P-384") => p384::PublicKey::from_sec1_bytes(bits)
                .map(Public::P384)
                .map_err(|e| e.to_string()),
            _ => Err("Unsupported elliptic curve".into()),
        },
        OID_ED25519 => {
            let raw: [u8; 32] = bits
                .try_into()
                .map_err(|_| "Invalid Ed25519 key".to_string())?;
            ed25519_dalek::VerifyingKey::from_bytes(&raw)
                .map(Public::Ed25519)
                .map_err(|e| e.to_string())
        }
        OID_X25519 => {
            let raw: [u8; 32] = bits
                .try_into()
                .map_err(|_| "Invalid X25519 key".to_string())?;
            Ok(Public::X25519(raw))
        }
        _ => Err(format!("Unsupported key algorithm {oid}")),
    }
}

fn oid(s: &str) -> pkcs8::ObjectIdentifier {
    pkcs8::ObjectIdentifier::new_unwrap(s)
}

fn pss_pkcs8(key: &RsaPrivateKey) -> Result<Vec<u8>, String> {
    let inner = key.to_pkcs1_der().map_err(|e| e.to_string())?;
    let info = pkcs8::PrivateKeyInfo {
        algorithm: spki::AlgorithmIdentifierRef {
            oid: oid(OID_RSA_PSS),
            parameters: None,
        },
        private_key: inner.as_bytes(),
        public_key: None,
    };
    info.to_der().map_err(|e| e.to_string())
}

fn pss_spki(key: &RsaPublicKey) -> Result<Vec<u8>, String> {
    let inner = key.to_pkcs1_der().map_err(|e| e.to_string())?;
    let info = spki::SubjectPublicKeyInfoRef {
        algorithm: spki::AlgorithmIdentifierRef {
            oid: oid(OID_RSA_PSS),
            parameters: None,
        },
        subject_public_key: spki::der::asn1::BitStringRef::from_bytes(inner.as_bytes())
            .map_err(|e| e.to_string())?,
    };
    info.to_der().map_err(|e| e.to_string())
}

fn x25519_pkcs8(raw: &[u8; 32]) -> Result<Vec<u8>, String> {
    let inner = pkcs8::der::asn1::OctetStringRef::new(raw)
        .map_err(|e| e.to_string())?
        .to_der()
        .map_err(|e| e.to_string())?;
    let info = pkcs8::PrivateKeyInfo {
        algorithm: spki::AlgorithmIdentifierRef {
            oid: oid(OID_X25519),
            parameters: None,
        },
        private_key: &inner,
        public_key: None,
    };
    info.to_der().map_err(|e| e.to_string())
}

fn okp_spki(alg: &str, raw: &[u8]) -> Result<Vec<u8>, String> {
    let info = spki::SubjectPublicKeyInfoRef {
        algorithm: spki::AlgorithmIdentifierRef {
            oid: oid(alg),
            parameters: None,
        },
        subject_public_key: spki::der::asn1::BitStringRef::from_bytes(raw)
            .map_err(|e| e.to_string())?,
    };
    info.to_der().map_err(|e| e.to_string())
}

fn public_of(key: &Private) -> Result<Public, String> {
    Ok(match key {
        Private::Rsa(k, pss) => Public::Rsa(k.to_public_key(), *pss),
        Private::P256(k) => Public::P256(k.public_key()),
        Private::P384(k) => Public::P384(k.public_key()),
        Private::Ed25519(k) => Public::Ed25519(k.verifying_key()),
        Private::X25519(raw) => {
            let secret = x25519_dalek::StaticSecret::from(*raw);
            Public::X25519(x25519_dalek::PublicKey::from(&secret).to_bytes())
        }
    })
}

fn private_der(key: &Private) -> Result<Vec<u8>, String> {
    match key {
        Private::Rsa(k, false) => k
            .to_pkcs8_der()
            .map(|d| d.as_bytes().to_vec())
            .map_err(|e| e.to_string()),
        Private::Rsa(k, true) => pss_pkcs8(k),
        Private::P256(k) => k
            .to_pkcs8_der()
            .map(|d| d.as_bytes().to_vec())
            .map_err(|e| e.to_string()),
        Private::P384(k) => k
            .to_pkcs8_der()
            .map(|d| d.as_bytes().to_vec())
            .map_err(|e| e.to_string()),
        Private::Ed25519(k) => k
            .to_pkcs8_der()
            .map(|d| d.as_bytes().to_vec())
            .map_err(|e| e.to_string()),
        Private::X25519(raw) => x25519_pkcs8(raw),
    }
}

fn public_der(key: &Public) -> Result<Vec<u8>, String> {
    match key {
        Public::Rsa(k, false) => k
            .to_public_key_der()
            .map(|d| d.as_bytes().to_vec())
            .map_err(|e| e.to_string()),
        Public::Rsa(k, true) => pss_spki(k),
        Public::P256(k) => k
            .to_public_key_der()
            .map(|d| d.as_bytes().to_vec())
            .map_err(|e| e.to_string()),
        Public::P384(k) => k
            .to_public_key_der()
            .map(|d| d.as_bytes().to_vec())
            .map_err(|e| e.to_string()),
        Public::Ed25519(k) => okp_spki(OID_ED25519, k.as_bytes()),
        Public::X25519(raw) => okp_spki(OID_X25519, raw),
    }
}

fn describe(vm: &mut Vm, kind: &str, der: Vec<u8>, public: &Public) -> R {
    let o = vm.new_object();
    o.set_own("kind", kind.into());
    let (alg, curve) = match public {
        Public::Rsa(_, false) => ("rsa", None),
        Public::Rsa(_, true) => ("rsa-pss", None),
        Public::P256(_) => ("ec", Some("P-256")),
        Public::P384(_) => ("ec", Some("P-384")),
        Public::Ed25519(_) => ("ed25519", None),
        Public::X25519(_) => ("x25519", None),
    };
    o.set_own("alg", alg.into());
    if let Some(c) = curve {
        o.set_own("curve", c.into());
    }
    if let Public::Rsa(k, _) = public {
        o.set_own("modulusLength", Value::Number(k.n().bits() as f64));
        let e = num_bigint::BigInt::from_bytes_be(num_bigint::Sign::Plus, &k.e().to_bytes_be());
        o.set_own("publicExponent", Value::BigInt(Rc::new(e)));
    }
    let bytes = out_bytes(vm, der);
    o.set_own("der", bytes);
    Ok(Value::Object(o))
}

fn pem_decode(text: &str) -> Result<(String, Vec<u8>), String> {
    let start = text.find("-----BEGIN").ok_or("Invalid PEM")?;
    let (label, der) =
        pem_rfc7468::decode_vec(text[start..].trim().as_bytes()).map_err(|e| e.to_string())?;
    Ok((label.to_string(), der))
}

fn extract_cert_spki(der: &[u8]) -> Option<Vec<u8>> {
    fn read_tlv(b: &[u8], at: usize) -> Option<(u8, usize, usize)> {
        let tag = *b.get(at)?;
        let first = *b.get(at + 1)? as usize;
        let (len, head) = if first < 0x80 {
            (first, 2)
        } else {
            let n = first & 0x7f;
            let mut len = 0usize;
            for i in 0..n {
                len = (len << 8) | *b.get(at + 2 + i)? as usize;
            }
            (len, 2 + n)
        };
        Some((tag, at + head, at + head + len))
    }
    let (_, cert_body, _) = read_tlv(der, 0)?;
    let (_, tbs_body, tbs_end) = read_tlv(der, cert_body)?;
    let mut at = tbs_body;
    let mut index = 0;
    while at < tbs_end {
        let (tag, _, end) = read_tlv(der, at)?;
        if tag == 0xa0 {
            at = end;
            continue;
        }
        if index == 5 {
            return Some(der[at..end].to_vec());
        }
        index += 1;
        at = end;
    }
    None
}

fn import_der(
    der: &[u8],
    label_type: &str,
    passphrase: Option<&[u8]>,
) -> Result<(String, Vec<u8>, Public), String> {
    match label_type {
        "pkcs8" | "PRIVATE KEY" => {
            let key = parse_private(der)?;
            let public = public_of(&key)?;
            Ok(("private".into(), private_der(&key)?, public))
        }
        "ENCRYPTED PRIVATE KEY" => {
            let pass = passphrase.ok_or("Passphrase required for encrypted key")?;
            let info = pkcs8::EncryptedPrivateKeyInfo::from_der(der).map_err(|e| e.to_string())?;
            let doc = info.decrypt(pass).map_err(|e| e.to_string())?;
            import_der(doc.as_bytes(), "pkcs8", None)
        }
        "pkcs1" | "RSA PRIVATE KEY" => {
            let key = Private::Rsa(
                RsaPrivateKey::from_pkcs1_der(der).map_err(|e| e.to_string())?,
                false,
            );
            let public = public_of(&key)?;
            Ok(("private".into(), private_der(&key)?, public))
        }
        "sec1" | "EC PRIVATE KEY" => {
            let key = match p256::SecretKey::from_sec1_der(der) {
                Ok(k) => Private::P256(k),
                Err(_) => {
                    Private::P384(p384::SecretKey::from_sec1_der(der).map_err(|e| e.to_string())?)
                }
            };
            let public = public_of(&key)?;
            Ok(("private".into(), private_der(&key)?, public))
        }
        "spki" | "PUBLIC KEY" => {
            let public = parse_public(der)?;
            Ok(("public".into(), public_der(&public)?, public))
        }
        "pkcs1-public" | "RSA PUBLIC KEY" => {
            let public = Public::Rsa(
                RsaPublicKey::from_pkcs1_der(der).map_err(|e| e.to_string())?,
                false,
            );
            Ok(("public".into(), public_der(&public)?, public))
        }
        "CERTIFICATE" => {
            let spki = extract_cert_spki(der).ok_or("Invalid certificate")?;
            import_der(&spki, "spki", None)
        }
        _ => Err(format!("Unsupported key type {label_type}")),
    }
}

fn native_key_import(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let format = str_arg(vm, args, 1)?;
    let ty = str_arg(vm, args, 2)?;
    let want = str_arg(vm, args, 3)?;
    let passphrase = match arg(args, 4) {
        Value::Undefined | Value::Null => None,
        v => Some(bytes_arg(vm, &v)?),
    };
    let result = if format == "pem" || (format.is_empty() && matches!(arg(args, 0), Value::Str(_)))
    {
        let text = match arg(args, 0) {
            Value::Str(s) => s.to_string(),
            v => String::from_utf8_lossy(&bytes_arg(vm, &v)?).into_owned(),
        };
        pem_decode(&text).and_then(|(label, der)| import_der(&der, &label, passphrase.as_deref()))
    } else {
        let der = bytes_arg(vm, &arg(args, 0))?;
        let ty = match (ty.as_str(), want.as_str()) {
            ("pkcs1", "public") => "pkcs1-public",
            ("", "public") => "spki",
            ("", _) => "pkcs8",
            (t, _) => t,
        };
        import_der(&der, ty, passphrase.as_deref())
    };
    let (kind, der, public) = match result {
        Ok(r) => r,
        Err(msg) => {
            return Err(crypto_error(
                vm,
                "ERR_OSSL_UNSUPPORTED",
                &format!("error:1E08010C:DECODER routines::unsupported ({msg})"),
            ));
        }
    };
    if want == "public" && kind == "private" {
        let der = public_der(&public).map_err(|m| crypto_error(vm, "ERR_OSSL_UNSUPPORTED", &m))?;
        return describe(vm, "public", der, &public);
    }
    describe(vm, &kind, der, &public)
}

fn b64url(bytes: &[u8]) -> String {
    use std::fmt::Write;
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        let count = chunk.len() + 1;
        for i in 0..count {
            let _ = out.write_char(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn big(n: &BigUint) -> String {
    b64url(&n.to_bytes_be())
}

fn native_key_export_jwk(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let der = bytes_arg(vm, &arg(args, 0))?;
    let kind = str_arg(vm, args, 1)?;
    let mut fields: Vec<(&str, String)> = Vec::new();
    let result: Result<(), String> = (|| {
        if kind == "private" {
            match parse_private(&der)? {
                Private::Rsa(mut k, _) => {
                    let _ = k.precompute();
                    fields.push(("kty", "RSA".into()));
                    fields.push(("n", big(k.n())));
                    fields.push(("e", big(k.e())));
                    fields.push(("d", big(k.d())));
                    let p = k.primes();
                    if p.len() >= 2 {
                        fields.push(("p", big(&p[0])));
                        fields.push(("q", big(&p[1])));
                    }
                    if let (Some(dp), Some(dq)) = (k.dp(), k.dq()) {
                        fields.push(("dp", big(dp)));
                        fields.push(("dq", big(dq)));
                    }
                    if let Some(qi) = k.crt_coefficient() {
                        fields.push(("qi", big(&qi)));
                    }
                }
                Private::P256(k) => {
                    let point = k.public_key().to_sec1_bytes();
                    fields.push(("kty", "EC".into()));
                    fields.push(("crv", "P-256".into()));
                    fields.push(("x", b64url(&point[1..33])));
                    fields.push(("y", b64url(&point[33..65])));
                    fields.push(("d", b64url(&k.to_bytes())));
                }
                Private::P384(k) => {
                    let point = k.public_key().to_sec1_bytes();
                    fields.push(("kty", "EC".into()));
                    fields.push(("crv", "P-384".into()));
                    fields.push(("x", b64url(&point[1..49])));
                    fields.push(("y", b64url(&point[49..97])));
                    fields.push(("d", b64url(&k.to_bytes())));
                }
                Private::Ed25519(k) => {
                    fields.push(("crv", "Ed25519".into()));
                    fields.push(("d", b64url(&k.to_bytes())));
                    fields.push(("x", b64url(k.verifying_key().as_bytes())));
                    fields.push(("kty", "OKP".into()));
                }
                Private::X25519(raw) => {
                    let public =
                        x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(raw));
                    fields.push(("crv", "X25519".into()));
                    fields.push(("d", b64url(&raw)));
                    fields.push(("x", b64url(public.as_bytes())));
                    fields.push(("kty", "OKP".into()));
                }
            }
        } else {
            match parse_public(&der)? {
                Public::Rsa(k, _) => {
                    fields.push(("kty", "RSA".into()));
                    fields.push(("n", big(k.n())));
                    fields.push(("e", big(k.e())));
                }
                Public::P256(k) => {
                    let point = k.to_sec1_bytes();
                    fields.push(("kty", "EC".into()));
                    fields.push(("crv", "P-256".into()));
                    fields.push(("x", b64url(&point[1..33])));
                    fields.push(("y", b64url(&point[33..65])));
                }
                Public::P384(k) => {
                    let point = k.to_sec1_bytes();
                    fields.push(("kty", "EC".into()));
                    fields.push(("crv", "P-384".into()));
                    fields.push(("x", b64url(&point[1..49])));
                    fields.push(("y", b64url(&point[49..97])));
                }
                Public::Ed25519(k) => {
                    fields.push(("crv", "Ed25519".into()));
                    fields.push(("x", b64url(k.as_bytes())));
                    fields.push(("kty", "OKP".into()));
                }
                Public::X25519(raw) => {
                    fields.push(("crv", "X25519".into()));
                    fields.push(("x", b64url(&raw)));
                    fields.push(("kty", "OKP".into()));
                }
            }
        }
        Ok(())
    })();
    if let Err(m) = result {
        return Err(crypto_error(vm, "ERR_CRYPTO_INVALID_KEY_OBJECT_TYPE", &m));
    }
    let o = vm.new_object();
    for (k, v) in fields {
        o.set_own(k, v.as_str().into());
    }
    Ok(Value::Object(o))
}

fn jwk_field(vm: &mut Vm, jwk: &Value, key: &str) -> Result<Option<Vec<u8>>, Value> {
    match vm.get_prop(jwk, key)? {
        Value::Str(s) => Ok(b64url_decode(&s)),
        _ => Ok(None),
    }
}

fn native_key_import_jwk(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let jwk = arg(args, 0);
    let want_private = arg(args, 1).truthy();
    let kty = match vm.get_prop(&jwk, "kty")? {
        Value::Str(s) => s.to_string(),
        _ => String::new(),
    };
    let crv = match vm.get_prop(&jwk, "crv")? {
        Value::Str(s) => s.to_string(),
        _ => String::new(),
    };
    let field = |vm: &mut Vm, k: &str| jwk_field(vm, &jwk, k);
    let invalid = |vm: &mut Vm| crypto_error(vm, "ERR_CRYPTO_INVALID_JWK", "Invalid JWK data");
    let d = field(vm, "d")?;
    let private = want_private || d.is_some();
    let result: Result<(String, Vec<u8>, Public), String> = match kty.as_str() {
        "RSA" => {
            let n = field(vm, "n")?.ok_or("n")?;
            let e = field(vm, "e")?.ok_or("e")?;
            let n = BigUint::from_bytes_be(&n);
            let e = BigUint::from_bytes_be(&e);
            if private {
                let d = d.ok_or("d")?;
                let mut primes = Vec::new();
                if let (Some(p), Some(q)) = (field(vm, "p")?, field(vm, "q")?) {
                    primes.push(BigUint::from_bytes_be(&p));
                    primes.push(BigUint::from_bytes_be(&q));
                }
                RsaPrivateKey::from_components(n, e, BigUint::from_bytes_be(&d), primes)
                    .map_err(|e| e.to_string())
                    .and_then(|k| {
                        let key = Private::Rsa(k, false);
                        Ok(("private".into(), private_der(&key)?, public_of(&key)?))
                    })
            } else {
                RsaPublicKey::new(n, e)
                    .map_err(|e| e.to_string())
                    .and_then(|k| {
                        let p = Public::Rsa(k, false);
                        Ok(("public".into(), public_der(&p)?, p))
                    })
            }
        }
        "EC" => {
            let x = field(vm, "x")?.unwrap_or_default();
            let y = field(vm, "y")?.unwrap_or_default();
            let mut point = vec![4u8];
            point.extend(&x);
            point.extend(&y);
            match (crv.as_str(), private) {
                ("P-256", true) => p256::SecretKey::from_slice(&d.unwrap_or_default())
                    .map_err(|e| e.to_string())
                    .and_then(|k| {
                        let key = Private::P256(k);
                        Ok(("private".into(), private_der(&key)?, public_of(&key)?))
                    }),
                ("P-384", true) => p384::SecretKey::from_slice(&d.unwrap_or_default())
                    .map_err(|e| e.to_string())
                    .and_then(|k| {
                        let key = Private::P384(k);
                        Ok(("private".into(), private_der(&key)?, public_of(&key)?))
                    }),
                ("P-256", false) => p256::PublicKey::from_sec1_bytes(&point)
                    .map_err(|e| e.to_string())
                    .and_then(|k| {
                        let p = Public::P256(k);
                        Ok(("public".into(), public_der(&p)?, p))
                    }),
                ("P-384", false) => p384::PublicKey::from_sec1_bytes(&point)
                    .map_err(|e| e.to_string())
                    .and_then(|k| {
                        let p = Public::P384(k);
                        Ok(("public".into(), public_der(&p)?, p))
                    }),
                _ => Err("Unsupported JWK curve".into()),
            }
        }
        "OKP" => {
            let x = field(vm, "x")?.unwrap_or_default();
            match (crv.as_str(), private) {
                ("Ed25519", true) => {
                    let raw: Result<[u8; 32], _> = d.unwrap_or_default().try_into();
                    raw.map_err(|_| "Invalid Ed25519 key".to_string())
                        .and_then(|raw| {
                            let key = Private::Ed25519(ed25519_dalek::SigningKey::from_bytes(&raw));
                            Ok(("private".into(), private_der(&key)?, public_of(&key)?))
                        })
                }
                ("X25519", true) => {
                    let raw: Result<[u8; 32], _> = d.unwrap_or_default().try_into();
                    raw.map_err(|_| "Invalid X25519 key".to_string())
                        .and_then(|raw| {
                            let key = Private::X25519(raw);
                            Ok(("private".into(), private_der(&key)?, public_of(&key)?))
                        })
                }
                ("Ed25519", false) => {
                    let raw: Result<[u8; 32], _> = x.try_into();
                    raw.map_err(|_| "Invalid Ed25519 key".to_string())
                        .and_then(|raw| {
                            let p = Public::Ed25519(
                                ed25519_dalek::VerifyingKey::from_bytes(&raw)
                                    .map_err(|e| e.to_string())?,
                            );
                            Ok(("public".into(), public_der(&p)?, p))
                        })
                }
                ("X25519", false) => {
                    let raw: Result<[u8; 32], _> = x.try_into();
                    raw.map_err(|_| "Invalid X25519 key".to_string())
                        .and_then(|raw| {
                            let p = Public::X25519(raw);
                            Ok(("public".into(), public_der(&p)?, p))
                        })
                }
                _ => Err("Unsupported JWK curve".into()),
            }
        }
        _ => Err("Unsupported JWK key type".into()),
    };
    match result {
        Ok((kind, der, public)) => describe(vm, &kind, der, &public),
        Err(_) => Err(invalid(vm)),
    }
}

fn native_key_export(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let der = bytes_arg(vm, &arg(args, 0))?;
    let kind = str_arg(vm, args, 1)?;
    let format = str_arg(vm, args, 2)?;
    let ty = str_arg(vm, args, 3)?;
    let passphrase = match arg(args, 4) {
        Value::Undefined | Value::Null => None,
        v => Some(bytes_arg(vm, &v)?),
    };
    let result: Result<(String, Vec<u8>), String> = (|| {
        if kind == "private" {
            let key = parse_private(&der)?;
            match ty.as_str() {
                "pkcs1" => match &key {
                    Private::Rsa(k, _) => Ok((
                        "RSA PRIVATE KEY".into(),
                        k.to_pkcs1_der()
                            .map_err(|e| e.to_string())?
                            .as_bytes()
                            .to_vec(),
                    )),
                    _ => Err("pkcs1 export requires an RSA key".into()),
                },
                "sec1" => match &key {
                    Private::P256(k) => Ok((
                        "EC PRIVATE KEY".into(),
                        k.to_sec1_der().map_err(|e| e.to_string())?.to_vec(),
                    )),
                    Private::P384(k) => Ok((
                        "EC PRIVATE KEY".into(),
                        k.to_sec1_der().map_err(|e| e.to_string())?.to_vec(),
                    )),
                    _ => Err("sec1 export requires an EC key".into()),
                },
                _ => match passphrase {
                    Some(pass) => {
                        let info =
                            pkcs8::PrivateKeyInfo::from_der(&der).map_err(|e| e.to_string())?;
                        let doc = info.encrypt(OsRng, &pass).map_err(|e| e.to_string())?;
                        Ok(("ENCRYPTED PRIVATE KEY".into(), doc.as_bytes().to_vec()))
                    }
                    None => Ok(("PRIVATE KEY".into(), der.clone())),
                },
            }
        } else {
            match ty.as_str() {
                "pkcs1" => match parse_public(&der)? {
                    Public::Rsa(k, _) => Ok((
                        "RSA PUBLIC KEY".into(),
                        k.to_pkcs1_der()
                            .map_err(|e| e.to_string())?
                            .as_bytes()
                            .to_vec(),
                    )),
                    _ => Err("pkcs1 export requires an RSA key".into()),
                },
                _ => Ok(("PUBLIC KEY".into(), der.clone())),
            }
        }
    })();
    let (label, out) = match result {
        Ok(r) => r,
        Err(m) => return Err(crypto_error(vm, "ERR_CRYPTO_INCOMPATIBLE_KEY_OPTIONS", &m)),
    };
    if format == "pem" {
        let text = pem_rfc7468::encode_string(&label, pem_rfc7468::LineEnding::LF, &out)
            .map_err(|e| crypto_error(vm, "ERR_CRYPTO_INVALID_KEY_OBJECT_TYPE", &e.to_string()))?;
        return Ok(text.into());
    }
    Ok(out_bytes(vm, out))
}

fn native_key_generate(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let alg = str_arg(vm, args, 0)?;
    let bits = vm.to_number(&arg(args, 1))?;
    let exponent = match arg(args, 2) {
        Value::Number(n) if n > 0.0 => n as u64,
        _ => 65537,
    };
    let curve = str_arg(vm, args, 3)?;
    let key: Result<Private, String> = match alg.as_str() {
        "rsa" | "rsa-pss" => {
            let bits = if bits.is_finite() && bits >= 512.0 {
                bits as usize
            } else {
                2048
            };
            RsaPrivateKey::new_with_exp(&mut OsRng, bits, &BigUint::from(exponent))
                .map(|k| Private::Rsa(k, alg == "rsa-pss"))
                .map_err(|e| e.to_string())
        }
        "ec" => match curve.as_str() {
            "P-256" | "prime256v1" | "secp256r1" => {
                Ok(Private::P256(p256::SecretKey::random(&mut OsRng)))
            }
            "P-384" | "secp384r1" => Ok(Private::P384(p384::SecretKey::random(&mut OsRng))),
            _ => Err(format!("Invalid EC curve name {curve}")),
        },
        "ed25519" => Ok(Private::Ed25519(ed25519_dalek::SigningKey::generate(
            &mut OsRng,
        ))),
        "x25519" => Ok(Private::X25519(
            x25519_dalek::StaticSecret::random_from_rng(OsRng).to_bytes(),
        )),
        _ => Err(format!(
            "The argument 'type' must be a supported key type. Received '{alg}'"
        )),
    };
    let key = match key {
        Ok(k) => k,
        Err(m) => return Err(crypto_error(vm, "ERR_INVALID_ARG_VALUE", &m)),
    };
    let public =
        public_of(&key).map_err(|m| crypto_error(vm, "ERR_CRYPTO_OPERATION_FAILED", &m))?;
    let private =
        private_der(&key).map_err(|m| crypto_error(vm, "ERR_CRYPTO_OPERATION_FAILED", &m))?;
    let spki =
        public_der(&public).map_err(|m| crypto_error(vm, "ERR_CRYPTO_OPERATION_FAILED", &m))?;
    let out = vm.new_object();
    let a = describe(vm, "private", private, &public)?;
    let b = describe(vm, "public", spki, &public)?;
    out.set_own("privateKey", a);
    out.set_own("publicKey", b);
    Ok(Value::Object(out))
}

fn digest_with(hash: &str, data: &[u8]) -> Option<Vec<u8>> {
    use sha2::Digest;
    Some(match hash {
        "sha1" => sha1::Sha1::digest(data).to_vec(),
        "sha224" => sha2::Sha224::digest(data).to_vec(),
        "sha256" => sha2::Sha256::digest(data).to_vec(),
        "sha384" => sha2::Sha384::digest(data).to_vec(),
        "sha512" => sha2::Sha512::digest(data).to_vec(),
        "md5" => md5::Md5::digest(data).to_vec(),
        _ => return None,
    })
}

fn norm_hash(name: &str) -> String {
    let n = name.to_ascii_lowercase().replace('-', "");
    let n = n.strip_prefix("rsa").unwrap_or(&n).to_string();
    let n = n.strip_prefix("ecdsawith").unwrap_or(&n).to_string();
    match n.as_str() {
        "sha1" | "sha" => "sha1".into(),
        "sha224" => "sha224".into(),
        "sha256" => "sha256".into(),
        "sha384" => "sha384".into(),
        "sha512" => "sha512".into(),
        "md5" => "md5".into(),
        other => other.to_string(),
    }
}

fn pkcs1v15(hash: &str) -> Option<rsa::Pkcs1v15Sign> {
    Some(match hash {
        "sha1" => rsa::Pkcs1v15Sign::new::<sha1::Sha1>(),
        "sha224" => rsa::Pkcs1v15Sign::new::<sha2::Sha224>(),
        "sha256" => rsa::Pkcs1v15Sign::new::<sha2::Sha256>(),
        "sha384" => rsa::Pkcs1v15Sign::new::<sha2::Sha384>(),
        "sha512" => rsa::Pkcs1v15Sign::new::<sha2::Sha512>(),
        "md5" => rsa::Pkcs1v15Sign::new::<md5::Md5>(),
        _ => return None,
    })
}

fn pss(hash: &str, salt: Option<usize>) -> Option<rsa::Pss> {
    macro_rules! make {
        ($d:ty) => {
            match salt {
                Some(s) => rsa::Pss::new_with_salt::<$d>(s),
                None => rsa::Pss::new::<$d>(),
            }
        };
    }
    Some(match hash {
        "sha1" => make!(sha1::Sha1),
        "sha224" => make!(sha2::Sha224),
        "sha256" => make!(sha2::Sha256),
        "sha384" => make!(sha2::Sha384),
        "sha512" => make!(sha2::Sha512),
        _ => return None,
    })
}

fn native_sign(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let hash = norm_hash(&str_arg(vm, args, 0)?);
    let data = bytes_arg(vm, &arg(args, 1))?;
    let der = bytes_arg(vm, &arg(args, 2))?;
    let padding = vm.to_number(&arg(args, 3))?;
    let salt = match arg(args, 4) {
        Value::Number(n) if n >= 0.0 => Some(n as usize),
        _ => None,
    };
    let p1363 = str_arg(vm, args, 5)? == "ieee-p1363";
    let key = parse_private(&der).map_err(|m| crypto_error(vm, "ERR_OSSL_UNSUPPORTED", &m))?;
    let result: Result<Vec<u8>, String> = match &key {
        Private::Rsa(k, is_pss) => {
            let hash = if hash.is_empty() {
                "sha256".to_string()
            } else {
                hash.clone()
            };
            let digest = digest_with(&hash, &data).ok_or("Invalid digest")?;
            if padding == 6.0 || *is_pss {
                let em_len = (k.n().bits() + 6) / 8;
                let max = em_len.saturating_sub(digest.len() + 2);
                let salt = Some(salt.unwrap_or(max).min(max));
                let scheme = pss(&hash, salt).ok_or("Invalid digest")?;
                k.sign_with_rng(&mut OsRng, scheme, &digest)
                    .map_err(|e| e.to_string())
            } else {
                k.sign(pkcs1v15(&hash).ok_or("Invalid digest")?, &digest)
                    .map_err(|e| e.to_string())
            }
        }
        Private::P256(k) => {
            use p256::ecdsa::signature::hazmat::PrehashSigner;
            let hash = if hash.is_empty() {
                "sha256".to_string()
            } else {
                hash.clone()
            };
            let digest = digest_with(&hash, &data).ok_or("Invalid digest")?;
            let sk = p256::ecdsa::SigningKey::from(k);
            let sig: p256::ecdsa::Signature =
                sk.sign_prehash(&digest).map_err(|e| e.to_string())?;
            Ok(if p1363 {
                sig.to_bytes().to_vec()
            } else {
                sig.to_der().as_bytes().to_vec()
            })
        }
        Private::P384(k) => {
            use p384::ecdsa::signature::hazmat::PrehashSigner;
            let hash = if hash.is_empty() {
                "sha384".to_string()
            } else {
                hash.clone()
            };
            let digest = digest_with(&hash, &data).ok_or("Invalid digest")?;
            let sk = p384::ecdsa::SigningKey::from(k);
            let sig: p384::ecdsa::Signature =
                sk.sign_prehash(&digest).map_err(|e| e.to_string())?;
            Ok(if p1363 {
                sig.to_bytes().to_vec()
            } else {
                sig.to_der().as_bytes().to_vec()
            })
        }
        Private::Ed25519(k) => {
            use ed25519_dalek::Signer;
            Ok(k.sign(&data).to_bytes().to_vec())
        }
        Private::X25519(_) => Err("X25519 keys cannot be used for signing".into()),
    };
    match result {
        Ok(sig) => Ok(out_bytes(vm, sig)),
        Err(m) => Err(crypto_error(
            vm,
            "ERR_OSSL_EVP_OPERATION_NOT_SUPPORTED_FOR_THIS_KEYTYPE",
            &m,
        )),
    }
}

fn native_verify(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let hash = norm_hash(&str_arg(vm, args, 0)?);
    let data = bytes_arg(vm, &arg(args, 1))?;
    let der = bytes_arg(vm, &arg(args, 2))?;
    let sig = bytes_arg(vm, &arg(args, 3))?;
    let padding = vm.to_number(&arg(args, 4))?;
    let salt = match arg(args, 5) {
        Value::Number(n) if n >= 0.0 => Some(n as usize),
        _ => None,
    };
    let p1363 = str_arg(vm, args, 6)? == "ieee-p1363";
    let key = parse_public(&der).map_err(|m| crypto_error(vm, "ERR_OSSL_UNSUPPORTED", &m))?;
    let ok = match &key {
        Public::Rsa(k, is_pss) => {
            let hash = if hash.is_empty() {
                "sha256".to_string()
            } else {
                hash.clone()
            };
            let Some(digest) = digest_with(&hash, &data) else {
                return Ok(Value::Bool(false));
            };
            if padding == 6.0 || *is_pss {
                let em_len = (k.n().bits() + 6) / 8;
                let max = em_len.saturating_sub(digest.len() + 2);
                let candidates: Vec<usize> = match salt {
                    Some(s) => vec![s],
                    None => {
                        let mut c = vec![max, digest.len(), 0];
                        c.extend((1..max).filter(|n| *n != digest.len()));
                        c
                    }
                };
                candidates.into_iter().any(|s| match pss(&hash, Some(s)) {
                    Some(scheme) => k.verify(scheme, &digest, &sig).is_ok(),
                    None => false,
                })
            } else {
                match pkcs1v15(&hash) {
                    Some(scheme) => k.verify(scheme, &digest, &sig).is_ok(),
                    None => false,
                }
            }
        }
        Public::P256(k) => {
            use p256::ecdsa::signature::hazmat::PrehashVerifier;
            let hash = if hash.is_empty() {
                "sha256".to_string()
            } else {
                hash.clone()
            };
            let digest = digest_with(&hash, &data).unwrap_or_default();
            let parsed = if p1363 {
                p256::ecdsa::Signature::from_slice(&sig)
            } else {
                p256::ecdsa::Signature::from_der(&sig)
            };
            match parsed {
                Ok(s) => p256::ecdsa::VerifyingKey::from(k)
                    .verify_prehash(&digest, &s)
                    .is_ok(),
                Err(_) => false,
            }
        }
        Public::P384(k) => {
            use p384::ecdsa::signature::hazmat::PrehashVerifier;
            let hash = if hash.is_empty() {
                "sha384".to_string()
            } else {
                hash.clone()
            };
            let digest = digest_with(&hash, &data).unwrap_or_default();
            let parsed = if p1363 {
                p384::ecdsa::Signature::from_slice(&sig)
            } else {
                p384::ecdsa::Signature::from_der(&sig)
            };
            match parsed {
                Ok(s) => p384::ecdsa::VerifyingKey::from(k)
                    .verify_prehash(&digest, &s)
                    .is_ok(),
                Err(_) => false,
            }
        }
        Public::Ed25519(k) => match ed25519_dalek::Signature::from_slice(&sig) {
            Ok(s) => k.verify_strict(&data, &s).is_ok(),
            Err(_) => false,
        },
        Public::X25519(_) => false,
    };
    Ok(Value::Bool(ok))
}

fn native_derive(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let private = bytes_arg(vm, &arg(args, 0))?;
    let public = bytes_arg(vm, &arg(args, 1))?;
    let key = parse_private(&private).map_err(|m| crypto_error(vm, "ERR_OSSL_UNSUPPORTED", &m))?;
    let peer = parse_public(&public).map_err(|m| crypto_error(vm, "ERR_OSSL_UNSUPPORTED", &m))?;
    let shared: Result<Vec<u8>, &str> = match (&key, &peer) {
        (Private::P256(a), Public::P256(b)) => Ok(p256::ecdh::diffie_hellman(
            a.to_nonzero_scalar(),
            b.as_affine(),
        )
        .raw_secret_bytes()
        .to_vec()),
        (Private::P384(a), Public::P384(b)) => Ok(p384::ecdh::diffie_hellman(
            a.to_nonzero_scalar(),
            b.as_affine(),
        )
        .raw_secret_bytes()
        .to_vec()),
        (Private::X25519(a), Public::X25519(b)) => {
            let secret = x25519_dalek::StaticSecret::from(*a);
            Ok(secret
                .diffie_hellman(&x25519_dalek::PublicKey::from(*b))
                .to_bytes()
                .to_vec())
        }
        _ => Err("Incompatible key types for Diffie-Hellman"),
    };
    match shared {
        Ok(s) => Ok(out_bytes(vm, s)),
        Err(m) => Err(crypto_error(vm, "ERR_CRYPTO_INCOMPATIBLE_KEY", m)),
    }
}

fn native_ecdh(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let op = str_arg(vm, args, 0)?;
    let curve = str_arg(vm, args, 1)?;
    let a = bytes_arg(vm, &arg(args, 2))?;
    let b = bytes_arg(vm, &arg(args, 3))?;
    let compressed = arg(args, 4).truthy();
    macro_rules! run {
        ($m:ident) => {{
            use $m::elliptic_curve::sec1::ToEncodedPoint;
            match op.as_str() {
                "generate" => Ok($m::SecretKey::random(&mut OsRng).to_bytes().to_vec()),
                "public" => $m::SecretKey::from_slice(&a)
                    .map(|k| {
                        k.public_key()
                            .to_encoded_point(compressed)
                            .as_bytes()
                            .to_vec()
                    })
                    .map_err(|_| "Private key is not valid for specified curve."),
                "compute" => match (
                    $m::SecretKey::from_slice(&a),
                    $m::PublicKey::from_sec1_bytes(&b),
                ) {
                    (Ok(k), Ok(p)) => Ok($m::ecdh::diffie_hellman(
                        k.to_nonzero_scalar(),
                        p.as_affine(),
                    )
                    .raw_secret_bytes()
                    .to_vec()),
                    (Err(_), _) => Err("Private key is not valid for specified curve."),
                    _ => Err("Public key is not valid for specified curve"),
                },
                "convert" => $m::PublicKey::from_sec1_bytes(&b)
                    .map(|p| p.to_encoded_point(compressed).as_bytes().to_vec())
                    .map_err(|_| "Public key is not valid for specified curve"),
                _ => Err("Unknown operation"),
            }
        }};
    }
    let result: Result<Vec<u8>, &str> = match curve.as_str() {
        "prime256v1" | "secp256r1" | "P-256" => run!(p256),
        "secp384r1" | "P-384" => run!(p384),
        "x25519" | "X25519" => match op.as_str() {
            "generate" => Ok(x25519_dalek::StaticSecret::random_from_rng(OsRng)
                .to_bytes()
                .to_vec()),
            "public" => match <[u8; 32]>::try_from(a.as_slice()) {
                Ok(raw) => Ok(
                    x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(raw))
                        .to_bytes()
                        .to_vec(),
                ),
                Err(_) => Err("Private key is not valid for specified curve."),
            },
            "compute" => match (
                <[u8; 32]>::try_from(a.as_slice()),
                <[u8; 32]>::try_from(b.as_slice()),
            ) {
                (Ok(k), Ok(p)) => Ok(x25519_dalek::StaticSecret::from(k)
                    .diffie_hellman(&x25519_dalek::PublicKey::from(p))
                    .to_bytes()
                    .to_vec()),
                _ => Err("Public key is not valid for specified curve"),
            },
            _ => Ok(b.clone()),
        },
        _ => Err("Invalid EC curve name"),
    };
    match result {
        Ok(out) => Ok(out_bytes(vm, out)),
        Err(m) => Err(crypto_error(vm, "ERR_CRYPTO_ECDH_INVALID_PUBLIC_KEY", m)),
    }
}

fn native_key_raw(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let op = str_arg(vm, args, 0)?;
    let alg = str_arg(vm, args, 1)?;
    let data = bytes_arg(vm, &arg(args, 2))?;
    let result: Result<Value, String> = match op.as_str() {
        "export" => parse_public(&data)
            .map(|p| match p {
                Public::P256(k) => k.to_sec1_bytes().to_vec(),
                Public::P384(k) => k.to_sec1_bytes().to_vec(),
                Public::Ed25519(k) => k.as_bytes().to_vec(),
                Public::X25519(raw) => raw.to_vec(),
                Public::Rsa(k, _) => k
                    .to_pkcs1_der()
                    .map(|d| d.as_bytes().to_vec())
                    .unwrap_or_default(),
            })
            .map(|b| out_bytes(vm, b)),
        _ => {
            let public = match alg.as_str() {
                "P-256" => p256::PublicKey::from_sec1_bytes(&data)
                    .map(Public::P256)
                    .map_err(|e| e.to_string()),
                "P-384" => p384::PublicKey::from_sec1_bytes(&data)
                    .map(Public::P384)
                    .map_err(|e| e.to_string()),
                "Ed25519" => <[u8; 32]>::try_from(data.as_slice())
                    .map_err(|_| "Invalid key length".to_string())
                    .and_then(|raw| {
                        ed25519_dalek::VerifyingKey::from_bytes(&raw)
                            .map(Public::Ed25519)
                            .map_err(|e| e.to_string())
                    }),
                "X25519" => <[u8; 32]>::try_from(data.as_slice())
                    .map(Public::X25519)
                    .map_err(|_| "Invalid key length".to_string()),
                _ => Err("Unsupported raw key".into()),
            };
            match public {
                Ok(p) => match public_der(&p) {
                    Ok(der) => describe(vm, "public", der, &p).map_err(|_| "describe".to_string()),
                    Err(m) => Err(m),
                },
                Err(m) => Err(m),
            }
        }
    };
    match result {
        Ok(v) => Ok(v),
        Err(m) => Err(crypto_error(vm, "ERR_CRYPTO_INVALID_KEYTYPE", &m)),
    }
}

fn native_rsa(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let op = str_arg(vm, args, 0)?;
    let der = bytes_arg(vm, &arg(args, 1))?;
    let data = bytes_arg(vm, &arg(args, 2))?;
    let padding = match arg(args, 3) {
        Value::Number(n) => n as u32,
        _ => 4,
    };
    let hash = {
        let h = norm_hash(&str_arg(vm, args, 4)?);
        if h.is_empty() { "sha1".to_string() } else { h }
    };
    let label = match arg(args, 5) {
        Value::Undefined | Value::Null => None,
        v => Some(String::from_utf8_lossy(&bytes_arg(vm, &v)?).into_owned()),
    };
    macro_rules! oaep {
        () => {
            match (hash.as_str(), label.clone()) {
                ("sha256", Some(l)) => rsa::Oaep::new_with_label::<sha2::Sha256, _>(l),
                ("sha256", None) => rsa::Oaep::new::<sha2::Sha256>(),
                ("sha384", Some(l)) => rsa::Oaep::new_with_label::<sha2::Sha384, _>(l),
                ("sha384", None) => rsa::Oaep::new::<sha2::Sha384>(),
                ("sha512", Some(l)) => rsa::Oaep::new_with_label::<sha2::Sha512, _>(l),
                ("sha512", None) => rsa::Oaep::new::<sha2::Sha512>(),
                (_, Some(l)) => rsa::Oaep::new_with_label::<sha1::Sha1, _>(l),
                (_, None) => rsa::Oaep::new::<sha1::Sha1>(),
            }
        };
    }
    let public_key = |der: &[u8]| -> Result<RsaPublicKey, String> {
        if let Ok(Private::Rsa(k, _)) = parse_private(der) {
            return Ok(k.to_public_key());
        }
        match parse_public(der)? {
            Public::Rsa(k, _) => Ok(k),
            _ => Err("RSA key required".into()),
        }
    };
    let private_key = |der: &[u8]| -> Result<RsaPrivateKey, String> {
        match parse_private(der)? {
            Private::Rsa(k, _) => Ok(k),
            _ => Err("RSA key required".into()),
        }
    };
    let result: Result<Vec<u8>, String> = match op.as_str() {
        "publicEncrypt" => public_key(&der).and_then(|k| {
            if padding == 1 {
                k.encrypt(&mut OsRng, rsa::Pkcs1v15Encrypt, &data)
                    .map_err(|e| e.to_string())
            } else {
                k.encrypt(&mut OsRng, oaep!(), &data)
                    .map_err(|e| e.to_string())
            }
        }),
        "privateDecrypt" => private_key(&der).and_then(|k| {
            if padding == 1 {
                k.decrypt(rsa::Pkcs1v15Encrypt, &data)
                    .map_err(|e| e.to_string())
            } else {
                k.decrypt(oaep!(), &data).map_err(|e| e.to_string())
            }
        }),
        "privateEncrypt" => private_key(&der).and_then(|k| {
            k.sign(rsa::Pkcs1v15Sign::new_unprefixed(), &data)
                .map_err(|e| e.to_string())
        }),
        "publicDecrypt" => public_key(&der).and_then(|k| {
            let c = BigUint::from_bytes_be(&data);
            let m = rsa::hazmat::rsa_encrypt(&k, &c).map_err(|e| e.to_string())?;
            let mut em = m.to_bytes_be();
            let size = k.size();
            while em.len() < size {
                em.insert(0, 0);
            }
            if em.len() < 11 || em[0] != 0 || em[1] != 1 {
                return Err("error:02000079:rsa routines::padding check failed".into());
            }
            let sep = em[2..]
                .iter()
                .position(|b| *b == 0)
                .ok_or("padding check failed")?;
            Ok(em[2 + sep + 1..].to_vec())
        }),
        _ => Err("Unknown RSA operation".into()),
    };
    match result {
        Ok(out) => Ok(out_bytes(vm, out)),
        Err(m) => Err(crypto_error(vm, "ERR_OSSL_RSA_OPERATION_FAILED", &m)),
    }
}

fn native_scrypt(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let password = bytes_arg(vm, &arg(args, 0))?;
    let salt = bytes_arg(vm, &arg(args, 1))?;
    let n = vm.to_number(&arg(args, 2))?;
    let r = vm.to_number(&arg(args, 3))? as u32;
    let p = vm.to_number(&arg(args, 4))? as u32;
    let len = vm.to_number(&arg(args, 5))? as usize;
    if n < 2.0 || (n as u64).count_ones() != 1 {
        return Err(crypto_error(
            vm,
            "ERR_CRYPTO_INVALID_SCRYPT_PARAMS",
            "Invalid scrypt params: N must be a power of two",
        ));
    }
    let log_n = (n as u64).trailing_zeros() as u8;
    let params = match scrypt::Params::new(log_n, r, p, len.max(10)) {
        Ok(p) => p,
        Err(e) => {
            return Err(crypto_error(
                vm,
                "ERR_CRYPTO_INVALID_SCRYPT_PARAMS",
                &format!("Invalid scrypt params: {e}"),
            ));
        }
    };
    let mut out = vec![0u8; len];
    if let Err(e) = scrypt::scrypt(&password, &salt, &params, &mut out) {
        return Err(crypto_error(
            vm,
            "ERR_CRYPTO_INVALID_SCRYPT_PARAMS",
            &e.to_string(),
        ));
    }
    Ok(out_bytes(vm, out))
}

enum AesKey {
    A128(aes::Aes128),
    A192(aes::Aes192),
    A256(aes::Aes256),
}

impl AesKey {
    fn new(key: &[u8]) -> Option<AesKey> {
        Some(match key.len() {
            16 => AesKey::A128(aes::Aes128::new_from_slice(key).ok()?),
            24 => AesKey::A192(aes::Aes192::new_from_slice(key).ok()?),
            32 => AesKey::A256(aes::Aes256::new_from_slice(key).ok()?),
            _ => return None,
        })
    }

    fn encrypt(&self, block: &mut [u8; 16]) {
        let b = aes::Block::from_mut_slice(block);
        match self {
            AesKey::A128(k) => k.encrypt_block(b),
            AesKey::A192(k) => k.encrypt_block(b),
            AesKey::A256(k) => k.encrypt_block(b),
        }
    }

    fn decrypt(&self, block: &mut [u8; 16]) {
        let b = aes::Block::from_mut_slice(block);
        match self {
            AesKey::A128(k) => k.decrypt_block(b),
            AesKey::A192(k) => k.decrypt_block(b),
            AesKey::A256(k) => k.decrypt_block(b),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Cbc,
    Ecb,
    Ctr,
    Gcm,
    ChaCha,
}

struct CipherCtx {
    mode: Mode,
    encrypt: bool,
    key: Vec<u8>,
    aes: Option<AesKey>,
    iv: [u8; 16],
    nonce: Vec<u8>,
    buf: Vec<u8>,
    pad: bool,
    aad: Vec<u8>,
    tag: Option<Vec<u8>>,
    tag_len: usize,
    stream: [u8; 16],
    used: usize,
}

thread_local! {
    static CIPHERS: RefCell<FxHashMap<u64, CipherCtx>> = RefCell::new(FxHashMap::default());
}

fn cipher_spec(name: &str) -> Option<(Mode, usize, usize)> {
    let n = name.to_ascii_lowercase();
    Some(match n.as_str() {
        "aes-128-cbc" | "aes128" => (Mode::Cbc, 16, 16),
        "aes-192-cbc" | "aes192" => (Mode::Cbc, 24, 16),
        "aes-256-cbc" | "aes256" => (Mode::Cbc, 32, 16),
        "aes-128-ecb" => (Mode::Ecb, 16, 0),
        "aes-192-ecb" => (Mode::Ecb, 24, 0),
        "aes-256-ecb" => (Mode::Ecb, 32, 0),
        "aes-128-ctr" => (Mode::Ctr, 16, 16),
        "aes-192-ctr" => (Mode::Ctr, 24, 16),
        "aes-256-ctr" => (Mode::Ctr, 32, 16),
        "aes-128-gcm" | "id-aes128-gcm" => (Mode::Gcm, 16, 12),
        "aes-192-gcm" | "id-aes192-gcm" => (Mode::Gcm, 24, 12),
        "aes-256-gcm" | "id-aes256-gcm" => (Mode::Gcm, 32, 12),
        "chacha20-poly1305" => (Mode::ChaCha, 32, 12),
        _ => return None,
    })
}

pub const CIPHER_NAMES: [&str; 13] = [
    "aes-128-cbc",
    "aes-128-ctr",
    "aes-128-ecb",
    "aes-128-gcm",
    "aes-192-cbc",
    "aes-192-ctr",
    "aes-192-ecb",
    "aes-192-gcm",
    "aes-256-cbc",
    "aes-256-ctr",
    "aes-256-ecb",
    "aes-256-gcm",
    "chacha20-poly1305",
];

fn native_cipher_new(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let name = str_arg(vm, args, 0)?;
    let key = bytes_arg(vm, &arg(args, 1))?;
    let iv = bytes_arg(vm, &arg(args, 2))?;
    let encrypt = arg(args, 3).truthy();
    let tag_len = match arg(args, 4) {
        Value::Number(n) if n > 0.0 => n as usize,
        _ => 16,
    };
    let Some((mode, key_len, iv_len)) = cipher_spec(&name) else {
        return Err(crypto_error(
            vm,
            "ERR_CRYPTO_UNKNOWN_CIPHER",
            "Unknown cipher",
        ));
    };
    if key.len() != key_len {
        let e = vm.range_error("Invalid key length");
        if let Value::Object(o) = &e {
            o.set_own("code", "ERR_CRYPTO_INVALID_KEYLEN".into());
        }
        return Err(e);
    }
    let iv_ok = match mode {
        Mode::Gcm => !iv.is_empty(),
        Mode::Ecb => iv.is_empty(),
        _ => iv.len() == iv_len,
    };
    if !iv_ok {
        return Err(crypto_error(
            vm,
            "ERR_CRYPTO_INVALID_IV",
            "Invalid initialization vector",
        ));
    }
    if matches!(mode, Mode::Gcm) && iv.len() != 12 {
        return Err(crypto_error(
            vm,
            "ERR_CRYPTO_INVALID_IV",
            "Only 12-byte GCM initialization vectors are supported",
        ));
    }
    let mut block = [0u8; 16];
    if mode == Mode::Cbc || mode == Mode::Ctr {
        block.copy_from_slice(&iv);
    }
    let ctx = CipherCtx {
        mode,
        encrypt,
        aes: AesKey::new(&key),
        key,
        iv: block,
        nonce: iv,
        buf: Vec::new(),
        pad: true,
        aad: Vec::new(),
        tag: None,
        tag_len,
        stream: [0u8; 16],
        used: 16,
    };
    let id = super::threads::next_id();
    CIPHERS.with(|c| c.borrow_mut().insert(id, ctx));
    Ok(Value::Number(id as f64))
}

fn with_ctx<T>(id: u64, f: impl FnOnce(&mut CipherCtx) -> T) -> Option<T> {
    CIPHERS.with(|c| c.borrow_mut().get_mut(&id).map(f))
}

fn block_process(ctx: &mut CipherCtx, data: &[u8], last: bool) -> Result<Vec<u8>, &'static str> {
    let Some(aes) = &ctx.aes else {
        return Err("bad key");
    };
    ctx.buf.extend_from_slice(data);
    let mut keep = ctx.buf.len() % 16;
    if !ctx.encrypt && ctx.pad && keep == 0 && !last && !ctx.buf.is_empty() {
        keep = 16;
    }
    let full = if last {
        ctx.buf.len()
    } else {
        ctx.buf.len() - keep
    };
    let mut out = Vec::with_capacity(full + 16);
    let mut work: Vec<u8> = ctx.buf.drain(..full).collect();
    if last && ctx.encrypt {
        if ctx.pad {
            let n = 16 - work.len() % 16;
            work.extend(std::iter::repeat_n(n as u8, n));
        } else if work.len() % 16 != 0 {
            return Err("wrong final block length");
        }
    }
    if last && !ctx.encrypt && work.len() % 16 != 0 {
        return Err("wrong final block length");
    }
    for chunk in work.chunks(16) {
        let mut b = [0u8; 16];
        b.copy_from_slice(chunk);
        if ctx.encrypt {
            if ctx.mode == Mode::Cbc {
                for (x, y) in b.iter_mut().zip(ctx.iv.iter()) {
                    *x ^= y;
                }
            }
            aes.encrypt(&mut b);
            if ctx.mode == Mode::Cbc {
                ctx.iv = b;
            }
            out.extend_from_slice(&b);
        } else {
            let cipher = b;
            aes.decrypt(&mut b);
            if ctx.mode == Mode::Cbc {
                for (x, y) in b.iter_mut().zip(ctx.iv.iter()) {
                    *x ^= y;
                }
                ctx.iv = cipher;
            }
            out.extend_from_slice(&b);
        }
    }
    if last && !ctx.encrypt && ctx.pad {
        let n = *out.last().ok_or("bad decrypt")? as usize;
        if n == 0
            || n > 16
            || n > out.len()
            || out[out.len() - n..].iter().any(|b| *b as usize != n)
        {
            return Err("bad decrypt");
        }
        out.truncate(out.len() - n);
    }
    Ok(out)
}

fn ctr_process(ctx: &mut CipherCtx, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for byte in data {
        if ctx.used == 16 {
            ctx.stream = ctx.iv;
            if let Some(aes) = &ctx.aes {
                aes.encrypt(&mut ctx.stream);
            }
            for i in (0..16).rev() {
                ctx.iv[i] = ctx.iv[i].wrapping_add(1);
                if ctx.iv[i] != 0 {
                    break;
                }
            }
            ctx.used = 0;
        }
        out.push(byte ^ ctx.stream[ctx.used]);
        ctx.used += 1;
    }
    out
}

fn aead_seal(ctx: &CipherCtx, data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), &'static str> {
    use aes_gcm::aead::AeadInPlace;
    let mut buf = data.to_vec();
    let tag = match ctx.mode {
        Mode::ChaCha => {
            let c = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&ctx.key)
                .map_err(|_| "bad key")?;
            c.encrypt_in_place_detached(
                chacha20poly1305::Nonce::from_slice(&ctx.nonce),
                &ctx.aad,
                &mut buf,
            )
            .map_err(|_| "encrypt failed")?
            .to_vec()
        }
        _ => {
            let nonce = aes_gcm::Nonce::from_slice(&ctx.nonce);
            match ctx.key.len() {
                16 => aes_gcm::Aes128Gcm::new_from_slice(&ctx.key)
                    .map_err(|_| "bad key")?
                    .encrypt_in_place_detached(nonce, &ctx.aad, &mut buf)
                    .map_err(|_| "encrypt failed")?
                    .to_vec(),
                24 => aes_gcm::AesGcm::<aes::Aes192, aes_gcm::aead::consts::U12>::new_from_slice(
                    &ctx.key,
                )
                .map_err(|_| "bad key")?
                .encrypt_in_place_detached(nonce, &ctx.aad, &mut buf)
                .map_err(|_| "encrypt failed")?
                .to_vec(),
                _ => aes_gcm::Aes256Gcm::new_from_slice(&ctx.key)
                    .map_err(|_| "bad key")?
                    .encrypt_in_place_detached(nonce, &ctx.aad, &mut buf)
                    .map_err(|_| "encrypt failed")?
                    .to_vec(),
            }
        }
    };
    Ok((buf, tag))
}

fn aead_open(ctx: &CipherCtx, data: &[u8]) -> Result<Vec<u8>, &'static str> {
    use aes_gcm::aead::AeadInPlace;
    let given = ctx
        .tag
        .clone()
        .ok_or("Unsupported state or unable to authenticate data")?;
    if ctx.mode == Mode::ChaCha {
        let c =
            chacha20poly1305::ChaCha20Poly1305::new_from_slice(&ctx.key).map_err(|_| "bad key")?;
        let mut buf = data.to_vec();
        if given.len() != 16 {
            return Err("Unsupported state or unable to authenticate data");
        }
        c.decrypt_in_place_detached(
            chacha20poly1305::Nonce::from_slice(&ctx.nonce),
            &ctx.aad,
            &mut buf,
            chacha20poly1305::Tag::from_slice(&given),
        )
        .map_err(|_| "Unsupported state or unable to authenticate data")?;
        return Ok(buf);
    }
    let plain = {
        let mut probe = CipherCtx {
            mode: ctx.mode,
            encrypt: true,
            key: ctx.key.clone(),
            aes: None,
            iv: ctx.iv,
            nonce: ctx.nonce.clone(),
            buf: Vec::new(),
            pad: false,
            aad: Vec::new(),
            tag: None,
            tag_len: 16,
            stream: [0u8; 16],
            used: 16,
        };
        probe.aad.clear();
        aead_seal(&probe, data)?.0
    };
    let (_, expected) = aead_seal(ctx, &plain)?;
    if given.is_empty() || given.len() > 16 || expected[..given.len()] != given[..] {
        return Err("Unsupported state or unable to authenticate data");
    }
    Ok(plain)
}

fn native_cipher_update(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let data = bytes_arg(vm, &arg(args, 1))?;
    let r = with_ctx(id, |ctx| match ctx.mode {
        Mode::Cbc | Mode::Ecb => block_process(ctx, &data, false),
        Mode::Ctr => Ok(ctr_process(ctx, &data)),
        Mode::Gcm | Mode::ChaCha => {
            ctx.buf.extend_from_slice(&data);
            Ok(Vec::new())
        }
    });
    match r {
        Some(Ok(out)) => Ok(out_bytes(vm, out)),
        Some(Err(m)) => Err(crypto_error(vm, "ERR_OSSL_BAD_DECRYPT", m)),
        None => Err(crypto_error(
            vm,
            "ERR_CRYPTO_INVALID_STATE",
            "Invalid state for operation update",
        )),
    }
}

fn native_cipher_final(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let r = with_ctx(id, |ctx| match ctx.mode {
        Mode::Cbc | Mode::Ecb => block_process(ctx, &[], true),
        Mode::Ctr => Ok(Vec::new()),
        Mode::Gcm | Mode::ChaCha => {
            let data = std::mem::take(&mut ctx.buf);
            if ctx.encrypt {
                aead_seal(ctx, &data).map(|(out, tag)| {
                    ctx.tag = Some(tag[..ctx.tag_len.min(16)].to_vec());
                    out
                })
            } else {
                aead_open(ctx, &data)
            }
        }
    });
    match r {
        Some(Ok(out)) => Ok(out_bytes(vm, out)),
        Some(Err(m)) => {
            let code = if m.starts_with("Unsupported state") {
                "ERR_CRYPTO_INVALID_STATE"
            } else {
                "ERR_OSSL_BAD_DECRYPT"
            };
            let e = crypto_error(vm, code, m);
            Err(e)
        }
        None => Err(crypto_error(
            vm,
            "ERR_CRYPTO_INVALID_STATE",
            "Invalid state for operation final",
        )),
    }
}

fn native_cipher_ctl(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let op = str_arg(vm, args, 1)?;
    let data = arg(args, 2);
    let bytes = match &data {
        Value::Bool(_) | Value::Undefined => Vec::new(),
        v => bytes_arg(vm, v)?,
    };
    let out = with_ctx(id, |ctx| match op.as_str() {
        "aad" => {
            ctx.aad.extend_from_slice(&bytes);
            None
        }
        "setTag" => {
            ctx.tag = Some(bytes.clone());
            None
        }
        "getTag" => ctx.tag.clone(),
        "padding" => {
            ctx.pad = data.truthy();
            None
        }
        _ => None,
    });
    if op == "free" {
        CIPHERS.with(|c| c.borrow_mut().remove(&id));
        return Ok(Value::Undefined);
    }
    match out {
        Some(Some(tag)) => Ok(out_bytes(vm, tag)),
        _ => Ok(Value::Undefined),
    }
}

pub fn install_natives(vm: &mut Vm, natives: &Rc<Obj>) {
    let list: [(&str, u32, crate::object::NativeFn); 16] = [
        ("keyImport", 5, native_key_import),
        ("keyImportJwk", 2, native_key_import_jwk),
        ("keyExport", 5, native_key_export),
        ("keyExportJwk", 2, native_key_export_jwk),
        ("keyGenerate", 4, native_key_generate),
        ("keyRaw", 3, native_key_raw),
        ("sign", 6, native_sign),
        ("verify", 7, native_verify),
        ("deriveShared", 2, native_derive),
        ("ecdh", 5, native_ecdh),
        ("rsa", 6, native_rsa),
        ("scrypt", 6, native_scrypt),
        ("cipherNew", 5, native_cipher_new),
        ("cipherUpdate", 2, native_cipher_update),
        ("cipherFinal", 1, native_cipher_final),
        ("cipherCtl", 3, native_cipher_ctl),
    ];
    for (name, arity, f) in list {
        method(vm, natives, name, arity, f);
    }
}
