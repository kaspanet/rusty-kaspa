//!
//! This crate provides functionality for manual serialization and deserialization of
//! data from `u8` buffers.  If you are looking for proper serialization libraries please
//! consider using [`serde`](https://crates.io/crates/serde). This crate is useful for
//! direct value extraction from memory buffers where you need to quickly extract values
//! from structured data that may not be aligned.
//!
//! NOTE: Current implementation support extraction only for primitives encoded with
//! little-endian encoding.
//!
//! # Example
//!
//! ```rust
//!
//! #[derive(Debug)]
//! pub struct Header {
//!     pub magic : usize,
//!     pub version : u16,
//!     pub payload : Vec<u8>,
//! }
//!
//! impl TrySerialize for Header {
//!     type Error = Error;
//!     fn try_serialize(&self, dest: &mut Serializer) -> Result<()> {
//!         dest.try_align_u32()?;
//!         dest.try_store_u16le(self.magic as u16)?;
//!         dest.try_store_u16le(self.version)?;
//!         dest.try_store_u32le(self.payload.len() as u32)?;
//!         dest.try_store_u8_slice(&self.payload)?;
//!         Ok(())
//!     }
//! }
//!
//! fn store() {
//!     let mut dest = Serializer::new(4096);
//!     let header = Header::default();
//!     dest.try_store(&header)?;
//! }
//!
//! impl TryDeserialize for Header {
//!     type Error = Error;
//!
//!     fn try_deserialize(src: &mut Deserializer) -> Result<Header> {
//!         src.try_align_u32()?;
//!
//!         let magic = src.try_load_u16le()? as usize;
//!         let version = src.try_load_u16le()?;
//!         let payload_length = src.try_load_u32le()? as usize;
//!         let payload = src.try_load_u8_vec(payload_length)?.to_vec()?;
//!
//!         Ok(Header{magic, version, payload})
//!     }
//! }
//!
//! fn load(data: &[u8], offset: usize) -> Result<(u32,Header)>{
//!     let mut src = Deserializer::new(data);
//!     src.offset(offset)?;
//!     let signature = src.try_load_u32le()?;
//!     let header: Header = src.try_load()?;
//!     Ok((signature,header))
//! }
//! ```
//!
pub mod error;
pub use error::Error;
pub mod result;
pub use result::Result;

/// Deserializer referring an existing `u8` buffer
pub struct Deserializer<'data> {
    data: &'data [u8],
    cursor: usize,
}

impl<'data> Deserializer<'data> {
    /// Create a new `Deserializer` referring the supplied `u8` buffer
    pub fn new(data: &'data [u8]) -> Deserializer<'data> {
        Deserializer { data, cursor: 0 }
    }

    /// Get current byte position of the reader
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Get amount of bytes remaining for consumption
    pub fn remaining(&self) -> usize {
        self.data.len() - self.cursor
    }

    /// Advance the reader by `offset` bytes
    pub fn try_offset(&mut self, offset: usize) -> Result<()> {
        self.cursor += offset;
        if self.cursor > self.data.len() {
            return Err(format!(
                "deserializer offset {offset} is out of bounds[0..{}]",
                self.data.len()
            )
            .into());
        }
        Ok(())
    }

    /// Advance the cursor to ensure that current cursor position is 32-bit aligned
    pub fn try_align_u32(&mut self) -> Result<()> {
        self.try_align(4)?;
        Ok(())
    }

    /// Advance the cursor to ensure that current cursor position is 64-bit aligned
    pub fn try_align_u64(&mut self) -> Result<()> {
        self.try_align(8)?;
        Ok(())
    }

    /// Advance the cursor to ensure its alignment is on the `align` byte boundary.
    /// The following ensures that the cursor is on a 128-bit alignment:
    /// ```
    /// deser.try_align(16)?;
    /// ```
    pub fn try_align(&mut self, align: usize) -> Result<()> {
        let offset = self.cursor % align;
        self.try_offset(offset)?;
        Ok(())
    }

    /// Set the cursor byte position to the given `cursor` value
    pub fn try_set_cursor(&mut self, cursor: usize) -> Result<()> {
        self.cursor = cursor;
        if self.cursor > self.data.len() {
            return Err(format!(
                "deserializer cursor {cursor} is out of bounds[0..{}]",
                self.data.len()
            )
            .into());
        }
        Ok(())
    }

    /// Try reading `Vec<u8>` buffer of the supplied `len` byte length.
    pub fn try_load_u8_vec(&mut self, len: usize) -> Result<Vec<u8>> {
        if self.cursor + len > self.data.len() {
            return Err(format!(
                "try_u8vec(): deserializer cursor {} is out of bounds[0..{}]",
                self.cursor + len,
                self.data.len()
            )
            .into());
        }
        let mut vec: Vec<u8> = Vec::with_capacity(len);
        vec.resize(len, 0);
        vec.copy_from_slice(&self.data[self.cursor..self.cursor + len]);
        self.cursor += len;
        Ok(vec)
    }

    /// Try reading `Vec<u16>` array of the supplied `len` elements.
    /// The `u16` values are read as little-endian values.
    pub fn try_load_u16le_vec(&mut self, len: usize) -> Result<Vec<u16>> {
        let mut vec: Vec<u16> = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(self.try_load_u16le()?)
        }
        Ok(vec)
    }

    /// Try reading an array of `u16` little-endian values from a
    /// zero-terminated `u16` array and return it as a Rust `String`.  
    /// This function is useful for reading windows `PCWSTR` zero-terminated
    /// strings into Rust strings.
    pub fn try_load_utf16le_sz(&mut self) -> Result<String> {
        let mut vec: Vec<u16> = Vec::new();
        loop {
            let v = self.try_load_u16le()?;
            if v == 0 {
                break;
            }
            vec.push(v);
        }
        Ok(String::from_utf16(&vec)?)
    }

    /// Load a u8 value
    pub fn load_u8(&mut self) -> u8 {
        let last = self.cursor + 1;
        let v = u8::from_le_bytes(self.data[self.cursor..last].try_into().unwrap());
        self.cursor = last;
        v
    }

    /// Try load a u8 value
    pub fn try_load_u8(&mut self) -> Result<u8> {
        let last = self.cursor + 1;
        let v = u8::from_le_bytes(self.data[self.cursor..last].try_into()?);
        self.cursor = last;
        Ok(v)
    }

    /// Load a u16 little-endian value
    pub fn load_u16le(&mut self) -> u16 {
        let last = self.cursor + 2;
        let v = u16::from_le_bytes(self.data[self.cursor..last].try_into().unwrap());
        self.cursor = last;
        v
    }

    /// Try load a u16 little-endian value
    pub fn try_load_u16le(&mut self) -> Result<u16> {
        let last = self.cursor + 2;
        let v = u16::from_le_bytes(self.data[self.cursor..last].try_into()?);
        self.cursor = last;
        Ok(v)
    }

    /// Load a u32 little-endian value
    pub fn load_u32le(&mut self) -> u32 {
        let last = self.cursor + 4;
        let v = u32::from_le_bytes(self.data[self.cursor..last].try_into().unwrap());
        self.cursor = last;
        v
    }

    /// Try load a u32 little-endian value
    pub fn try_load_u32le(&mut self) -> Result<u32> {
        let last = self.cursor + 4;
        let v = u32::from_le_bytes(self.data[self.cursor..last].try_into()?);
        self.cursor = last;
        Ok(v)
    }

    /// Load a u64 little-endian value
    pub fn load_u64le(&mut self) -> u64 {
        let last = self.cursor + 8;
        let v = u64::from_le_bytes(self.data[self.cursor..last].try_into().unwrap());
        self.cursor = last;
        v
    }

    /// Try load a u64 little-endian value
    pub fn try_load_u64le(&mut self) -> Result<u64> {
        let last = self.cursor + 8;
        let v = u64::from_le_bytes(self.data[self.cursor..last].try_into()?);
        self.cursor = last;
        Ok(v)
    }

    /// Load a primitive implementing a [`Deserialize`] trait
    pub fn load<S: Deserialize>(&mut self) -> S {
        S::deserialize(self)
    }

    /// Try load a primitive implementing a [`Deserialize`] trait
    pub fn try_load<S: TryDeserialize>(&mut self) -> std::result::Result<S, S::Error> {
        S::try_deserialize(self)
    }
}

/// TryDeserialize trait accepted by [`Deserializer::try_load`]
pub trait TryDeserialize
where
    Self: Sized,
{
    type Error;
    fn try_deserialize(dest: &mut Deserializer) -> std::result::Result<Self, Self::Error>;
}

/// Deserialize trait accepted by [`Deserializer::load`]
pub trait Deserialize {
    fn deserialize(dest: &mut Deserializer) -> Self;
}

/// Serializer struct containing a serialization buffer.
/// Please note that this struct expects to have a sufficiently large buffer
/// to perform the ongoing serialization.
pub struct Serializer {
    data: Vec<u8>,
    cursor: usize,
}

/// Default implementation for [`Serializer`] that allocates 4096 byte buffer.
impl Default for Serializer {
    fn default() -> Serializer {
        Serializer::new(4096)
    }
}

impl Serializer {
    /// Create a new [`Serializer`] struct with `len` byte buffer
    pub fn new(len: usize) -> Serializer {
        let mut data = Vec::with_capacity(len);
        data.resize(len, 0);
        Serializer { data, cursor: 0 }
    }

    /// Returns the current byte length of the ongoing serialization (cursor position)
    pub fn len(&self) -> usize {
        self.cursor
    }

    /// Returns `Vec<u8>` of the currently serialized data
    pub fn to_vec(&self) -> Vec<u8> {
        self.data[0..self.cursor].to_vec()
    }

    /// Returns a slice `&[u8]` of the currently serialized data
    pub fn as_slice<'slice>(&'slice self) -> &'slice [u8] {
        &self.data[0..self.cursor]
    }

    /// Advance the cursor by `offset` bytes. Since the underlying
    /// buffer is zero-initialized, skipped bytes will remain as zero.
    pub fn offset(&mut self, offset: usize) -> &mut Self {
        if self.cursor + offset >= self.len() {}
        self.cursor += offset;
        self
    }

    /// Try advance the cursor by `offset` bytes. Since the underlying
    /// buffer is zero-initialized, skipped bytes will remain as zero.
    pub fn try_offset(&mut self, offset: usize) -> Result<&mut Self> {
        if self.cursor + offset >= self.data.len() {
            return Err(Error::TryOffsetError(offset, self.cursor, self.len()));
        }
        self.cursor += offset;
        Ok(self)
    }

    /// Advance the cursor by `offset` bytes while explicitly setting
    /// skipped bytes to zero. This can be useful if manually positioning
    /// cursor within the buffer and the repositioning can result in
    /// the buffer containing previously serialized data.
    pub fn offset_with_zeros(&mut self, offset: usize) -> &mut Self {
        for _ in 0..offset {
            self.store_u8(0);
        }
        self
    }

    /// Try advance the cursor by `offset` bytes while explicitly setting
    /// skipped bytes to zero. This can be useful if manually positioning
    /// cursor within the buffer and the repositioning can result in
    /// the buffer containing previously serialized data.
    pub fn try_offset_with_zeros(&mut self, offset: usize) -> Result<&mut Self> {
        if self.cursor + offset >= self.data.len() {
            return Err(Error::TryOffsetError(offset, self.cursor, self.len()));
        }
        for _ in 0..offset {
            self.store_u8(0);
        }
        Ok(self)
    }

    /// Advance the cursor to ensure that the current cursor position
    /// is on the 32-bit alignment boundary.
    pub fn align_u32(&mut self) -> &mut Self {
        let offset = self.cursor % 4;
        self.offset(offset)
    }

    /// Try advance the cursor to ensure that the current cursor position
    /// is on the 32-bit alignment boundary.
    pub fn try_align_u32(&mut self) -> Result<&mut Self> {
        let offset = self.cursor % 4;
        self.try_offset(offset)
    }

    /// Advance the cursor to ensure that the current cursor position
    /// is on the 64-bit alignment boundary.
    pub fn align_u64(&mut self) -> &mut Self {
        let offset = self.cursor % 8;
        self.offset(offset)
    }

    /// Try advance the cursor to ensure that the current cursor position
    /// is on the 64-bit alignment boundary.
    pub fn try_align_u64(&mut self) -> Result<&mut Self> {
        let offset = self.cursor % 8;
        self.try_offset(offset)
    }

    /// Store a single `u8` value, advancing the cursor by 1 byte.
    pub fn store_u8(&mut self, v: u8) -> &mut Self {
        let last = self.cursor + 1;
        self.data[self.cursor..last].copy_from_slice(&v.to_le_bytes());
        self.cursor = last;
        self
    }

    /// Try store a single `u8` value, advancing the cursor by 1 byte.
    pub fn try_store_u8(&mut self, v: u8) -> Result<&mut Self> {
        if self.cursor + 1 >= self.data.len() {
            return Err(Error::TryStoreError("u8", self.cursor, self.data.len()));
        }
        let last = self.cursor + 1;
        self.data[self.cursor..last].copy_from_slice(&v.to_le_bytes());
        self.cursor = last;
        Ok(self)
    }

    /// Store a `u16` value using little-endian encoding, advancing the cursor by 2 bytes.
    pub fn store_u16le(&mut self, v: u16) -> &mut Self {
        let last = self.cursor + 2;
        self.data[self.cursor..last].copy_from_slice(&v.to_le_bytes());
        self.cursor = last;
        self
    }

    /// Try to store a `u16` value using little-endian encoding, advancing the cursor by 2 bytes.
    pub fn try_store_u16le(&mut self, v: u16) -> Result<&mut Self> {
        if self.cursor + 2 >= self.data.len() {
            return Err(Error::TryStoreError("u16", self.cursor, self.data.len()));
        }
        let last = self.cursor + 2;
        self.data[self.cursor..last].copy_from_slice(&v.to_le_bytes());
        self.cursor = last;
        Ok(self)
    }

    /// Store a `u32` value using little-endian encoding, advancing the cursor by 4 bytes.
    pub fn store_u32le(&mut self, v: u32) -> &mut Self {
        let last = self.cursor + 4;
        self.data[self.cursor..last].copy_from_slice(&v.to_le_bytes());
        self.cursor = last;
        self
    }

    /// Try to store a `u32` value using little-endian encoding, advancing the cursor by 4 bytes.
    pub fn try_store_u32le(&mut self, v: u32) -> Result<&mut Self> {
        if self.cursor + 4 >= self.data.len() {
            return Err(Error::TryStoreError("u32", self.cursor, self.data.len()));
        }
        let last = self.cursor + 4;
        self.data[self.cursor..last].copy_from_slice(&v.to_le_bytes());
        self.cursor = last;
        Ok(self)
    }

    /// Store a `u64` value using little-endian encoding, advancing the cursor by 8 bytes.
    pub fn store_u64le(&mut self, v: u64) -> &mut Self {
        let last = self.cursor + 8;
        self.data[self.cursor..last].copy_from_slice(&v.to_le_bytes());
        self.cursor = last;
        self
    }

    /// Try to store a `u64` value using little-endian encoding, advancing the cursor by 8 bytes.
    pub fn try_store_u64le(&mut self, v: u64) -> Result<&mut Self> {
        if self.cursor + 8 >= self.data.len() {
            return Err(Error::TryStoreError("u64", self.cursor, self.data.len()));
        }
        let last = self.cursor + 8;
        self.data[self.cursor..last].copy_from_slice(&v.to_le_bytes());
        self.cursor = last;
        Ok(self)
    }

    /// Try to store a Rust `String` as a zero-terminated sequence of `u16`
    /// little-endian encoded bytes. This is useful to serialize windows `PCWSTR`
    /// zero-terminated strings.
    pub fn try_store_utf16le_sz(&mut self, text: &String) -> Result<&mut Self> {
        let len = text.len() + 1;
        let mut vec: Vec<u16> = Vec::with_capacity(len);
        for c in text.chars() {
            // TODO - proper encoding
            // let buf = [0;2];
            // c.encode_utf16(&mut buf);
            vec.push(c as u16);
        }
        vec.push(0);
        // println!("text: {} vec: {:?}",text,vec);
        self.try_store_u16le_slice(&vec)?;
        Ok(self)
    }

    /// Try to store a `u8` slice
    pub fn try_store_u8_slice(&mut self, vec: &[u8]) -> Result<&mut Self> {
        let len = vec.len();
        let last = self.cursor + len;
        if last >= self.data.len() {
            return Err(Error::TryStoreSliceError(len, self.cursor, self.data.len()));
        }
        let src = unsafe { std::mem::transmute(vec.as_ptr()) };
        let dest = self.data[self.cursor..last].as_mut_ptr();
        unsafe {
            std::ptr::copy(src, dest, len);
        }
        self.cursor = last;
        Ok(self)
    }

    /// Try to store a `u16` slice as a sequence of little-endian encoded `u16` values.
    pub fn try_store_u16le_slice(&mut self, vec: &[u16]) -> Result<&mut Self> {
        let src = unsafe { std::mem::transmute(vec.as_ptr()) };
        let bytelen = vec.len() * 2;
        let last = self.cursor + bytelen;
        if last >= self.data.len() {
            return Err(Error::TryStoreSliceError(
                bytelen,
                self.cursor,
                self.data.len(),
            ));
        }
        let dest = self.data[self.cursor..last].as_mut_ptr();
        unsafe {
            std::ptr::copy(src, dest, bytelen);
        }
        self.cursor = last;
        Ok(self)
    }

    /// Store a primitive implementing a [`Serialize`] trait
    pub fn store<S: Serialize>(&mut self, s: &S) -> &mut Self {
        s.serialize(self);
        self
    }

    /// Try store a primitive implementing a [`TrySerialize`] trait
    pub fn try_store<S: TrySerialize>(
        &mut self,
        s: &S,
    ) -> std::result::Result<&mut Self, S::Error> {
        s.try_serialize(self)?;
        Ok(self)
    }
}

/// TrySerialize trait accepted by the [`Serializer::try_store`]
pub trait TrySerialize {
    type Error;
    fn try_serialize(&self, dest: &mut Serializer) -> std::result::Result<(), Self::Error>;
}

/// Serialize trait accepted by the [`Serializer::store`]
pub trait Serialize {
    fn serialize(&self, dest: &mut Serializer);
}

// helper functions

/// store u64 little-endian value in the supplied buffer, returning the number of bytes written
#[inline]
pub fn store_u64le(dest: &mut [u8], v: u64) -> usize {
    dest[0..8].copy_from_slice(&v.to_le_bytes());
    8
}

/// store u32 little-endian value in the supplied buffer, returning the number of bytes written
#[inline]
pub fn store_u32le(dest: &mut [u8], v: u32) -> usize {
    dest[0..4].copy_from_slice(&v.to_le_bytes());
    4
}

/// store u16 little-endian value in the supplied buffer, returning the number of bytes written
#[inline]
pub fn store_u16le(dest: &mut [u8], v: u16) -> usize {
    dest[0..2].copy_from_slice(&v.to_le_bytes());
    2
}

/// store u8 value in the supplied buffer, returning the number of bytes written
#[inline]
pub fn store_u8(dest: &mut [u8], v: u8) -> usize {
    dest[0..1].copy_from_slice(&v.to_le_bytes());
    1
}

/// load u64 little-endian value from the supplied buffer
#[inline]
pub fn load_u64le(src: &[u8]) -> u64 {
    u64::from_le_bytes(src[0..8].try_into().unwrap())
}

/// load u32 little-endian value from the supplied buffer
#[inline]
pub fn load_u32le(src: &[u8]) -> u32 {
    u32::from_le_bytes(src[0..4].try_into().unwrap())
}

/// load u16 little-endian value from the supplied buffer
#[inline]
pub fn load_u16le(src: &[u8]) -> u16 {
    u16::from_le_bytes(src[0..2].try_into().unwrap())
}

/// load u8 value from the supplied buffer
#[inline]
pub fn load_u8(src: &[u8]) -> u8 {
    u8::from_le_bytes(src[0..1].try_into().unwrap())
}
