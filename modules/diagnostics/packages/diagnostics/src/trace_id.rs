use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TRACE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SPAN_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId(u128);

impl TraceId {
    pub fn generate() -> Self {
        let high = NEXT_TRACE_ID.fetch_add(1, Ordering::Relaxed) as u128;
        let low = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()) as u128;
        Self((high << 64) | low)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.len() > 32 {
            return None;
        }
        u128::from_str_radix(trimmed, 16).ok().map(Self)
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        format!("{:032x}", self.0)
    }

    #[must_use]
    pub const fn as_u128(&self) -> u128 {
        self.0
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::generate()
    }
}

impl Display for TraceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId(u64);

impl SpanId {
    pub fn generate() -> Self {
        Self(NEXT_SPAN_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.len() > 16 {
            return None;
        }
        u64::from_str_radix(trimmed, 16).ok().map(Self)
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        format!("{:016x}", self.0)
    }

    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::generate()
    }
}

impl Display for SpanId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_generates_unique() {
        let id1 = TraceId::generate();
        let id2 = TraceId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn trace_id_hex_roundtrip() {
        let id = TraceId::generate();
        let hex = id.to_hex();
        let parsed = TraceId::from_hex(&hex).expect("parse");
        assert_eq!(id, parsed);
        assert_eq!(hex.len(), 32);
    }

    #[test]
    fn span_id_generates_unique() {
        let id1 = SpanId::generate();
        let id2 = SpanId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn span_id_hex_roundtrip() {
        let id = SpanId::generate();
        let hex = id.to_hex();
        let parsed = SpanId::from_hex(&hex).expect("parse");
        assert_eq!(id, parsed);
        assert_eq!(hex.len(), 16);
    }

    #[test]
    fn trace_id_display_format() {
        let id = TraceId::from_hex("abc123").expect("parse");
        let display = format!("{}", id);
        assert_eq!(display, "00000000000000000000000000abc123");
    }
}
