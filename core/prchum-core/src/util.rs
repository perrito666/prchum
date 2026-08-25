//! Small dependency-free utilities: stable hashing, ids, timestamps.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// FNV-1a 64-bit. Chosen over a crypto hash to stay dependency-free; these
/// hashes identify content (anchors, source keys), they don't protect it.
/// Stable across platforms and releases, which persistence requires.
pub fn fnv64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Hex form of [`fnv64`], 16 characters.
pub fn fnv64_hex(data: &[u8]) -> String {
    format!("{:016x}", fnv64(data))
}

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh 16-hex local id, unique within and across processes in practice
/// (wall clock nanos mixed with a process counter).
pub fn new_local_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let count = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id() as u64;
    fnv64_hex(&[nanos.to_le_bytes(), count.to_le_bytes(), pid.to_le_bytes()].concat())
}

/// The current time as RFC 3339 UTC (`2026-01-02T03:04:05Z`).
///
/// RFC 3339 lexical order is chronological order, which the comment sorting
/// relies on.
pub fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rfc3339_from_unix(secs)
}

/// Converts a unix timestamp to RFC 3339 UTC (Howard Hinnant's civil-date
/// algorithm, exact for the proleptic Gregorian calendar).
pub fn rfc3339_from_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv64_hex(b""), "cbf29ce484222325");
        assert_eq!(fnv64_hex(b"hello"), "a430d84680aabd0b");
    }

    #[test]
    fn ids_are_unique_and_16_hex() {
        let a = new_local_id();
        let b = new_local_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn rfc3339_known_values() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_unix(951_786_245), "2000-02-29T01:04:05Z");
        assert_eq!(rfc3339_from_unix(1_767_225_599), "2025-12-31T23:59:59Z");
    }
}
