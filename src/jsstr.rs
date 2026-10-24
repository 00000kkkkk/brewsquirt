use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::borrow::{Borrow, Cow};
use std::cell::Cell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::mem::{align_of, size_of};
use std::ops::Deref;
use std::ptr::NonNull;

#[repr(C)]
struct Header {
    rc: Cell<usize>,
    len: usize,
}

const ASCII: usize = 1 << (usize::BITS - 1);

pub struct JsStr {
    ptr: NonNull<Header>,
    _owned: PhantomData<*const u8>,
}

impl JsStr {
    fn layout(len: usize) -> Layout {
        match Layout::from_size_align(size_of::<Header>() + len, align_of::<Header>()) {
            Ok(l) => l,
            Err(_) => handle_alloc_error(Layout::new::<Header>()),
        }
    }

    pub fn new(s: &str) -> JsStr {
        let len = s.len();
        let layout = Self::layout(len);
        unsafe {
            let p = alloc(layout) as *mut Header;
            let Some(ptr) = NonNull::new(p) else {
                handle_alloc_error(layout)
            };
            p.write(Header {
                rc: Cell::new(1),
                len: if s.is_ascii() { len | ASCII } else { len },
            });
            std::ptr::copy_nonoverlapping(s.as_ptr(), (p as *mut u8).add(size_of::<Header>()), len);
            JsStr {
                ptr,
                _owned: PhantomData,
            }
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        unsafe {
            let h = self.ptr.as_ptr();
            let data = (h as *const u8).add(size_of::<Header>());
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(data, (*h).len & !ASCII))
        }
    }

    #[inline]
    pub fn is_ascii(&self) -> bool {
        unsafe { (*self.ptr.as_ptr()).len & ASCII != 0 }
    }

    #[inline]
    pub fn ptr_eq(a: &JsStr, b: &JsStr) -> bool {
        a.ptr == b.ptr
    }

    #[inline]
    pub fn as_ptr(s: &JsStr) -> *const u8 {
        s.ptr.as_ptr() as *const u8
    }

    #[inline]
    pub fn strong_count(s: &JsStr) -> usize {
        unsafe { (*s.ptr.as_ptr()).rc.get() }
    }
}

impl Clone for JsStr {
    #[inline]
    fn clone(&self) -> JsStr {
        unsafe {
            let rc = &(*self.ptr.as_ptr()).rc;
            let n = rc.get().wrapping_add(1);
            if n == 0 {
                std::process::abort();
            }
            rc.set(n);
        }
        JsStr {
            ptr: self.ptr,
            _owned: PhantomData,
        }
    }
}

impl Drop for JsStr {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            let h = self.ptr.as_ptr();
            let n = (*h).rc.get() - 1;
            if n == 0 {
                let len = (*h).len & !ASCII;
                dealloc(h as *mut u8, Self::layout(len));
            } else {
                (*h).rc.set(n);
            }
        }
    }
}

impl Deref for JsStr {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for JsStr {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for JsStr {
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}

impl AsRef<std::ffi::OsStr> for JsStr {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.as_str().as_ref()
    }
}

impl AsRef<std::path::Path> for JsStr {
    fn as_ref(&self) -> &std::path::Path {
        self.as_str().as_ref()
    }
}

impl Borrow<str> for JsStr {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Default for JsStr {
    fn default() -> JsStr {
        JsStr::new("")
    }
}

impl PartialEq for JsStr {
    #[inline]
    fn eq(&self, other: &JsStr) -> bool {
        self.ptr == other.ptr || self.as_str() == other.as_str()
    }
}

impl Eq for JsStr {}

impl PartialEq<str> for JsStr {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for JsStr {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialOrd for JsStr {
    fn partial_cmp(&self, other: &JsStr) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JsStr {
    fn cmp(&self, other: &JsStr) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for JsStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl fmt::Display for JsStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl fmt::Debug for JsStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl From<&str> for JsStr {
    #[inline]
    fn from(s: &str) -> JsStr {
        JsStr::new(s)
    }
}

impl From<&mut str> for JsStr {
    fn from(s: &mut str) -> JsStr {
        JsStr::new(s)
    }
}

impl From<String> for JsStr {
    #[inline]
    fn from(s: String) -> JsStr {
        JsStr::new(&s)
    }
}

impl From<&String> for JsStr {
    fn from(s: &String) -> JsStr {
        JsStr::new(s)
    }
}

impl From<Box<str>> for JsStr {
    fn from(s: Box<str>) -> JsStr {
        JsStr::new(&s)
    }
}

impl From<Cow<'_, str>> for JsStr {
    fn from(s: Cow<'_, str>) -> JsStr {
        JsStr::new(&s)
    }
}

impl From<char> for JsStr {
    fn from(c: char) -> JsStr {
        let mut buf = [0u8; 4];
        JsStr::new(c.encode_utf8(&mut buf))
    }
}

impl From<&JsStr> for JsStr {
    fn from(s: &JsStr) -> JsStr {
        s.clone()
    }
}

impl From<JsStr> for String {
    fn from(s: JsStr) -> String {
        s.as_str().to_string()
    }
}

impl From<&JsStr> for String {
    fn from(s: &JsStr) -> String {
        s.as_str().to_string()
    }
}

impl From<JsStr> for std::path::PathBuf {
    fn from(s: JsStr) -> std::path::PathBuf {
        std::path::PathBuf::from(s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::JsStr;

    #[test]
    fn round_trips_and_counts() {
        let a = JsStr::from("héllo");
        let b = a.clone();
        assert_eq!(&*a, "héllo");
        assert!(JsStr::ptr_eq(&a, &b));
        assert_eq!(JsStr::strong_count(&a), 2);
        drop(b);
        assert_eq!(JsStr::strong_count(&a), 1);
        assert_eq!(JsStr::from(String::new()).len(), 0);
        assert_eq!(std::mem::size_of::<JsStr>(), 8);
    }
}
