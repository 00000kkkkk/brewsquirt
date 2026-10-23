#[cfg(unix)]
macro_rules! code {
    ($name:ident, $fallback:expr) => {
        -libc::$name
    };
}

#[cfg(not(unix))]
macro_rules! code {
    ($name:ident, $fallback:expr) => {
        $fallback
    };
}

#[cfg(any(target_os = "linux", target_os = "android"))]
macro_rules! code_linux {
    ($name:ident, $fallback:expr) => {
        -libc::$name
    };
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
macro_rules! code_linux {
    ($name:ident, $fallback:expr) => {
        $fallback
    };
}

#[cfg(target_vendor = "apple")]
macro_rules! code_apple {
    ($name:ident, $fallback:expr) => {
        -libc::$name
    };
}

#[cfg(not(target_vendor = "apple"))]
macro_rules! code_apple {
    ($name:ident, $fallback:expr) => {
        $fallback
    };
}

pub static UV_ERRORS: &[(&str, &str, i32)] = &[
    ("E2BIG", "argument list too long", code!(E2BIG, -4093)),
    ("EACCES", "permission denied", code!(EACCES, -4092)),
    (
        "EADDRINUSE",
        "address already in use",
        code!(EADDRINUSE, -4091),
    ),
    (
        "EADDRNOTAVAIL",
        "address not available",
        code!(EADDRNOTAVAIL, -4090),
    ),
    (
        "EAFNOSUPPORT",
        "address family not supported",
        code!(EAFNOSUPPORT, -4089),
    ),
    (
        "EAGAIN",
        "resource temporarily unavailable",
        code!(EAGAIN, -4088),
    ),
    ("EAI_ADDRFAMILY", "address family not supported", -3000),
    ("EAI_AGAIN", "temporary failure", -3001),
    ("EAI_BADFLAGS", "bad ai_flags value", -3002),
    ("EAI_BADHINTS", "invalid value for hints", -3013),
    ("EAI_CANCELED", "request canceled", -3003),
    ("EAI_FAIL", "permanent failure", -3004),
    ("EAI_FAMILY", "ai_family not supported", -3005),
    ("EAI_MEMORY", "out of memory", -3006),
    ("EAI_NODATA", "no address", -3007),
    ("EAI_NONAME", "unknown node or service", -3008),
    ("EAI_OVERFLOW", "argument buffer overflow", -3009),
    ("EAI_PROTOCOL", "resolved protocol is unknown", -3014),
    (
        "EAI_SERVICE",
        "service not available for socket type",
        -3010,
    ),
    ("EAI_SOCKTYPE", "socket type not supported", -3011),
    (
        "EALREADY",
        "connection already in progress",
        code!(EALREADY, -4084),
    ),
    ("EBADF", "bad file descriptor", code!(EBADF, -4083)),
    ("EBUSY", "resource busy or locked", code!(EBUSY, -4082)),
    ("ECANCELED", "operation canceled", code!(ECANCELED, -4081)),
    ("ECHARSET", "invalid Unicode character", -4080),
    (
        "ECONNABORTED",
        "software caused connection abort",
        code!(ECONNABORTED, -4079),
    ),
    (
        "ECONNREFUSED",
        "connection refused",
        code!(ECONNREFUSED, -4078),
    ),
    (
        "ECONNRESET",
        "connection reset by peer",
        code!(ECONNRESET, -4077),
    ),
    (
        "EDESTADDRREQ",
        "destination address required",
        code!(EDESTADDRREQ, -4076),
    ),
    ("EEXIST", "file already exists", code!(EEXIST, -4075)),
    (
        "EFAULT",
        "bad address in system call argument",
        code!(EFAULT, -4074),
    ),
    ("EFBIG", "file too large", code!(EFBIG, -4036)),
    (
        "EHOSTUNREACH",
        "host is unreachable",
        code!(EHOSTUNREACH, -4073),
    ),
    ("EINTR", "interrupted system call", code!(EINTR, -4072)),
    ("EINVAL", "invalid argument", code!(EINVAL, -4071)),
    ("EIO", "i/o error", code!(EIO, -4070)),
    (
        "EISCONN",
        "socket is already connected",
        code!(EISCONN, -4069),
    ),
    (
        "EISDIR",
        "illegal operation on a directory",
        code!(EISDIR, -4068),
    ),
    (
        "ELOOP",
        "too many symbolic links encountered",
        code!(ELOOP, -4067),
    ),
    ("EMFILE", "too many open files", code!(EMFILE, -4066)),
    ("EMSGSIZE", "message too long", code!(EMSGSIZE, -4065)),
    ("ENAMETOOLONG", "name too long", code!(ENAMETOOLONG, -4064)),
    ("ENETDOWN", "network is down", code!(ENETDOWN, -4063)),
    (
        "ENETUNREACH",
        "network is unreachable",
        code!(ENETUNREACH, -4062),
    ),
    ("ENFILE", "file table overflow", code!(ENFILE, -4061)),
    (
        "ENOBUFS",
        "no buffer space available",
        code!(ENOBUFS, -4060),
    ),
    ("ENODEV", "no such device", code!(ENODEV, -4059)),
    ("ENOENT", "no such file or directory", code!(ENOENT, -4058)),
    ("ENOMEM", "not enough memory", code!(ENOMEM, -4057)),
    (
        "ENONET",
        "machine is not on the network",
        code_linux!(ENONET, -4056),
    ),
    (
        "ENOPROTOOPT",
        "protocol not available",
        code!(ENOPROTOOPT, -4035),
    ),
    ("ENOSPC", "no space left on device", code!(ENOSPC, -4055)),
    ("ENOSYS", "function not implemented", code!(ENOSYS, -4054)),
    (
        "ENOTCONN",
        "socket is not connected",
        code!(ENOTCONN, -4053),
    ),
    ("ENOTDIR", "not a directory", code!(ENOTDIR, -4052)),
    ("ENOTEMPTY", "directory not empty", code!(ENOTEMPTY, -4051)),
    (
        "ENOTSOCK",
        "socket operation on non-socket",
        code!(ENOTSOCK, -4050),
    ),
    (
        "ENOTSUP",
        "operation not supported on socket",
        code!(ENOTSUP, -4049),
    ),
    (
        "EOVERFLOW",
        "value too large for defined data type",
        code!(EOVERFLOW, -4026),
    ),
    ("EPERM", "operation not permitted", code!(EPERM, -4048)),
    ("EPIPE", "broken pipe", code!(EPIPE, -4047)),
    ("EPROTO", "protocol error", code!(EPROTO, -4046)),
    (
        "EPROTONOSUPPORT",
        "protocol not supported",
        code!(EPROTONOSUPPORT, -4045),
    ),
    (
        "EPROTOTYPE",
        "protocol wrong type for socket",
        code!(EPROTOTYPE, -4044),
    ),
    ("ERANGE", "result too large", code!(ERANGE, -4034)),
    ("EROFS", "read-only file system", code!(EROFS, -4043)),
    (
        "ESHUTDOWN",
        "cannot send after transport endpoint shutdown",
        code!(ESHUTDOWN, -4042),
    ),
    ("ESPIPE", "invalid seek", code!(ESPIPE, -4041)),
    ("ESRCH", "no such process", code!(ESRCH, -4040)),
    ("ETIMEDOUT", "connection timed out", code!(ETIMEDOUT, -4039)),
    ("ETXTBSY", "text file is busy", code!(ETXTBSY, -4038)),
    (
        "EXDEV",
        "cross-device link not permitted",
        code!(EXDEV, -4037),
    ),
    ("UNKNOWN", "unknown error", -4094),
    ("EOF", "end of file", -4095),
    ("ENXIO", "no such device or address", code!(ENXIO, -4033)),
    ("EMLINK", "too many links", code!(EMLINK, -4032)),
    ("EHOSTDOWN", "host is down", code!(EHOSTDOWN, -4031)),
    (
        "EREMOTEIO",
        "remote I/O error",
        code_linux!(EREMOTEIO, -4030),
    ),
    (
        "ENOTTY",
        "inappropriate ioctl for device",
        code!(ENOTTY, -4029),
    ),
    (
        "EFTYPE",
        "inappropriate file type or format",
        code_apple!(EFTYPE, -4028),
    ),
    ("EILSEQ", "illegal byte sequence", code!(EILSEQ, -4027)),
    (
        "ESOCKTNOSUPPORT",
        "socket type not supported",
        code!(ESOCKTNOSUPPORT, -4025),
    ),
    ("ENODATA", "no data available", code!(ENODATA, -4024)),
    (
        "EUNATCH",
        "protocol driver not attached",
        code_linux!(EUNATCH, -4023),
    ),
    ("ENOEXEC", "exec format error", code!(ENOEXEC, -4022)),
];

const EIO: (&str, &str, i32) = ("EIO", "i/o error", code!(EIO, -4070));

pub fn by_name(name: &str) -> (&'static str, &'static str, i32) {
    UV_ERRORS
        .iter()
        .find(|e| e.0 == name)
        .copied()
        .unwrap_or(EIO)
}

pub fn by_code(code: i32) -> Option<(&'static str, &'static str, i32)> {
    UV_ERRORS.iter().find(|e| e.2 == code).copied()
}
