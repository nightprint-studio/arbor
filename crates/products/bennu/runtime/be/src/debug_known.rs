//! JDK value types, rendered from their fields.
//!
//! `debug_value` never calls `toString()` in the debugged VM (see its header): an object reads as
//! `BigDecimal@2e`, which is true and useless — a logpoint that prints an order total prints a handle.
//! Most of what a program logs, though, is a small set of JDK types whose state is a few primitive
//! fields, and a value can be *computed* from those fields here, with no call into the program and so
//! no lock, no side effect and no exception to fear. `BigDecimal` is `intCompact` and `scale`;
//! `LocalDate` is three numbers; an enum is its `name`.
//!
//! Anything not listed, or any read that fails or finds the field missing (another JDK's layout), answers
//! `None` and the caller falls back to `Type@id`. The same renderer feeds the variables panel, the
//! watches, the logpoints and the assistant, so a value reads one way everywhere.
//!
//! The layouts are those of JDK 17 and 21 (`BigDecimal.intCompact`, `Optional.value`, …).

use bennu_jdwp::prelude::{Id, Value};

/// How deep a known value is followed into another (`ZonedDateTime` → `LocalDateTime` → `LocalDate`).
const MAX_DEPTH: u8 = 4;
/// The most 32-bit words of a `BigInteger` turned into decimal; past it the number is not shown.
const MAX_MAG_WORDS: usize = 64;
/// A `BigDecimal` scale beyond this is not padded out: `1E-1000000` is not worth a million zeros.
const MAX_SCALE: i64 = 1_000;

/// What the renderer needs to read from the debugged heap. Implemented over a live session by
/// `debug_value`, and by a map in the tests.
pub(crate) trait Heap {
    /// The fully qualified class of an object (`java.math.BigDecimal`).
    fn class_of(&self, id: Id) -> Option<String>;
    /// Whether the object's class is `class` or has it among its superclasses.
    fn is_a(&self, id: Id, class: &str) -> bool;
    /// An instance field, declared by the class or inherited.
    fn field(&self, id: Id, name: &str) -> Option<Value>;
    fn string(&self, id: Id) -> Option<String>;
    /// The elements of an `int[]` / `byte[]`, or `None` if it is longer than `max`.
    fn ints(&self, id: Id, max: usize) -> Option<Vec<i64>>;
    /// Any value as the text a row shows, one level deeper.
    fn show(&self, value: Value, depth: u8) -> String;
}

/// The text of the object `id` if it is a type this module knows how to read.
pub(crate) fn render_known(heap: &dyn Heap, id: Id, depth: u8) -> Option<String> {
    if id == 0 || depth > MAX_DEPTH {
        return None;
    }
    let class = heap.class_of(id)?;
    match class.as_str() {
        "java.lang.Integer" | "java.lang.Long" | "java.lang.Short" | "java.lang.Byte" | "java.lang.Boolean" | "java.lang.Character" | "java.lang.Float" | "java.lang.Double" => {
            Some(heap.show(heap.field(id, "value")?, depth + 1))
        }
        "java.math.BigDecimal" => big_decimal(heap, id),
        "java.math.BigInteger" => big_integer(heap, id),
        "java.util.UUID" => uuid(heap, id),
        "java.util.Optional" => Some(match heap.field(id, "value")? {
            Value::Object { id: 0, .. } => "Optional.empty".to_string(),
            inner => format!("Optional[{}]", heap.show(inner, depth + 1)),
        }),
        "java.util.concurrent.atomic.AtomicInteger" | "java.util.concurrent.atomic.AtomicLong" => Some(format!("{}({})", simple(&class), num(heap, id, "value")?)),
        "java.util.concurrent.atomic.AtomicBoolean" => Some(format!("AtomicBoolean({})", num(heap, id, "value")? != 0)),
        "java.time.LocalDate" => local_date(heap, id),
        "java.time.LocalTime" => local_time(heap, id),
        "java.time.LocalDateTime" => local_date_time(heap, id),
        "java.time.OffsetDateTime" => Some(format!("{}{}", local_date_time(heap, object(heap, id, "dateTime")?)?, offset(heap, object(heap, id, "offset")?)?)),
        "java.time.OffsetTime" => Some(format!("{}{}", local_time(heap, object(heap, id, "time")?)?, offset(heap, object(heap, id, "offset")?)?)),
        "java.time.ZonedDateTime" => zoned_date_time(heap, id),
        "java.time.ZoneOffset" => offset(heap, id),
        "java.time.Instant" => Some(epoch_text(num(heap, id, "seconds")?, num(heap, id, "nanos")?)),
        "java.time.Duration" => duration(heap, id),
        "java.net.URI" => text(heap, id, "string"),
        "java.io.File" => text(heap, id, "path"),
        "java.util.Currency" => text(heap, id, "currencyCode"),
        _ => by_ancestry(heap, id, &class),
    }
}

/// The types recognised by what they extend rather than by their exact name: enums (every constant is its
/// own anonymous class when it has a body), dates (`java.sql.Timestamp` is a `Date`), collections.
fn by_ancestry(heap: &dyn Heap, id: Id, class: &str) -> Option<String> {
    if heap.is_a(id, "java.lang.Enum") {
        return text(heap, id, "name");
    }
    if heap.is_a(id, "java.util.Date") {
        let ms = num(heap, id, "fastTime")?;
        return Some(epoch_text(ms.div_euclid(1000), ms.rem_euclid(1000) * 1_000_000));
    }
    let sized = |holder: Id| Some(format!("{}(size={})", simple(class), num(heap, holder, "size")?));
    if ["java.util.ArrayList", "java.util.LinkedList", "java.util.HashMap", "java.util.TreeMap"].iter().any(|c| heap.is_a(id, c)) {
        return sized(id);
    }
    // A set is a map with a facade: its size is its backing map's.
    for (set, backing) in [("java.util.HashSet", "map"), ("java.util.TreeSet", "m")] {
        if heap.is_a(id, set) {
            return sized(object(heap, id, backing)?);
        }
    }
    None
}

// ── field readers ─────────────────────────────────────────────────────────────────

fn num(heap: &dyn Heap, id: Id, name: &str) -> Option<i64> {
    match heap.field(id, name)? {
        Value::Int(v) => Some(i64::from(v)),
        Value::Long(v) => Some(v),
        Value::Short(v) => Some(i64::from(v)),
        Value::Byte(v) => Some(i64::from(v)),
        _ => None,
    }
}

fn object(heap: &dyn Heap, id: Id, name: &str) -> Option<Id> {
    match heap.field(id, name)? {
        Value::Object { id, .. } if id != 0 => Some(id),
        _ => None,
    }
}

fn text(heap: &dyn Heap, id: Id, name: &str) -> Option<String> {
    heap.string(object(heap, id, name)?)
}

fn simple(class: &str) -> &str {
    class.rsplit(['.', '$']).next().unwrap_or(class)
}

// ── numbers ───────────────────────────────────────────────────────────────────────

fn big_decimal(heap: &dyn Heap, id: Id) -> Option<String> {
    let scale = num(heap, id, "scale")?;
    if scale.abs() > MAX_SCALE {
        return None;
    }
    // `Long.MIN_VALUE` marks a value too big for a long: the digits are in `intVal`.
    let compact = num(heap, id, "intCompact")?;
    let unscaled = if compact == i64::MIN { big_integer(heap, object(heap, id, "intVal")?)? } else { compact.to_string() };
    Some(place_point(&unscaled, scale))
}

/// `12345` with scale 2 → `123.45`; scale 0 → as is; negative scale → `1E+3`.
fn place_point(unscaled: &str, scale: i64) -> String {
    if scale == 0 {
        return unscaled.to_string();
    }
    let (sign, digits) = match unscaled.strip_prefix('-') {
        Some(d) => ("-", d),
        None => ("", unscaled),
    };
    if scale < 0 {
        return format!("{sign}{digits}E+{}", -scale);
    }
    let scale = scale as usize;
    let padded = if digits.len() <= scale { format!("{}{digits}", "0".repeat(scale + 1 - digits.len())) } else { digits.to_string() };
    let cut = padded.len() - scale;
    format!("{sign}{}.{}", &padded[..cut], &padded[cut..])
}

fn big_integer(heap: &dyn Heap, id: Id) -> Option<String> {
    let signum = num(heap, id, "signum")?;
    let words = heap.ints(object(heap, id, "mag")?, MAX_MAG_WORDS)?;
    Some(decimal(signum, &words))
}

/// A sign and a big-endian magnitude of 32-bit words, as decimal digits.
fn decimal(signum: i64, mag: &[i64]) -> String {
    let mut limbs: Vec<u32> = mag.iter().map(|w| *w as u32).collect();
    let mut groups: Vec<u32> = Vec::new();
    while limbs.iter().any(|l| *l != 0) {
        let mut rest: u64 = 0;
        for limb in limbs.iter_mut() {
            let current = (rest << 32) | u64::from(*limb);
            *limb = (current / 1_000_000_000) as u32;
            rest = current % 1_000_000_000;
        }
        groups.push(rest as u32);
    }
    let Some((top, lower)) = groups.split_last() else { return "0".to_string() };
    let mut out = String::new();
    if signum < 0 {
        out.push('-');
    }
    out.push_str(&top.to_string());
    for group in lower.iter().rev() {
        out.push_str(&format!("{group:09}"));
    }
    out
}

fn uuid(heap: &dyn Heap, id: Id) -> Option<String> {
    let high = num(heap, id, "mostSigBits")? as u64;
    let low = num(heap, id, "leastSigBits")? as u64;
    Some(format!("{:08x}-{:04x}-{:04x}-{:04x}-{:012x}", high >> 32, (high >> 16) & 0xffff, high & 0xffff, low >> 48, low & 0xffff_ffff_ffff))
}

// ── time ──────────────────────────────────────────────────────────────────────────

fn local_date(heap: &dyn Heap, id: Id) -> Option<String> {
    Some(format!("{:04}-{:02}-{:02}", num(heap, id, "year")?, num(heap, id, "month")?, num(heap, id, "day")?))
}

fn local_time(heap: &dyn Heap, id: Id) -> Option<String> {
    let (hour, minute, second, nano) = (num(heap, id, "hour")?, num(heap, id, "minute")?, num(heap, id, "second")?, num(heap, id, "nano")?);
    // As `LocalTime.toString` does: the seconds only when there are some.
    let mut out = format!("{hour:02}:{minute:02}");
    if second > 0 || nano > 0 {
        out.push_str(&format!(":{second:02}"));
        if nano > 0 {
            out.push_str(&fraction(nano));
        }
    }
    Some(out)
}

fn local_date_time(heap: &dyn Heap, id: Id) -> Option<String> {
    Some(format!("{}T{}", local_date(heap, object(heap, id, "date")?)?, local_time(heap, object(heap, id, "time")?)?))
}

/// `Z`, `+01:00`, `-05:30`: the zone offset's own cached id, else computed from its seconds.
fn offset(heap: &dyn Heap, id: Id) -> Option<String> {
    if let Some(cached) = text(heap, id, "id") {
        return Some(cached);
    }
    let total = num(heap, id, "totalSeconds")?;
    if total == 0 {
        return Some("Z".to_string());
    }
    let (sign, abs) = if total < 0 { ('-', -total) } else { ('+', total) };
    let mut out = format!("{sign}{:02}:{:02}", abs / 3600, abs % 3600 / 60);
    if abs % 60 != 0 {
        out.push_str(&format!(":{:02}", abs % 60));
    }
    Some(out)
}

fn zoned_date_time(heap: &dyn Heap, id: Id) -> Option<String> {
    let at = local_date_time(heap, object(heap, id, "dateTime")?)?;
    let shift = offset(heap, object(heap, id, "offset")?)?;
    // A zone that is only an offset is not repeated: `…+01:00`, not `…+01:00[+01:00]`.
    let zone = object(heap, id, "zone")?;
    let region = if heap.class_of(zone).as_deref() == Some("java.time.ZoneOffset") { String::new() } else { format!("[{}]", text(heap, zone, "id")?) };
    Some(format!("{at}{shift}{region}"))
}

/// `.123`, `.123456` or `.123456789`: the shortest of the three that loses nothing.
fn fraction(nano: i64) -> String {
    if nano % 1_000_000 == 0 {
        format!(".{:03}", nano / 1_000_000)
    } else if nano % 1_000 == 0 {
        format!(".{:06}", nano / 1_000)
    } else {
        format!(".{nano:09}")
    }
}

/// `2023-11-14T22:13:20Z`, as `Instant.toString`.
fn epoch_text(seconds: i64, nanos: i64) -> String {
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let in_day = seconds.rem_euclid(86_400);
    let mut out = format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}", in_day / 3600, in_day % 3600 / 60, in_day % 60);
    if nanos > 0 {
        out.push_str(&fraction(nanos));
    }
    out.push('Z');
    out
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

/// `PT1H30M`, `PT0.5S`. A negative duration is left to the generic row.
fn duration(heap: &dyn Heap, id: Id) -> Option<String> {
    let (seconds, nanos) = (num(heap, id, "seconds")?, num(heap, id, "nanos")?);
    if seconds < 0 || nanos < 0 {
        return None;
    }
    if seconds == 0 && nanos == 0 {
        return Some("PT0S".to_string());
    }
    let mut out = String::from("PT");
    if seconds >= 3600 {
        out.push_str(&format!("{}H", seconds / 3600));
    }
    if seconds % 3600 >= 60 {
        out.push_str(&format!("{}M", seconds % 3600 / 60));
    }
    if seconds % 60 > 0 || nanos > 0 {
        out.push_str(&(seconds % 60).to_string());
        if nanos > 0 {
            out.push_str(format!(".{nanos:09}").trim_end_matches('0'));
        }
        out.push('S');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_jdwp::prelude::Tag;
    use std::collections::HashMap;

    /// A heap in a map: `(class, superclasses)` per object, fields by `(object, name)`.
    #[derive(Default)]
    struct Fake {
        classes: HashMap<Id, (String, Vec<String>)>,
        fields: HashMap<(Id, String), Value>,
        strings: HashMap<Id, String>,
        arrays: HashMap<Id, Vec<i64>>,
        next: Id,
    }

    impl Heap for Fake {
        fn class_of(&self, id: Id) -> Option<String> {
            self.classes.get(&id).map(|c| c.0.clone())
        }
        fn is_a(&self, id: Id, class: &str) -> bool {
            self.classes.get(&id).is_some_and(|(c, supers)| c == class || supers.iter().any(|s| s == class))
        }
        fn field(&self, id: Id, name: &str) -> Option<Value> {
            self.fields.get(&(id, name.to_string())).copied()
        }
        fn string(&self, id: Id) -> Option<String> {
            self.strings.get(&id).cloned()
        }
        fn ints(&self, id: Id, max: usize) -> Option<Vec<i64>> {
            self.arrays.get(&id).filter(|a| a.len() <= max).cloned()
        }
        fn show(&self, value: Value, depth: u8) -> String {
            match value {
                Value::Int(v) => v.to_string(),
                Value::Long(v) => v.to_string(),
                Value::Boolean(v) => v.to_string(),
                Value::Object { id, .. } => render_known(self, id, depth).unwrap_or_else(|| format!("obj@{id:x}")),
                other => format!("{other:?}"),
            }
        }
    }

    impl Fake {
        fn object(&mut self, class: &str, supers: &[&str]) -> Id {
            self.next += 1;
            self.classes.insert(self.next, (class.to_string(), supers.iter().map(|s| s.to_string()).collect()));
            self.next
        }
        fn set(&mut self, id: Id, name: &str, value: Value) {
            self.fields.insert((id, name.to_string()), value);
        }
        fn int(&mut self, id: Id, name: &str, v: i32) {
            self.set(id, name, Value::Int(v));
        }
        fn long(&mut self, id: Id, name: &str, v: i64) {
            self.set(id, name, Value::Long(v));
        }
        fn link(&mut self, id: Id, name: &str, target: Id) {
            self.set(id, name, Value::Object { tag: Tag::Object, id: target });
        }
        fn word(&mut self, id: Id, name: &str, s: &str) {
            self.next += 1;
            self.strings.insert(self.next, s.to_string());
            let target = self.next;
            self.link(id, name, target);
        }
        fn date(&mut self, y: i32, m: i32, d: i32) -> Id {
            let id = self.object("java.time.LocalDate", &[]);
            self.int(id, "year", y);
            self.int(id, "month", m);
            self.int(id, "day", d);
            id
        }
        fn time(&mut self, h: i32, m: i32, s: i32, nano: i32) -> Id {
            let id = self.object("java.time.LocalTime", &[]);
            self.int(id, "hour", h);
            self.int(id, "minute", m);
            self.int(id, "second", s);
            self.int(id, "nano", nano);
            id
        }
        fn zone_offset(&mut self, seconds: i32) -> Id {
            let id = self.object("java.time.ZoneOffset", &[]);
            self.int(id, "totalSeconds", seconds);
            id
        }
        fn date_time(&mut self, date: Id, time: Id) -> Id {
            let id = self.object("java.time.LocalDateTime", &[]);
            self.link(id, "date", date);
            self.link(id, "time", time);
            id
        }
        fn big_decimal(&mut self, compact: i64, scale: i32) -> Id {
            let id = self.object("java.math.BigDecimal", &[]);
            self.long(id, "intCompact", compact);
            self.int(id, "scale", scale);
            id
        }
    }

    fn shown(heap: &Fake, id: Id) -> Option<String> {
        render_known(heap, id, 0)
    }

    #[test]
    fn a_big_decimal_reads_from_its_compact_value_and_scale() {
        let mut h = Fake::default();
        for (compact, scale, want) in [(12345, 2, "123.45"), (5, 3, "0.005"), (-5, 3, "-0.005"), (100, 0, "100"), (0, 2, "0.00"), (7, -3, "7E+3")] {
            let id = h.big_decimal(compact, scale);
            assert_eq!(shown(&h, id).as_deref(), Some(want), "{compact} scale {scale}");
        }
        let absurd = h.big_decimal(1, 5_000);
        assert_eq!(shown(&h, absurd), None, "a huge scale is not padded out");
    }

    #[test]
    fn a_big_decimal_too_big_for_a_long_reads_its_big_integer() {
        let mut h = Fake::default();
        let mag = h.object("[I", &[]);
        h.arrays.insert(mag, vec![1, 0]); // 2^32
        let digits = h.object("java.math.BigInteger", &[]);
        h.int(digits, "signum", -1);
        h.link(digits, "mag", mag);
        let id = h.big_decimal(i64::MIN, 2);
        h.link(id, "intVal", digits);
        assert_eq!(shown(&h, id).as_deref(), Some("-42949672.96"));
        assert_eq!(shown(&h, digits).as_deref(), Some("-4294967296"));
    }

    #[test]
    fn a_big_integer_across_several_groups_keeps_its_zeros() {
        // 10^18 = 0x0DE0B6B3A7640000
        assert_eq!(decimal(1, &[0x0DE0_B6B3, i64::from(0xA764_0000_u32)]), "1000000000000000000");
        assert_eq!(decimal(1, &[]), "0");
        assert_eq!(decimal(0, &[0]), "0");
        assert_eq!(decimal(1, &[-1]), "4294967295", "a word with the top bit set is unsigned");
    }

    #[test]
    fn boxes_and_enums_and_ids_read_as_their_value() {
        let mut h = Fake::default();
        let int = h.object("java.lang.Integer", &[]);
        h.int(int, "value", 42);
        assert_eq!(shown(&h, int).as_deref(), Some("42"));

        let constant = h.object("com.acme.Color$1", &["com.acme.Color", "java.lang.Enum"]);
        h.word(constant, "name", "RED");
        assert_eq!(shown(&h, constant).as_deref(), Some("RED"), "a constant with a body is a subclass, found by ancestry");

        let id = h.object("java.util.UUID", &[]);
        h.long(id, "mostSigBits", 0x123e_4567_e89b_12d3);
        h.long(id, "leastSigBits", i64::from_be_bytes(0xa456_4266_1417_4000_u64.to_be_bytes()));
        assert_eq!(shown(&h, id).as_deref(), Some("123e4567-e89b-12d3-a456-426614174000"));
    }

    #[test]
    fn dates_and_times_read_the_way_java_prints_them() {
        let mut h = Fake::default();
        let date = h.date(2024, 3, 7);
        assert_eq!(shown(&h, date).as_deref(), Some("2024-03-07"));
        let noon = h.time(12, 30, 0, 0);
        assert_eq!(shown(&h, noon).as_deref(), Some("12:30"), "no seconds when there are none");
        let precise = h.time(9, 5, 7, 250_000_000);
        assert_eq!(shown(&h, precise).as_deref(), Some("09:05:07.250"));
        let micro = h.time(9, 5, 7, 123_456_000);
        assert_eq!(shown(&h, micro).as_deref(), Some("09:05:07.123456"));

        let stamp = h.date_time(date, precise);
        assert_eq!(shown(&h, stamp).as_deref(), Some("2024-03-07T09:05:07.250"));

        let plus_one = h.zone_offset(3600);
        let offset_stamp = h.object("java.time.OffsetDateTime", &[]);
        h.link(offset_stamp, "dateTime", stamp);
        h.link(offset_stamp, "offset", plus_one);
        assert_eq!(shown(&h, offset_stamp).as_deref(), Some("2024-03-07T09:05:07.250+01:00"));

        let utc = h.zone_offset(0);
        let utc_stamp = h.object("java.time.OffsetDateTime", &[]);
        h.link(utc_stamp, "dateTime", stamp);
        h.link(utc_stamp, "offset", utc);
        assert_eq!(shown(&h, utc_stamp).as_deref(), Some("2024-03-07T09:05:07.250Z"));

        let india = h.zone_offset(-(5 * 3600 + 30 * 60));
        assert_eq!(shown(&h, india).as_deref(), Some("-05:30"));
    }

    #[test]
    fn a_zoned_date_time_names_its_region_but_not_a_bare_offset() {
        let mut h = Fake::default();
        let (date, time) = (h.date(2024, 1, 1), h.time(10, 0, 0, 0));
        let stamp = h.date_time(date, time);
        let plus_one = h.zone_offset(3600);
        let region = h.object("java.time.ZoneRegion", &["java.time.ZoneId"]);
        h.word(region, "id", "Europe/Rome");

        let rome = h.object("java.time.ZonedDateTime", &[]);
        h.link(rome, "dateTime", stamp);
        h.link(rome, "offset", plus_one);
        h.link(rome, "zone", region);
        assert_eq!(shown(&h, rome).as_deref(), Some("2024-01-01T10:00+01:00[Europe/Rome]"));

        let bare = h.object("java.time.ZonedDateTime", &[]);
        h.link(bare, "dateTime", stamp);
        h.link(bare, "offset", plus_one);
        h.link(bare, "zone", plus_one);
        assert_eq!(shown(&h, bare).as_deref(), Some("2024-01-01T10:00+01:00"));
    }

    #[test]
    fn an_instant_and_a_legacy_date_are_the_same_moment() {
        let mut h = Fake::default();
        let instant = h.object("java.time.Instant", &[]);
        h.long(instant, "seconds", 1_700_000_000);
        h.int(instant, "nanos", 0);
        assert_eq!(shown(&h, instant).as_deref(), Some("2023-11-14T22:13:20Z"));

        let epoch = h.object("java.time.Instant", &[]);
        h.long(epoch, "seconds", 0);
        h.int(epoch, "nanos", 500_000_000);
        assert_eq!(shown(&h, epoch).as_deref(), Some("1970-01-01T00:00:00.500Z"));

        let before = h.object("java.time.Instant", &[]);
        h.long(before, "seconds", -1);
        h.int(before, "nanos", 0);
        assert_eq!(shown(&h, before).as_deref(), Some("1969-12-31T23:59:59Z"));

        let timestamp = h.object("java.sql.Timestamp", &["java.util.Date"]);
        h.long(timestamp, "fastTime", 1_700_000_000_250);
        assert_eq!(shown(&h, timestamp).as_deref(), Some("2023-11-14T22:13:20.250Z"));
    }

    #[test]
    fn a_duration_reads_as_iso() {
        let mut h = Fake::default();
        for (seconds, nanos, want) in [(0, 0, "PT0S"), (5400, 0, "PT1H30M"), (1, 500_000_000, "PT1.5S"), (3661, 0, "PT1H1M1S")] {
            let id = h.object("java.time.Duration", &[]);
            h.long(id, "seconds", seconds);
            h.int(id, "nanos", nanos);
            assert_eq!(shown(&h, id).as_deref(), Some(want));
        }
        let negative = h.object("java.time.Duration", &[]);
        h.long(negative, "seconds", -1);
        h.int(negative, "nanos", 0);
        assert_eq!(shown(&h, negative), None);
    }

    #[test]
    fn collections_show_their_size_and_sets_their_maps() {
        let mut h = Fake::default();
        let list = h.object("java.util.ArrayList", &["java.util.AbstractList"]);
        h.int(list, "size", 3);
        assert_eq!(shown(&h, list).as_deref(), Some("ArrayList(size=3)"));

        let linked = h.object("java.util.LinkedHashMap", &["java.util.HashMap"]);
        h.int(linked, "size", 0);
        assert_eq!(shown(&h, linked).as_deref(), Some("LinkedHashMap(size=0)"), "a subclass is found by ancestry");

        let map = h.object("java.util.HashMap", &[]);
        h.int(map, "size", 2);
        let set = h.object("java.util.HashSet", &[]);
        h.link(set, "map", map);
        assert_eq!(shown(&h, set).as_deref(), Some("HashSet(size=2)"));
    }

    #[test]
    fn what_is_not_known_or_not_readable_is_left_to_the_generic_row() {
        let mut h = Fake::default();
        let order = h.object("com.acme.Order", &[]);
        assert_eq!(shown(&h, order), None);
        assert_eq!(shown(&h, 0), None);
        let broken = h.object("java.math.BigDecimal", &[]);
        assert_eq!(shown(&h, broken), None, "another JDK's layout answers None, not a wrong number");
        let empty = h.object("java.util.Optional", &[]);
        h.set(empty, "value", Value::Object { tag: Tag::Object, id: 0 });
        assert_eq!(shown(&h, empty).as_deref(), Some("Optional.empty"));
    }

    #[test]
    fn civil_dates_cross_leap_years() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}
