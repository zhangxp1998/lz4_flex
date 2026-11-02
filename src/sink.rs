#[cfg(feature = "alloc")]
#[allow(unused_imports)]
use alloc::vec::Vec;

use crate::fastcpy::slice_copy;

/// Returns a Sink implementation appropriate for outputting up to `required_capacity`
/// bytes at `vec[offset..offset+required_capacity]`.
/// It can be either a `SliceSink` (pre-filling the vec with zeroes if necessary)
/// when the `safe-decode` feature is enabled, or `VecSink` otherwise.
/// The argument `pos` defines the initial output position in the Sink.
#[inline]
#[cfg(feature = "frame")]
pub(crate) fn vec_sink_for_compression(
    vec: &mut Vec<u8>,
    offset: usize,
    pos: usize,
    required_capacity: usize,
) -> SliceSink<'_> {
    {
        vec.resize(offset + required_capacity, 0);
        SliceSink::new(&mut vec[offset..], pos)
    }
}

/// Returns a Sink implementation appropriate for outputting up to `required_capacity`
/// bytes at `vec[offset..offset+required_capacity]`.
/// It can be either a `SliceSink` (pre-filling the vec with zeroes if necessary)
/// when the `safe-decode` feature is enabled, or `VecSink` otherwise.
/// The argument `pos` defines the initial output position in the Sink.
#[cfg(feature = "frame")]
#[inline]
pub(crate) fn vec_sink_for_decompression(
    vec: &mut Vec<u8>,
    offset: usize,
    pos: usize,
    required_capacity: usize,
) -> SliceSink<'_> {
    {
        vec.resize(offset + required_capacity, 0);
        SliceSink::new(&mut vec[offset..], pos)
    }
}

pub(crate) trait Sink {
    /// Returns a raw ptr to the first unfilled byte of the Sink. Analogous to `[pos..].as_ptr()`.
    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    #[allow(dead_code)]
    unsafe fn pos_mut_ptr(&mut self) -> *mut u8;

    /// read byte at position
    #[allow(dead_code)]
    fn byte_at(&mut self, pos: usize) -> u8;

    /// Pushes a byte to the end of the Sink.
    #[cfg(feature = "safe-encode")]
    fn push(&mut self, byte: u8);

    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    unsafe fn base_mut_ptr(&mut self) -> *mut u8;

    fn pos(&self) -> usize;

    fn capacity(&self) -> usize;

    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    unsafe fn set_pos(&mut self, new_pos: usize);

    #[cfg(feature = "safe-decode")]
    fn extend_with_fill(&mut self, byte: u8, len: usize);

    /// Extends the Sink with `data`.
    fn extend_from_slice(&mut self, data: &[u8]);

    fn extend_from_slice_wild(&mut self, data: &[u8], copy_len: usize);

    /// Copies `len` bytes starting from `start` to the end of the Sink.
    /// # Panics
    /// Panics if `start` >= `pos`.
    #[cfg(feature = "safe-decode")]
    fn extend_from_within(&mut self, start: usize, wild_len: usize, copy_len: usize);

    #[cfg(feature = "safe-decode")]
    fn extend_from_within_overlapping(&mut self, start: usize, num_bytes: usize);
}

/// SliceSink is used as target to de/compress data into a preallocated and possibly uninitialized
/// `&[u8]`
/// space.
///
/// # Handling of Capacity
/// Extend methods will panic if there's insufficient capacity left in the Sink.
///
/// # Invariants
///   - Bytes `[..pos()]` are always initialized.
pub(crate) struct SliceSink<'a> {
    /// The working slice, which may contain uninitialized bytes
    output: &'a mut [u8],
    /// Number of bytes in start of `output` guaranteed to be initialized
    pos: usize,
}

impl<'a> SliceSink<'a> {
    /// Creates a `Sink` backed by the given byte slice.
    /// `pos` defines the initial output position in the Sink.
    /// # Panics
    /// Panics if `pos` is out of bounds.
    #[inline]
    pub(crate) fn new(output: &'a mut [u8], pos: usize) -> Self {
        // SAFETY: Caller guarantees that all elements of `output[..pos]` are initialized.
        let _ = &mut output[..pos]; // bounds check pos
        SliceSink { output, pos }
    }
}

impl Sink for SliceSink<'_> {
    /// Returns a raw ptr to the first unfilled byte of the Sink. Analogous to `[pos..].as_ptr()`.
    #[inline]
    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    unsafe fn pos_mut_ptr(&mut self) -> *mut u8 {
        unsafe { self.base_mut_ptr().add(self.pos()) as *mut u8 }
    }

    /// Pushes a byte to the end of the Sink.
    #[inline]
    fn byte_at(&mut self, pos: usize) -> u8 {
        self.output[pos]
    }

    /// Pushes a byte to the end of the Sink.
    #[inline]
    #[cfg(feature = "safe-encode")]
    fn push(&mut self, byte: u8) {
        self.output[self.pos] = byte;
        self.pos += 1;
    }

    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    unsafe fn base_mut_ptr(&mut self) -> *mut u8 {
        self.output.as_mut_ptr()
    }

    #[inline]
    fn pos(&self) -> usize {
        self.pos
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.output.len()
    }

    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    #[inline]
    unsafe fn set_pos(&mut self, new_pos: usize) {
        debug_assert!(new_pos <= self.capacity());
        self.pos = new_pos;
    }

    #[inline]
    #[cfg(feature = "safe-decode")]
    fn extend_with_fill(&mut self, byte: u8, len: usize) {
        self.output[self.pos..self.pos + len].fill(byte);
        self.pos += len;
    }

    /// Extends the Sink with `data`.
    #[inline]
    fn extend_from_slice(&mut self, data: &[u8]) {
        self.extend_from_slice_wild(data, data.len())
    }

    #[inline]
    fn extend_from_slice_wild(&mut self, data: &[u8], copy_len: usize) {
        assert!(copy_len <= data.len());
        slice_copy(data, &mut self.output[self.pos..(self.pos) + data.len()]);
        self.pos += copy_len;
    }

    /// Copies `len` bytes starting from `start` to the end of the Sink.
    /// # Panics
    /// Panics if `start` >= `pos`.
    #[inline]
    #[cfg(feature = "safe-decode")]
    fn extend_from_within(&mut self, start: usize, wild_len: usize, copy_len: usize) {
        self.output.copy_within(start..start + wild_len, self.pos);
        self.pos += copy_len;
    }

    #[inline]
    #[cfg(feature = "safe-decode")]
    #[cfg_attr(feature = "nightly", optimize(size))] // to avoid loop unrolling
    fn extend_from_within_overlapping(&mut self, start: usize, num_bytes: usize) {
        #[cold]
        fn copy_long_match(output: &mut [u8], offset: usize) {
            assert!(offset != 0);
            let mut copied = offset;
            while copied < output.len() {
                let len = copied.min(output.len() - copied);
                let (src, dst) = output.split_at_mut(copied);
                dst[..len].copy_from_slice(&src[..len]);
                copied += len;
            }
        }

        let offset = self.pos - start;
        if num_bytes > 32 {
            // Only copy bytes that have already been produced. Each copy can
            // double the initialized repeating prefix, avoiding a serial
            // load/store dependency for every byte of a long match.
            copy_long_match(&mut self.output[start..self.pos + num_bytes], offset);
        } else {
            // The byte loop is cheaper than setting up bulk copies for the
            // short matches handled by the decoder's hot loop.
            for i in start + offset..start + offset + num_bytes {
                self.output[i] = self.output[i - offset];
            }
        }
        self.pos += num_bytes;
    }
}

/// PtrSink is used as target to de/compress data into a preallocated and possibly uninitialized
/// `&[u8]`
/// space.
///
///
#[cfg(all(
    feature = "alloc",
    not(all(feature = "safe-encode", feature = "safe-decode"))
))]
pub(crate) struct PtrSink {
    /// The working slice, which may contain uninitialized bytes
    output: *mut u8,
    /// Number of bytes in start of `output` guaranteed to be initialized
    pos: usize,
    /// Number of bytes in output available
    cap: usize,
}

#[cfg(all(
    feature = "alloc",
    not(all(feature = "safe-encode", feature = "safe-decode"))
))]
impl PtrSink {
    /// Creates a `Sink` backed by the given byte slice.
    /// `pos` defines the initial output position in the Sink.
    /// # Panics
    /// Panics if `pos` is out of bounds.
    #[inline]
    pub(crate) fn from_vec(output: &mut Vec<u8>, pos: usize) -> Self {
        assert!(
            pos <= output.len(),
            "invalid pos {pos} for vec with len {}",
            output.len()
        );

        // SAFETY: Bytes behind pointer may be uninitialized.
        Self {
            output: output.as_mut_ptr(),
            pos,
            cap: output.capacity(),
        }
    }
}

#[cfg(all(
    feature = "alloc",
    not(all(feature = "safe-encode", feature = "safe-decode"))
))]
impl Sink for PtrSink {
    /// Returns a raw ptr to the first unfilled byte of the Sink. Analogous to `[pos..].as_ptr()`.
    #[inline]
    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    unsafe fn pos_mut_ptr(&mut self) -> *mut u8 {
        unsafe { self.base_mut_ptr().add(self.pos()) as *mut u8 }
    }

    /// Pushes a byte to the end of the Sink.
    #[inline]
    fn byte_at(&mut self, pos: usize) -> u8 {
        unsafe { self.output.add(pos).read() }
    }

    /// Pushes a byte to the end of the Sink.
    #[inline]
    #[cfg(feature = "safe-encode")]
    fn push(&mut self, byte: u8) {
        unsafe {
            self.pos_mut_ptr().write(byte);
        }
        self.pos += 1;
    }

    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    unsafe fn base_mut_ptr(&mut self) -> *mut u8 {
        self.output
    }

    #[inline]
    fn pos(&self) -> usize {
        self.pos
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.cap
    }

    #[cfg(not(all(feature = "safe-encode", feature = "safe-decode")))]
    #[inline]
    unsafe fn set_pos(&mut self, new_pos: usize) {
        debug_assert!(new_pos <= self.capacity());
        self.pos = new_pos;
    }

    #[inline]
    #[cfg(feature = "safe-decode")]
    fn extend_with_fill(&mut self, _byte: u8, _len: usize) {
        unreachable!();
    }

    /// Extends the Sink with `data`.
    #[inline]
    fn extend_from_slice(&mut self, data: &[u8]) {
        self.extend_from_slice_wild(data, data.len())
    }

    #[inline]
    fn extend_from_slice_wild(&mut self, data: &[u8], copy_len: usize) {
        assert!(copy_len <= data.len());
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), self.pos_mut_ptr(), copy_len);
        }
        self.pos += copy_len;
    }

    /// Copies `len` bytes starting from `start` to the end of the Sink.
    /// # Panics
    /// Panics if `start` >= `pos`.
    #[inline]
    #[cfg(feature = "safe-decode")]
    fn extend_from_within(&mut self, _start: usize, _wild_len: usize, _copy_len: usize) {
        unreachable!();
    }

    #[inline]
    #[cfg(feature = "safe-decode")]
    fn extend_from_within_overlapping(&mut self, _start: usize, _num_bytes: usize) {
        unreachable!();
    }
}

#[cfg(test)]
mod tests {

    #[test]
    #[cfg(feature = "safe-decode")]
    fn overlapping_matches_preserve_pattern_and_tail() {
        use crate::sink::{Sink, SliceSink};

        for prefix_len in [0, 1, 7] {
            for offset in [1, 2, 3, 4, 7, 8, 15, 16, 17, 31, 32, 33, 255, 65535] {
                for len in [0, 1, 17, 31, 32, 33, 34, 63, 64, 65, 127, 257, 4096, 65536] {
                    let pos = prefix_len + offset;
                    // Test both exact-capacity output and a dirty suffix, which
                    // must not be read as match data or overwritten.
                    for tail_len in [0, 11] {
                        let mut actual = vec![0xa5; pos + len + tail_len];
                        for (i, byte) in actual[..pos].iter_mut().enumerate() {
                            *byte = (i.wrapping_mul(31) ^ (i >> 3)) as u8;
                        }
                        let mut expected = actual.clone();
                        for i in pos..pos + len {
                            expected[i] = expected[i - offset];
                        }
                        let mut sink = SliceSink::new(&mut actual, pos);
                        sink.extend_from_within_overlapping(prefix_len, len);
                        assert_eq!(sink.pos(), pos + len);
                        assert_eq!(actual, expected, "offset={offset}, len={len}");
                    }
                }
            }
        }
    }

    #[test]
    #[cfg(any(feature = "safe-encode", feature = "safe-decode"))]
    fn test_sink_slice() {
        use crate::sink::Sink;
        use crate::sink::SliceSink;
        let mut data = vec![0; 5];
        let sink = SliceSink::new(&mut data, 1);
        assert_eq!(sink.pos(), 1);
        assert_eq!(sink.capacity(), 5);
    }
}
