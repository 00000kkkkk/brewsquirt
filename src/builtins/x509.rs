use super::{arg, method};
use crate::object::Obj;
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::rc::Rc;
use x509_cert::Certificate;
use x509_cert::der::{Any, Decode, Encode, Tag, Tagged};
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{
    AuthorityInfoAccessSyntax, BasicConstraints, ExtendedKeyUsage, SubjectAltName,
};
use x509_cert::name::Name;

type R = Result<Value, Value>;

fn attr_name(oid: &str) -> String {
    match oid {
        "2.5.4.3" => "CN",
        "2.5.4.4" => "SN",
        "2.5.4.5" => "serialNumber",
        "2.5.4.6" => "C",
        "2.5.4.7" => "L",
        "2.5.4.8" => "ST",
        "2.5.4.9" => "street",
        "2.5.4.10" => "O",
        "2.5.4.11" => "OU",
        "2.5.4.12" => "title",
        "2.5.4.17" => "postalCode",
        "2.5.4.42" => "GN",
        "2.5.4.43" => "initials",
        "2.5.4.46" => "dnQualifier",
        "2.5.4.65" => "pseudonym",
        "1.2.840.113549.1.9.1" => "emailAddress",
        "0.9.2342.19200300.100.1.1" => "UID",
        "0.9.2342.19200300.100.1.25" => "DC",
        "1.3.6.1.4.1.311.60.2.1.3" => "jurisdictionC",
        "2.5.4.15" => "businessCategory",
        other => other,
    }
    .to_string()
}

fn attr_value(any: &Any) -> String {
    let bytes = any.value();
    match any.tag() {
        Tag::BmpString => {
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        Tag::TeletexString => bytes.iter().map(|b| *b as char).collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn name_value(vm: &mut Vm, name: &Name) -> Value {
    let mut rdns = Vec::new();
    for rdn in name.0.iter() {
        let mut parts = Vec::new();
        for atv in rdn.0.iter() {
            let pair = vec![
                attr_name(&atv.oid.to_string()).into(),
                attr_value(&atv.value).into(),
            ];
            parts.push(vm.new_array(pair));
        }
        rdns.push(vm.new_array(parts));
    }
    vm.new_array(rdns)
}

fn ip_text(bytes: &[u8]) -> String {
    match bytes.len() {
        4 => bytes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join("."),
        16 => bytes
            .chunks_exact(2)
            .map(|c| format!("{:X}", u16::from_be_bytes([c[0], c[1]])))
            .collect::<Vec<_>>()
            .join(":"),
        _ => bytes.iter().map(|b| format!("{b:02X}")).collect(),
    }
}

fn general_name(gn: &GeneralName) -> Option<(&'static str, String)> {
    Some(match gn {
        GeneralName::DnsName(s) => ("DNS", s.as_str().to_string()),
        GeneralName::Rfc822Name(s) => ("email", s.as_str().to_string()),
        GeneralName::UniformResourceIdentifier(s) => ("URI", s.as_str().to_string()),
        GeneralName::IpAddress(o) => ("IP Address", ip_text(o.as_bytes())),
        GeneralName::RegisteredId(oid) => ("Registered ID", oid.to_string()),
        _ => return None,
    })
}

fn pairs(vm: &mut Vm, items: Vec<(&str, String)>) -> Value {
    let rows = items
        .into_iter()
        .map(|(k, v)| vm.new_array(vec![k.into(), v.into()]))
        .collect();
    vm.new_array(rows)
}

fn time_ms(t: x509_cert::time::Time) -> Value {
    Value::Number(t.to_unix_duration().as_millis() as f64)
}

fn parse(vm: &mut Vm, der: &[u8]) -> R {
    let cert = Certificate::from_der(der).map_err(|e| {
        vm.make_error(
            ErrorKind::Error,
            format!("error:0680007B:asn1 encoding routines::header too long ({e})"),
        )
    })?;
    let tbs = &cert.tbs_certificate;
    let o: Rc<Obj> = vm.new_object();
    let subject = name_value(vm, &tbs.subject);
    o.set_own("subject", subject);
    let issuer = name_value(vm, &tbs.issuer);
    o.set_own("issuer", issuer);
    o.set_own("notBefore", time_ms(tbs.validity.not_before));
    o.set_own("notAfter", time_ms(tbs.validity.not_after));
    let serial = tbs.serial_number.as_bytes();
    let first = serial
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(serial.len().saturating_sub(1));
    let serial: String = serial[first..].iter().map(|b| format!("{b:02X}")).collect();
    o.set_own("serialNumber", serial.into());
    let mut san = Vec::new();
    let mut ca = false;
    let mut eku = Vec::new();
    let mut access = Vec::new();
    for ext in tbs.extensions.iter().flatten() {
        let body = ext.extn_value.as_bytes();
        match ext.extn_id.to_string().as_str() {
            "2.5.29.17" => {
                if let Ok(names) = SubjectAltName::from_der(body) {
                    san.extend(names.0.iter().filter_map(general_name));
                }
            }
            "2.5.29.19" => {
                if let Ok(bc) = BasicConstraints::from_der(body) {
                    ca = bc.ca;
                }
            }
            "2.5.29.37" => {
                if let Ok(usage) = ExtendedKeyUsage::from_der(body) {
                    eku.extend(usage.0.iter().map(|oid| oid.to_string()));
                }
            }
            "1.3.6.1.5.5.7.1.1" => {
                if let Ok(aia) = AuthorityInfoAccessSyntax::from_der(body) {
                    for d in aia.0.iter() {
                        let method = match d.access_method.to_string().as_str() {
                            "1.3.6.1.5.5.7.48.1" => "OCSP".to_string(),
                            "1.3.6.1.5.5.7.48.2" => "CA Issuers".to_string(),
                            other => other.to_string(),
                        };
                        if let Some((kind, value)) = general_name(&d.access_location) {
                            access.push((format!("{method} - {kind}"), value));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let san = pairs(vm, san);
    o.set_own("subjectAltName", san);
    o.set_own("ca", Value::Bool(ca));
    let eku = eku.into_iter().map(Value::from).collect();
    let eku = vm.new_array(eku);
    o.set_own("extKeyUsage", eku);
    let rows = access
        .into_iter()
        .map(|(k, v)| vm.new_array(vec![k.into(), v.into()]))
        .collect();
    let access = vm.new_array(rows);
    o.set_own("infoAccess", access);
    let encode = |vm: &mut Vm, r: Result<Vec<u8>, x509_cert::der::Error>| -> R {
        r.map(|b| super::typed::new_uint8(vm, b))
            .map_err(|e| vm.make_error(ErrorKind::Error, e.to_string()))
    };
    let spki = encode(vm, tbs.subject_public_key_info.to_der())?;
    o.set_own("spki", spki);
    let tbs_der = encode(vm, tbs.to_der())?;
    o.set_own("tbs", tbs_der);
    let subject_der = encode(vm, tbs.subject.to_der())?;
    o.set_own("subjectDer", subject_der);
    let issuer_der = encode(vm, tbs.issuer.to_der())?;
    o.set_own("issuerDer", issuer_der);
    o.set_own(
        "signatureAlgorithm",
        cert.signature_algorithm.oid.to_string().into(),
    );
    let sig = super::typed::new_uint8(vm, cert.signature.raw_bytes().to_vec());
    o.set_own("signature", sig);
    o.set_own(
        "keyAlgorithm",
        tbs.subject_public_key_info.algorithm.oid.to_string().into(),
    );
    Ok(Value::Object(o))
}

fn native_parse(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let bytes = super::typed::buffer_source_bytes(&arg(args, 0)).unwrap_or_default();
    parse(vm, &bytes)
}

pub fn install_natives(vm: &mut Vm, natives: &Rc<Obj>) {
    method(vm, natives, "x509Parse", 1, native_parse);
}
