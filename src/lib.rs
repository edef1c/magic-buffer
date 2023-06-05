use std::{
    error::Error,
    fmt::{Display, Formatter},
    ops::{
        Deref, DerefMut, Index, IndexMut, Range, RangeFrom, RangeFull, RangeTo, RangeToInclusive,
    },
    ptr::{slice_from_raw_parts, slice_from_raw_parts_mut},
};

#[cfg(target_family = "windows")]
mod windows;

#[cfg(target_family = "windows")]
use windows::*;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
use linux::*;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod macos;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use macos::*;

#[derive(Debug)]
pub struct BufferError {
    msg: String,
}

impl Display for BufferError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

impl Error for BufferError {
    fn description(&self) -> &str {
        &self.msg
    }
}

#[derive(Debug)]
pub struct VoodooBuffer {
    addr: *mut u8,
    len: usize,
    mask: usize,
}

#[allow(clippy::len_without_is_empty)]
impl VoodooBuffer {
    pub fn new(len: usize) -> Result<Self, BufferError> {
        if len == 0 {
            return Err(BufferError {
                msg: "len must be greater than 0".to_string(),
            });
        }

        if !len.is_power_of_two() {
            return Err(BufferError {
                msg: "len must be power of two".to_string(),
            });
        }

        let min_len = Self::min_len();
        if len % min_len != 0 {
            return Err(BufferError {
                msg: format!("len must be page aligned, {}", min_len),
            });
        }

        Ok(Self {
            addr: unsafe { voodoo_buf_alloc(len) }?,
            mask: len - 1,
            len,
        })
    }

    pub fn min_len() -> usize {
        unsafe { voodoo_buf_min_len() }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    unsafe fn as_slice(&self, offset: usize, len: usize) -> &[u8] {
        &*(slice_from_raw_parts(self.addr.add(offset), len))
    }

    #[inline(always)]
    unsafe fn as_slice_mut(&mut self, offset: usize, len: usize) -> &mut [u8] {
        &mut *(slice_from_raw_parts_mut(self.addr.add(offset), len))
    }

    #[inline(always)]
    fn fast_mod(&self, v: usize) -> usize {
        v & self.mask
    }
}

impl Drop for VoodooBuffer {
    fn drop(&mut self) {
        unsafe { voodoo_buf_free(self.addr, self.len) }
    }
}

impl Deref for VoodooBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { self.as_slice(0, self.len) }
    }
}

impl DerefMut for VoodooBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.as_slice_mut(0, self.len) }
    }
}

impl Index<usize> for VoodooBuffer {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        unsafe { &*self.addr.add(self.fast_mod(index)) }
    }
}

impl IndexMut<usize> for VoodooBuffer {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        unsafe { &mut *self.addr.add(self.fast_mod(index)) }
    }
}

impl Index<Range<usize>> for VoodooBuffer {
    type Output = [u8];

    fn index(&self, index: Range<usize>) -> &Self::Output {
        if index.start > index.end {
            return &[];
        }

        let len = index.end - index.start;
        if len > self.len {
            panic!("out of bounds")
        }

        unsafe { self.as_slice(self.fast_mod(index.start), len) }
    }
}

impl IndexMut<Range<usize>> for VoodooBuffer {
    fn index_mut(&mut self, index: Range<usize>) -> &mut Self::Output {
        if index.start > index.end {
            return &mut [];
        }

        let len = index.end - index.start;
        if len > self.len {
            panic!("out of bounds")
        }

        unsafe { self.as_slice_mut(self.fast_mod(index.start), len) }
    }
}

impl Index<RangeTo<usize>> for VoodooBuffer {
    type Output = [u8];

    fn index(&self, index: RangeTo<usize>) -> &Self::Output {
        let start = index.end - self.len;
        unsafe { self.as_slice(self.fast_mod(start), self.len) }
    }
}

impl IndexMut<RangeTo<usize>> for VoodooBuffer {
    fn index_mut(&mut self, index: RangeTo<usize>) -> &mut Self::Output {
        let start = index.end - self.len;
        unsafe { self.as_slice_mut(self.fast_mod(start), self.len) }
    }
}

impl Index<RangeFrom<usize>> for VoodooBuffer {
    type Output = [u8];

    fn index(&self, index: RangeFrom<usize>) -> &Self::Output {
        unsafe { self.as_slice(self.fast_mod(index.start), self.len) }
    }
}

impl IndexMut<RangeFrom<usize>> for VoodooBuffer {
    fn index_mut(&mut self, index: RangeFrom<usize>) -> &mut Self::Output {
        unsafe { self.as_slice_mut(self.fast_mod(index.start), self.len) }
    }
}

impl Index<RangeToInclusive<usize>> for VoodooBuffer {
    type Output = [u8];

    fn index(&self, index: RangeToInclusive<usize>) -> &Self::Output {
        let start = index.end - self.len + 1;
        unsafe { self.as_slice(self.fast_mod(start), self.len) }
    }
}

impl IndexMut<RangeToInclusive<usize>> for VoodooBuffer {
    fn index_mut(&mut self, index: RangeToInclusive<usize>) -> &mut Self::Output {
        let start = index.end - self.len + 1;
        unsafe { self.as_slice_mut(self.fast_mod(start), self.len) }
    }
}

impl Index<RangeFull> for VoodooBuffer {
    type Output = [u8];

    fn index(&self, _: RangeFull) -> &Self::Output {
        unsafe { self.as_slice(0, self.len) }
    }
}

impl IndexMut<RangeFull> for VoodooBuffer {
    fn index_mut(&mut self, _: RangeFull) -> &mut Self::Output {
        unsafe { self.as_slice_mut(0, self.len) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_BUF_LEN: usize = 1 << 16;
    const INVALID_BUF_LEN_ALIGN: usize = 1 << 8;
    const INVALID_BUF_LEN_POW2: usize = (1 << 16) + 5;

    #[test]
    fn allocates_buffer() {
        let buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        drop(buf);
    }

    #[test]
    fn requires_power_of_two() {
        VoodooBuffer::new(INVALID_BUF_LEN_POW2)
            .map_err(|e| {
                println!("{}", e.msg);
                e
            })
            .expect_err("should not allocate buffer");
    }

    #[test]
    fn requires_aligned_len() {
        VoodooBuffer::new(INVALID_BUF_LEN_ALIGN)
            .map_err(|e| {
                println!("{}", e.msg);
                e
            })
            .expect_err("should not allocate buffer");
    }

    #[test]
    fn writes_are_visible_wrap_around() {
        let mut buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        buf[0] = b'a';
        assert_eq!(buf[0], buf[VALID_BUF_LEN]);
    }

    #[test]
    fn deref_as_slice() {
        let buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice: &[u8] = &buf;
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn deref_mut_as_slice() {
        let mut buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice: &mut [u8] = &mut buf;
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn closed_range() {
        let buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &buf[0..VALID_BUF_LEN];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn closed_range_mut() {
        let mut buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &mut buf[0..VALID_BUF_LEN];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn range_to() {
        let buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &buf[..VALID_BUF_LEN + 1];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn range_to_mut() {
        let mut buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &mut buf[..VALID_BUF_LEN + 1];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn range_from() {
        let buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &buf[1..];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn range_from_mut() {
        let mut buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &mut buf[1..];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn range_to_inclusive() {
        let buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &buf[..=VALID_BUF_LEN];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn range_to_inclusive_mut() {
        let mut buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &mut buf[..=VALID_BUF_LEN];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn range_full() {
        let buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &buf[..];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }

    #[test]
    fn range_full_mut() {
        let mut buf = VoodooBuffer::new(VALID_BUF_LEN).expect("should allocate buffer");
        let slice = &mut buf[..];
        assert_eq!(VALID_BUF_LEN, slice.len());
    }
}
