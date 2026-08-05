//! Reading and writing JDWP's data types.
//!
//! Everything is **big-endian**, and the sizes of the five identifier kinds are not fixed by
//! the specification: they are *negotiated*. `VirtualMachine.IDSizes` is the first command any
//! client sends, and until its answer is in hand no other reply can be parsed — an `objectID`
//! is four bytes on one VM and eight on the next, and guessing wrong does not fail loudly, it
//! silently shifts every field after it.
//!
//! That is why [`IdSizes`] is threaded through [`Reader`] and [`Writer`] rather than being a
//! constant: it is the one piece of state the codec cannot do without.

use crate::error::{JdwpError, Result};

/// The negotiated width, in bytes, of each identifier kind. Answered by
/// `VirtualMachine.IDSizes`.
///
/// The default is the 8-byte answer every 64-bit HotSpot gives, so a client that has not
/// asked yet can at least parse that command's own reply — which contains nothing but ints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdSizes {
    pub field: usize,
    pub method: usize,
    pub object: usize,
    pub reference_type: usize,
    pub frame: usize,
}

impl Default for IdSizes {
    fn default() -> Self {
        IdSizes { field: 8, method: 8, object: 8, reference_type: 8, frame: 8 }
    }
}

/// An identifier of any kind. All five are opaque numbers whose width came from [`IdSizes`];
/// keeping them as one type avoids five newtypes that never do anything different.
pub type Id = u64;

/// Where execution is: a method, and how far into its bytecode.
///
/// `index` is a **bytecode index**, not a line — turning one into the other is
/// `Method.LineTable`, and a class compiled without debug information has no table at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    /// `1` = class, `2` = interface, `3` = array.
    pub type_tag: u8,
    pub class: Id,
    pub method: Id,
    pub index: u64,
}

/// The tag byte in front of every value the VM hands back — JDWP is dynamically typed on the
/// wire, so the tag decides how many bytes follow and what they mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Array,
    Byte,
    Char,
    Object,
    Float,
    Double,
    Int,
    Long,
    Short,
    Void,
    Boolean,
    String,
    Thread,
    ThreadGroup,
    ClassLoader,
    ClassObject,
}

impl Tag {
    pub fn from_byte(b: u8) -> Result<Tag> {
        Ok(match b {
            b'[' => Tag::Array,
            b'B' => Tag::Byte,
            b'C' => Tag::Char,
            b'L' => Tag::Object,
            b'F' => Tag::Float,
            b'D' => Tag::Double,
            b'I' => Tag::Int,
            b'J' => Tag::Long,
            b'S' => Tag::Short,
            b'V' => Tag::Void,
            b'Z' => Tag::Boolean,
            b's' => Tag::String,
            b't' => Tag::Thread,
            b'g' => Tag::ThreadGroup,
            b'l' => Tag::ClassLoader,
            b'c' => Tag::ClassObject,
            other => {
                return Err(JdwpError::Protocol(format!("unknown value tag {other:#04x}")));
            }
        })
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Tag::Array => b'[',
            Tag::Byte => b'B',
            Tag::Char => b'C',
            Tag::Object => b'L',
            Tag::Float => b'F',
            Tag::Double => b'D',
            Tag::Int => b'I',
            Tag::Long => b'J',
            Tag::Short => b'S',
            Tag::Void => b'V',
            Tag::Boolean => b'Z',
            Tag::String => b's',
            Tag::Thread => b't',
            Tag::ThreadGroup => b'g',
            Tag::ClassLoader => b'l',
            Tag::ClassObject => b'c',
        }
    }

    /// Whether this tag's payload is an object identifier rather than a primitive — the
    /// difference between a value you can print and one you have to go and fetch.
    pub fn is_object(self) -> bool {
        matches!(
            self,
            Tag::Array
                | Tag::Object
                | Tag::String
                | Tag::Thread
                | Tag::ThreadGroup
                | Tag::ClassLoader
                | Tag::ClassObject
        )
    }
}

/// One value read off the wire.
///
/// Object-ish values carry only their identifier: JDWP hands out a handle, and turning it
/// into something readable is another round trip (`StringReference.Value`,
/// `ObjectReference.ReferenceType`). Keeping that explicit here is deliberate — a "value"
/// that silently made three more calls would make a variables panel unpredictably slow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Void,
    Boolean(bool),
    Byte(i8),
    Char(u16),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    /// An object handle, with the tag that says what kind. `0` is `null`.
    Object { tag: Tag, id: Id },
}

impl Value {
    /// Whether this is the null reference — the single most common thing a debugger is asked
    /// about, and one that needs no round trip to answer.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Object { id: 0, .. })
    }
}

// ── reading ─────────────────────────────────────────────────────────────────────

/// A cursor over a reply's payload.
pub struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    sizes: IdSizes,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], sizes: IdSizes) -> Self {
        Reader { data, at: 0, sizes }
    }

    pub fn sizes(&self) -> IdSizes {
        self.sizes
    }

    /// How much is left — for a caller checking it consumed everything.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.at)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or_else(|| short(n, 0))?;
        if end > self.data.len() {
            return Err(short(n, self.remaining()));
        }
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i64(&mut self) -> Result<i64> {
        let b = self.take(8)?;
        Ok(i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    /// An identifier of `width` bytes (1, 2, 4 or 8 — the VM chooses).
    pub fn id(&mut self, width: usize) -> Result<Id> {
        let b = self.take(width)?;
        Ok(b.iter().fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte)))
    }

    pub fn object_id(&mut self) -> Result<Id> {
        self.id(self.sizes.object)
    }

    pub fn reference_type_id(&mut self) -> Result<Id> {
        self.id(self.sizes.reference_type)
    }

    pub fn method_id(&mut self) -> Result<Id> {
        self.id(self.sizes.method)
    }

    pub fn field_id(&mut self) -> Result<Id> {
        self.id(self.sizes.field)
    }

    pub fn frame_id(&mut self) -> Result<Id> {
        self.id(self.sizes.frame)
    }

    /// A length-prefixed string. JDWP says *modified* UTF-8; the difference (a two-byte
    /// encoding of NUL, surrogate pairs written separately) shows up in string contents, never
    /// in the signatures and names this crate reads, so the bytes are taken as UTF-8 and
    /// anything invalid is replaced rather than failing a whole reply.
    pub fn string(&mut self) -> Result<String> {
        let len = self.i32()?;
        let len = usize::try_from(len)
            .map_err(|_| JdwpError::Protocol(format!("negative string length {len}")))?;
        Ok(String::from_utf8_lossy(self.take(len)?).into_owned())
    }

    pub fn location(&mut self) -> Result<Location> {
        Ok(Location {
            type_tag: self.u8()?,
            class: self.reference_type_id()?,
            method: self.method_id()?,
            index: self.i64()? as u64,
        })
    }

    /// A tagged value — the tag byte, then as many bytes as it implies.
    pub fn value(&mut self) -> Result<Value> {
        let tag = Tag::from_byte(self.u8()?)?;
        self.value_of(tag)
    }

    /// A value whose tag was already read (or is known from a signature).
    pub fn value_of(&mut self, tag: Tag) -> Result<Value> {
        Ok(match tag {
            Tag::Void => Value::Void,
            Tag::Boolean => Value::Boolean(self.bool()?),
            Tag::Byte => Value::Byte(self.i8()?),
            Tag::Char => Value::Char(self.u16()?),
            Tag::Short => Value::Short(self.u16()? as i16),
            Tag::Int => Value::Int(self.i32()?),
            Tag::Long => Value::Long(self.i64()?),
            Tag::Float => Value::Float(f32::from_bits(self.i32()? as u32)),
            Tag::Double => Value::Double(f64::from_bits(self.i64()? as u64)),
            other => Value::Object { tag: other, id: self.object_id()? },
        })
    }
}

fn short(want: usize, have: usize) -> JdwpError {
    JdwpError::Protocol(format!("truncated packet: wanted {want} more bytes, {have} left"))
}

// ── writing ─────────────────────────────────────────────────────────────────────

/// A growing command payload.
#[derive(Debug, Clone)]
pub struct Writer {
    data: Vec<u8>,
    sizes: IdSizes,
}

impl Writer {
    pub fn new(sizes: IdSizes) -> Self {
        Writer { data: Vec::new(), sizes }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.data.push(v);
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.data.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.data.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// An identifier, written in the width the VM negotiated for its kind.
    pub fn id(&mut self, v: Id, width: usize) -> &mut Self {
        let bytes = v.to_be_bytes();
        self.data.extend_from_slice(&bytes[8 - width.min(8)..]);
        self
    }

    pub fn object_id(&mut self, v: Id) -> &mut Self {
        self.id(v, self.sizes.object)
    }

    pub fn reference_type_id(&mut self, v: Id) -> &mut Self {
        self.id(v, self.sizes.reference_type)
    }

    pub fn method_id(&mut self, v: Id) -> &mut Self {
        self.id(v, self.sizes.method)
    }

    pub fn field_id(&mut self, v: Id) -> &mut Self {
        self.id(v, self.sizes.field)
    }

    pub fn frame_id(&mut self, v: Id) -> &mut Self {
        self.id(v, self.sizes.frame)
    }

    pub fn string(&mut self, v: &str) -> &mut Self {
        self.i32(v.len() as i32);
        self.data.extend_from_slice(v.as_bytes());
        self
    }

    pub fn location(&mut self, l: Location) -> &mut Self {
        self.u8(l.type_tag);
        self.reference_type_id(l.class);
        self.method_id(l.method);
        self.i64(l.index as i64);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NARROW: IdSizes =
        IdSizes { field: 4, method: 4, object: 4, reference_type: 4, frame: 4 };

    #[test]
    fn identifiers_are_read_at_the_width_the_vm_chose() {
        // The same four bytes are one id on a narrow VM and half of one on a wide VM. This is
        // the whole reason IDSizes is asked first.
        let bytes = [0, 0, 0, 1, 0, 0, 0, 2];
        let mut narrow = Reader::new(&bytes, NARROW);
        assert_eq!(narrow.object_id().unwrap(), 1);
        assert_eq!(narrow.object_id().unwrap(), 2);

        let mut wide = Reader::new(&bytes, IdSizes::default());
        assert_eq!(wide.object_id().unwrap(), 0x0000_0001_0000_0002);
    }

    #[test]
    fn an_identifier_round_trips_at_any_width() {
        for sizes in [NARROW, IdSizes::default()] {
            let mut w = Writer::new(sizes);
            w.object_id(0x0102_0304);
            let bytes = w.into_bytes();
            assert_eq!(bytes.len(), sizes.object);
            assert_eq!(Reader::new(&bytes, sizes).object_id().unwrap(), 0x0102_0304);
        }
    }

    #[test]
    fn a_location_round_trips() {
        let sizes = IdSizes::default();
        let loc = Location { type_tag: 1, class: 77, method: 4242, index: 19 };
        let mut w = Writer::new(sizes);
        w.location(loc);
        let bytes = w.into_bytes();
        assert_eq!(Reader::new(&bytes, sizes).location().unwrap(), loc);
    }

    #[test]
    fn a_string_carries_its_length() {
        let mut w = Writer::new(IdSizes::default());
        w.string("Lcom/acme/Order;");
        let bytes = w.into_bytes();
        assert_eq!(&bytes[..4], &[0, 0, 0, 16]);
        assert_eq!(Reader::new(&bytes, IdSizes::default()).string().unwrap(), "Lcom/acme/Order;");
    }

    #[test]
    fn primitives_and_objects_are_told_apart_by_their_tag() {
        let sizes = IdSizes::default();
        let mut bytes = vec![b'I', 0, 0, 0, 42];
        assert_eq!(Reader::new(&bytes, sizes).value().unwrap(), Value::Int(42));

        bytes = vec![b's', 0, 0, 0, 0, 0, 0, 0, 9];
        let v = Reader::new(&bytes, sizes).value().unwrap();
        assert_eq!(v, Value::Object { tag: Tag::String, id: 9 });
        assert!(!v.is_null());

        bytes = vec![b'L', 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(Reader::new(&bytes, sizes).value().unwrap().is_null());
    }

    #[test]
    fn a_truncated_payload_is_an_error_not_a_panic() {
        let bytes = [0u8, 0, 0];
        let err = Reader::new(&bytes, IdSizes::default()).i32();
        assert!(matches!(err, Err(JdwpError::Protocol(_))));
    }

    #[test]
    fn a_double_survives_the_round_trip_bit_for_bit() {
        let mut w = Writer::new(IdSizes::default());
        w.i64(f64::to_bits(-0.5) as i64);
        let bytes = w.into_bytes();
        let v = Reader::new(&bytes, IdSizes::default()).value_of(Tag::Double).unwrap();
        assert_eq!(v, Value::Double(-0.5));
    }
}
