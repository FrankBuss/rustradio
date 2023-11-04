//! Test implementation of circular buffers.
//! Full of unsafe. Full of ugly code.
//!
//! TODO:
//! * It should not be possible to get multiple write buffers.
//! * read buffer .consume() should consume the read buffer.
//! * write buffer .produce() should consume the write buffer.
//!
//! All of the above probably requires that {read,write}_buf returns
//! some handler object.

use anyhow::Result;
use std::os::fd::AsRawFd;

use libc::{c_int, c_uchar, c_void, off_t, size_t};
use libc::{MAP_FAILED, MAP_SHARED, PROT_READ, PROT_WRITE};

extern "C" {
    fn mmap(
        addr: *const c_void,
        len: size_t,
        prot: c_int,
        flags: c_int,
        fd: c_int,
        offset: off_t,
    ) -> *mut c_void;
    fn munmap(addr: *const c_void, length: size_t) -> c_int;
}

/// Circular buffer dealing in bytes.
pub struct Circ {
    buf: *mut c_uchar,
    len: usize,
}

impl Circ {
    /// Create a new circular buffer.
    ///
    /// TODO:
    /// * don't leak memory on error.
    /// * release memory on drop.
    pub fn new() -> Result<Self> {
        let len = 4096usize;
        let len2 = len * 2;
        let f = tempfile::tempfile()?;
        f.set_len(len2 as u64)?;
        let fd = f.as_raw_fd();

        // Map first.
        let buf = unsafe {
            let buf = mmap(
                std::ptr::null::<c_void>(),
                len2 as size_t,
                PROT_READ | PROT_WRITE,
                MAP_SHARED, // flags
                fd,         // fd
                0,          // offset
            );
            if buf == MAP_FAILED {
                panic!();
            }
            buf as *mut c_uchar
        };
        let second = (buf as libc::uintptr_t + len as libc::uintptr_t) as *const c_void;
        // Unmap second half.
        unsafe {
            let rc = munmap(second, len);
            if rc != 0 {
                panic!();
            }
        }
        // Map second half.
        unsafe {
            let buf = mmap(
                second as *const c_void,
                len as size_t,
                PROT_READ | PROT_WRITE,
                MAP_SHARED, // flags
                fd,         // fd
                0,          // offset
            );
            if buf == MAP_FAILED {
                panic!();
            }
            assert_eq!(buf as *const c_void, second);
        };
        Ok(Self { len: len2, buf })
    }
    fn full_buffer<T>(&self) -> &'static mut [T] {
        assert!(self.len % std::mem::size_of::<T>() == 0);
        unsafe {
            std::slice::from_raw_parts_mut(self.buf as *mut T, self.len / std::mem::size_of::<T>())
        }
    }
    fn len(&self) -> usize {
        self.len / 2
    }
}

unsafe impl Send for Circ {}

/// Type aware buffer.
pub struct Buffer<T> {
    rpos: usize, // In samples.
    wpos: usize, // In samples.
    used: usize, // In samples.
    circ: Circ,
    dummy: std::marker::PhantomData<T>,
}

impl<T: Default + std::fmt::Debug + Copy> Buffer<T> {
    /// Create a new Buffer.
    ///
    /// TODO: actually use the `size` parameter.
    pub fn new(size: usize) -> Result<Self> {
        assert_eq!(size, 4096);
        Ok(Self {
            rpos: 0,
            wpos: 0,
            used: 0,
            circ: Circ::new()?,
            dummy: std::marker::PhantomData,
        })
    }

    /// Consume samples from input buffer.
    pub fn consume(&mut self, n: usize) {
        assert!(
            n <= self.used,
            "trying to consume {}, but only have {}",
            n,
            self.used
        );
        self.rpos = (self.rpos + n) % self.capacity();
        self.used -= n;
    }

    /// Produce samples (commit writes).
    pub fn produce(&mut self, n: usize) {
        assert!(self.free() >= n);
        assert!(
            self.write_capacity() >= n,
            "can't produce that much. {} < {}",
            self.write_capacity(),
            n
        );
        self.wpos = (self.wpos + n) % self.capacity();
        self.used += n;
    }

    // In samples.
    fn capacity(&self) -> usize {
        self.circ.len() / std::mem::size_of::<T>()
    }

    // Write capacity, in samples.
    fn write_capacity(&self) -> usize {
        let (a, b) = self.write_range();
        b - a
    }

    // Free space, in samples
    fn free(&self) -> usize {
        self.capacity() - self.used
    }

    // Return write range, in samples.
    fn write_range(&self) -> (usize, usize) {
        //eprintln!("Write range: {} {}", self.rpos, self.wpos);
        (self.wpos, self.wpos + self.free())
    }

    // Read range, in samples
    fn read_range(&self) -> (usize, usize) {
        (self.rpos, self.rpos + self.used)
    }

    /// Get the read slice.
    pub fn read_buf(&mut self) -> &'static [T] {
        let buf = self.circ.full_buffer::<T>();
        let (start, end) = self.read_range();
        unsafe { std::mem::transmute(&buf[start..end]) }
    }

    /// Get the write slice.
    pub fn write_buf(&mut self) -> &'static mut [T] {
        let buf = self.circ.full_buffer::<T>();
        let (start, end) = self.write_range();
        unsafe { std::mem::transmute(&mut buf[start..end]) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Float;
    use std::sync::{Arc, Mutex};

    #[test]
    pub fn test_build() -> Result<()> {
        let b = Arc::new(Mutex::new(Buffer::<u8>::new(4096)?));
        let i1 = b.lock().unwrap().read_buf();
        let i2 = b.lock().unwrap().read_buf();
        let w1 = b.lock().unwrap().write_buf();
        let w2 = b.lock().unwrap().write_buf();
        assert_eq!(i1, i2);
        assert_eq!(w1, w2);
        Ok(())
    }

    #[test]
    pub fn test_typical() -> Result<()> {
        let mut b: Buffer<u8> = Buffer::new(4096)?;

        // Initial.
        assert!(b.read_buf().is_empty());
        assert_eq!(b.write_buf().len(), 4096);

        // Write a byte.
        b.write_buf()[0] = 123;
        b.produce(1);
        assert_eq!(b.read_buf(), vec![123]);
        assert_eq!(b.write_buf().len(), 4095);

        // Consume the byte.
        b.consume(1);
        assert!(b.read_buf().is_empty());
        assert_eq!(b.write_buf().len(), 4096);

        // Write towards the end bytes.
        {
            let n = 4000;
            for i in 0..n {
                b.write_buf()[i] = (i & 0xff) as u8;
            }
            b.produce(n);
            assert_eq!(b.read_buf().len(), n);
            for i in 0..n {
                assert_eq!(b.read_buf()[i], (i & 0xff) as u8);
            }
            assert_eq!(b.write_buf().len(), 4096 - n);
        }
        b.consume(4000);

        // Write 100 bytes.
        {
            let n = 100;
            for i in 0..n {
                b.write_buf()[i] = ((n - i) & 0xff) as u8;
            }
            b.produce(n);
            assert_eq!(b.read_buf().len(), n);
            for i in 0..n {
                assert_eq!(b.read_buf()[i], ((n - i) & 0xff) as u8);
            }
        }
        assert_eq!(b.read_buf().len(), 100);
        assert_eq!(b.write_buf().len(), 3996);
        Ok(())
    }

    #[test]
    pub fn exact_overflow() -> Result<()> {
        let mut b: Buffer<u8> = Buffer::new(4096)?;

        // Initial.
        assert!(b.read_buf().is_empty());
        assert_eq!(b.write_buf().len(), 4096);

        // Full.
        b.produce(4096);
        assert_eq!(b.read_buf().len(), 4096);
        assert_eq!(b.write_buf().len(), 0);

        // Empty again.
        b.consume(4096);
        assert!(b.read_buf().is_empty());
        assert_eq!(b.write_buf().len(), 4096);
        Ok(())
    }

    #[test]
    pub fn test_float() -> Result<()> {
        let mut b: Buffer<Float> = Buffer::new(4096)?;

        // Initial.
        assert!(b.read_buf().is_empty());
        assert_eq!(b.write_buf().len(), 1024);

        // Write a sample.
        b.write_buf()[0] = 123.321;
        b.produce(1);
        assert_eq!(b.read_buf(), vec![123.321]);
        assert_eq!(b.write_buf().len(), 1023);

        // Consume the sample.
        b.consume(1);
        assert!(b.read_buf().is_empty());
        assert_eq!(b.write_buf().len(), 1024);

        // Write towards the end bytes.
        {
            let n = 1000;
            for i in 0..n {
                b.write_buf()[i] = i as Float;
            }
            b.produce(n);
            assert_eq!(b.read_buf().len(), n);
            for i in 0..n {
                assert_eq!(b.read_buf()[i], i as Float);
            }
            assert_eq!(b.write_buf().len(), 24);
        }
        b.consume(1000);

        // Write 100 bytes.
        {
            let n = 100;
            for i in 0..n {
                b.write_buf()[i] = (n - i) as Float;
            }
            b.produce(n);
            assert_eq!(b.read_buf().len(), n);
            for i in 0..n {
                assert_eq!(b.read_buf()[i], (n - i) as Float);
            }
        }
        assert_eq!(b.read_buf().len(), 100);
        assert_eq!(b.write_buf().len(), 1024 - 100);
        Ok(())
    }
}