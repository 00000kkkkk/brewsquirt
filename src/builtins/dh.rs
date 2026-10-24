use super::{arg, method};
use crate::object::Obj;
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};
use rand_core::{OsRng, RngCore};
use std::rc::Rc;

type R = Result<Value, Value>;

const GROUPS: [(&str, u64, &str); 8] = [
    (
        "modp1",
        0,
        "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a63a3620ffffffffffffffff",
    ),
    (
        "modp2",
        0,
        "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece65381ffffffffffffffff",
    ),
    (
        "modp5",
        200,
        "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca237327ffffffffffffffff",
    ),
    (
        "modp14",
        225,
        "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aacaa68ffffffffffffffff",
    ),
    (
        "modp15",
        275,
        "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aaac42dad33170d04507a33a85521abdf1cba64ecfb850458dbef0a8aea71575d060c7db3970f85a6e1e4c7abf5ae8cdb0933d71e8c94e04a25619dcee3d2261ad2ee6bf12ffa06d98a0864d87602733ec86a64521f2b18177b200cbbe117577a615d6c770988c0bad946e208e24fa074e5ab3143db5bfce0fd108e4b82d120a93ad2caffffffffffffffff",
    ),
    (
        "modp16",
        325,
        "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aaac42dad33170d04507a33a85521abdf1cba64ecfb850458dbef0a8aea71575d060c7db3970f85a6e1e4c7abf5ae8cdb0933d71e8c94e04a25619dcee3d2261ad2ee6bf12ffa06d98a0864d87602733ec86a64521f2b18177b200cbbe117577a615d6c770988c0bad946e208e24fa074e5ab3143db5bfce0fd108e4b82d120a92108011a723c12a787e6d788719a10bdba5b2699c327186af4e23c1a946834b6150bda2583e9ca2ad44ce8dbbbc2db04de8ef92e8efc141fbecaa6287c59474e6bc05d99b2964fa090c3a2233ba186515be7ed1f612970cee2d7afb81bdd762170481cd0069127d5b05aa993b4ea988d8fddc186ffb7dc90a6c08f4df435c934063199ffffffffffffffff",
    ),
    (
        "modp17",
        375,
        "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aaac42dad33170d04507a33a85521abdf1cba64ecfb850458dbef0a8aea71575d060c7db3970f85a6e1e4c7abf5ae8cdb0933d71e8c94e04a25619dcee3d2261ad2ee6bf12ffa06d98a0864d87602733ec86a64521f2b18177b200cbbe117577a615d6c770988c0bad946e208e24fa074e5ab3143db5bfce0fd108e4b82d120a92108011a723c12a787e6d788719a10bdba5b2699c327186af4e23c1a946834b6150bda2583e9ca2ad44ce8dbbbc2db04de8ef92e8efc141fbecaa6287c59474e6bc05d99b2964fa090c3a2233ba186515be7ed1f612970cee2d7afb81bdd762170481cd0069127d5b05aa993b4ea988d8fddc186ffb7dc90a6c08f4df435c93402849236c3fab4d27c7026c1d4dcb2602646dec9751e763dba37bdf8ff9406ad9e530ee5db382f413001aeb06a53ed9027d831179727b0865a8918da3edbebcf9b14ed44ce6cbaced4bb1bdb7f1447e6cc254b332051512bd7af426fb8f401378cd2bf5983ca01c64b92ecf032ea15d1721d03f482d7ce6e74fef6d55e702f46980c82b5a84031900b1c9e59e7c97fbec7e8f323a97a7e36cc88be0f1d45b7ff585ac54bd407b22b4154aacc8f6d7ebf48e1d814cc5ed20f8037e0a79715eef29be32806a1d58bb7c5da76f550aa3d8a1fbff0eb19ccb1a313d55cda56c9ec2ef29632387fe8d76e3c0468043e8f663f4860ee12bf2d5b0b7474d6e694f91e6dcc4024ffffffffffffffff",
    ),
    (
        "modp18",
        400,
        "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aaac42dad33170d04507a33a85521abdf1cba64ecfb850458dbef0a8aea71575d060c7db3970f85a6e1e4c7abf5ae8cdb0933d71e8c94e04a25619dcee3d2261ad2ee6bf12ffa06d98a0864d87602733ec86a64521f2b18177b200cbbe117577a615d6c770988c0bad946e208e24fa074e5ab3143db5bfce0fd108e4b82d120a92108011a723c12a787e6d788719a10bdba5b2699c327186af4e23c1a946834b6150bda2583e9ca2ad44ce8dbbbc2db04de8ef92e8efc141fbecaa6287c59474e6bc05d99b2964fa090c3a2233ba186515be7ed1f612970cee2d7afb81bdd762170481cd0069127d5b05aa993b4ea988d8fddc186ffb7dc90a6c08f4df435c93402849236c3fab4d27c7026c1d4dcb2602646dec9751e763dba37bdf8ff9406ad9e530ee5db382f413001aeb06a53ed9027d831179727b0865a8918da3edbebcf9b14ed44ce6cbaced4bb1bdb7f1447e6cc254b332051512bd7af426fb8f401378cd2bf5983ca01c64b92ecf032ea15d1721d03f482d7ce6e74fef6d55e702f46980c82b5a84031900b1c9e59e7c97fbec7e8f323a97a7e36cc88be0f1d45b7ff585ac54bd407b22b4154aacc8f6d7ebf48e1d814cc5ed20f8037e0a79715eef29be32806a1d58bb7c5da76f550aa3d8a1fbff0eb19ccb1a313d55cda56c9ec2ef29632387fe8d76e3c0468043e8f663f4860ee12bf2d5b0b7474d6e694f91e6dbe115974a3926f12fee5e438777cb6a932df8cd8bec4d073b931ba3bc832b68d9dd300741fa7bf8afc47ed2576f6936ba424663aab639c5ae4f5683423b4742bf1c978238f16cbe39d652de3fdb8befc848ad922222e04a4037c0713eb57a81a23f0c73473fc646cea306b4bcbc8862f8385ddfa9d4b7fa2c087e879683303ed5bdd3a062b3cf5b3a278a66d2a13f83f44f82ddf310ee074ab6a364597e899a0255dc164f31cc50846851df9ab48195ded7ea1b1d510bd7ee74d73faf36bc31ecfa268359046f4eb879f924009438b481c6cd7889a002ed5ee382bc9190da6fc026e479558e4475677e9aa9e3050e2765694dfc81f56e880b96e7160c980dd98edd3dfffffffffffffffff",
    ),
];

fn parse_hex(hex: &str) -> BigUint {
    BigUint::parse_bytes(hex.as_bytes(), 16).unwrap_or_default()
}

pub fn group_prime(name: &str) -> Option<BigUint> {
    GROUPS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, hex)| parse_hex(hex))
}

fn known_group(p: &BigUint) -> Option<u64> {
    GROUPS
        .iter()
        .find(|(_, _, hex)| hex.len() as u64 * 4 == p.bits() && parse_hex(hex) == *p)
        .map(|(_, len, _)| *len)
}

fn key_bits(p: &BigUint, g: &BigUint) -> u64 {
    if *g != BigUint::from(2u32) {
        return 0;
    }
    known_group(p).unwrap_or(0)
}

fn random_bits(bits: u64) -> BigUint {
    let mut bytes = vec![0u8; bits.div_ceil(8) as usize];
    OsRng.fill_bytes(&mut bytes);
    let extra = bytes.len() as u64 * 8 - bits;
    if let Some(b) = bytes.first_mut() {
        *b &= 0xffu8 >> extra;
    }
    BigUint::from_bytes_be(&bytes)
}

fn random_below(n: &BigUint) -> BigUint {
    loop {
        let r = random_bits(n.bits());
        if r < *n {
            return r;
        }
    }
}

fn small_primes(limit: usize) -> Vec<u32> {
    let mut composite = vec![false; limit];
    let mut out = Vec::new();
    for i in 2..limit {
        if composite[i] {
            continue;
        }
        out.push(i as u32);
        let mut j = i * i;
        while j < limit {
            composite[j] = true;
            j += i;
        }
    }
    out
}

fn witness(n: &BigUint, n1: &BigUint, d: &BigUint, s: u64, base: &BigUint) -> bool {
    let mut x = base.modpow(d, n);
    if x.is_one() || x == *n1 {
        return true;
    }
    for _ in 1..s {
        x = &x * &x % n;
        if x == *n1 {
            return true;
        }
        if x.is_one() {
            return false;
        }
    }
    false
}

fn probable_prime(n: &BigUint, rounds: usize) -> bool {
    let two = BigUint::from(2u32);
    if *n < two {
        return false;
    }
    for sp in [2u32, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47] {
        let sp = BigUint::from(sp);
        if *n == sp {
            return true;
        }
        if (n % &sp).is_zero() {
            return false;
        }
    }
    let n1 = n - 1u32;
    let s = n1.trailing_zeros().unwrap_or(0);
    let d = &n1 >> s;
    if !witness(n, &n1, &d, s, &two) {
        return false;
    }
    let span = n - 3u32;
    (1..rounds).all(|_| witness(n, &n1, &d, s, &(random_below(&span) + 2u32)))
}

fn rounds_for(bits: u64) -> usize {
    match bits {
        b if b >= 3747 => 3,
        b if b >= 1345 => 4,
        b if b >= 476 => 5,
        b if b >= 400 => 6,
        b if b >= 347 => 7,
        b if b >= 308 => 8,
        b if b >= 55 => 27,
        _ => 34,
    }
}

fn mod_inverse(a: u64, m: u64) -> u64 {
    let (mut result, mut base, mut exp) = (1u64, a % m, m - 2);
    while exp > 0 {
        if exp & 1 == 1 {
            result = result * base % m;
        }
        base = base * base % m;
        exp >>= 1;
    }
    result
}

pub fn safe_prime(bits: u64, generator: u32) -> BigUint {
    let (add, rem): (u64, u64) = match generator {
        2 => (24, 23),
        5 => (60, 59),
        _ => (12, 11),
    };
    let primes = small_primes(1 << 18);
    let rounds = rounds_for(bits);
    const WINDOW: usize = 1 << 14;
    loop {
        let mut base = random_bits(bits);
        base.set_bit(bits - 1, true);
        base.set_bit(bits - 2, true);
        let r = (&base % add).to_u64().unwrap_or(0);
        base = base - r + rem;
        let mut bad = vec![false; WINDOW];
        for &sp in &primes {
            let s = sp as u64;
            if add % s == 0 {
                continue;
            }
            let residue = (&base % s).to_u64().unwrap_or(0);
            let inv = mod_inverse(add % s, s);
            for target in [0u64, 1] {
                let mut k = ((target + s - residue) % s * inv % s) as usize;
                while k < WINDOW {
                    bad[k] = true;
                    k += s as usize;
                }
            }
        }
        for (k, skip) in bad.iter().enumerate() {
            if *skip {
                continue;
            }
            let candidate = &base + k as u64 * add;
            if candidate.bits() != bits {
                break;
            }
            let q = &candidate >> 1u32;
            if probable_prime(&q, 1)
                && probable_prime(&candidate, 1)
                && probable_prime(&q, rounds)
                && probable_prime(&candidate, rounds)
            {
                return candidate;
            }
        }
    }
}

pub fn check(p: &BigUint, g: &BigUint) -> u32 {
    if key_bits(p, g) > 0 {
        return 0;
    }
    if p.bits() > 32768 {
        return 0x101;
    }
    let mut flags = 0;
    if !p.bit(0) {
        flags |= 0x01;
    }
    let p1 = if p.is_zero() {
        BigUint::zero()
    } else {
        p - 1u32
    };
    if *g <= BigUint::one() || *g >= p1 {
        flags |= 0x08;
    }
    if p.bits() < 512 {
        flags |= 0x80;
    }
    if p.bits() > 10000 {
        flags |= 0x100;
    }
    if known_group(p).is_some() {
        return flags;
    }
    if !probable_prime(p, 10) {
        flags |= 0x01;
    } else if !probable_prime(&(p >> 1u32), 10) {
        flags |= 0x02;
    }
    flags
}

pub fn private_key(p: &BigUint, g: &BigUint) -> Option<BigUint> {
    if p.bits() < 512 {
        return None;
    }
    let n = key_bits(p, g);
    if n > 0 {
        loop {
            let x = random_bits(n);
            if !x.is_zero() {
                return Some(x);
            }
        }
    }
    let l = p.bits() - 1;
    let mut x = random_bits(l);
    x.set_bit(l - 1, true);
    Some(x)
}

pub fn padded(value: &BigUint, p: &BigUint) -> Vec<u8> {
    let bytes = value.to_bytes_be();
    let size = p.bits().div_ceil(8) as usize;
    if bytes.len() >= size {
        return bytes;
    }
    let mut out = vec![0u8; size - bytes.len()];
    out.extend(bytes);
    out
}

pub enum SecretError {
    Small,
    Large,
    Failed,
}

pub fn secret(p: &BigUint, x: Option<&BigUint>, y: &BigUint) -> Result<Vec<u8>, SecretError> {
    if *y <= BigUint::one() {
        return Err(SecretError::Small);
    }
    if p.is_zero() || *y >= p - 1u32 {
        return Err(SecretError::Large);
    }
    let Some(x) = x else {
        return Err(SecretError::Failed);
    };
    if p.bits() < 512 {
        return Err(SecretError::Failed);
    }
    Ok(padded(&y.modpow(x, p), p))
}

fn crypto_error(vm: &mut Vm, kind: ErrorKind, code: &str, msg: &str) -> Value {
    let e = vm.make_error(kind, msg);
    if let Value::Object(o) = &e {
        o.set_own("code", code.into());
    }
    e
}

fn big_arg(v: &Value) -> Option<BigUint> {
    super::typed::buffer_source_bytes(v).map(|b| BigUint::from_bytes_be(&b))
}

fn out_bytes(vm: &Vm, bytes: Vec<u8>) -> Value {
    let len = bytes.len();
    let buf = super::typed::new_buffer(vm, bytes);
    super::typed::new_typed(vm, crate::object::TypedKind::Uint8, buf, 0, len)
}

fn minimal(n: &BigUint) -> Vec<u8> {
    if n.is_zero() {
        Vec::new()
    } else {
        n.to_bytes_be()
    }
}

fn native_group(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let name = match arg(args, 0) {
        Value::Str(s) => s.to_string(),
        _ => return Ok(Value::Undefined),
    };
    Ok(match group_prime(&name) {
        Some(p) => out_bytes(vm, p.to_bytes_be()),
        None => Value::Undefined,
    })
}

fn native_prime(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let bits = vm.to_number(&arg(args, 0))?;
    let generator = vm.to_number(&arg(args, 1))?;
    if !(512.0..=10000.0).contains(&bits) {
        return Ok(Value::Undefined);
    }
    let p = safe_prime(bits as u64, generator as u32);
    Ok(out_bytes(vm, p.to_bytes_be()))
}

fn native_check(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = big_arg(&arg(args, 0)).unwrap_or_default();
    let g = big_arg(&arg(args, 1)).unwrap_or_default();
    Ok(Value::Number(check(&p, &g) as f64))
}

fn native_keys(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = big_arg(&arg(args, 0)).unwrap_or_default();
    let g = big_arg(&arg(args, 1)).unwrap_or_default();
    let x = match big_arg(&arg(args, 2)) {
        Some(x) if p.bits() >= 512 => Some(x),
        _ => private_key(&p, &g),
    };
    let Some(x) = x else {
        return Err(crypto_error(
            vm,
            ErrorKind::Error,
            "ERR_CRYPTO_OPERATION_FAILED",
            "Key generation failed",
        ));
    };
    let y = g.modpow(&x, &p);
    let private = out_bytes(vm, minimal(&x));
    let public = out_bytes(vm, minimal(&y));
    Ok(vm.new_array(vec![private, public]))
}

fn native_compute(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = big_arg(&arg(args, 0)).unwrap_or_default();
    let x = big_arg(&arg(args, 1));
    let y = big_arg(&arg(args, 2)).unwrap_or_default();
    match secret(&p, x.as_ref(), &y) {
        Ok(s) => Ok(out_bytes(vm, s)),
        Err(SecretError::Small) => Err(crypto_error(
            vm,
            ErrorKind::RangeError,
            "ERR_CRYPTO_INVALID_KEYLEN",
            "Supplied key is too small",
        )),
        Err(SecretError::Large) => Err(crypto_error(
            vm,
            ErrorKind::RangeError,
            "ERR_CRYPTO_INVALID_KEYLEN",
            "Supplied key is too large",
        )),
        Err(SecretError::Failed) => Err(crypto_error(
            vm,
            ErrorKind::Error,
            "ERR_CRYPTO_OPERATION_FAILED",
            "Failed to compute shared secret",
        )),
    }
}

pub fn install_natives(vm: &mut Vm, natives: &Rc<Obj>) {
    method(vm, natives, "dhGroup", 1, native_group);
    method(vm, natives, "dhPrime", 2, native_prime);
    method(vm, natives, "dhCheck", 2, native_check);
    method(vm, natives, "dhKeys", 3, native_keys);
    method(vm, natives, "dhCompute", 3, native_compute);
}
