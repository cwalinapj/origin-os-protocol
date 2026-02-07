#![no_std]

/// Compare two byte slices in constant time.
#[inline]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result: u8 = 0;
    for (&lhs, &rhs) in a.iter().zip(b.iter()) {
        result |= lhs ^ rhs;
    }

    result == 0
}

/// Constant-time equality check for 32-byte arrays.
#[inline]
pub fn constant_time_eq_32(a: &[u8; 32], b: &[u8; 32]) -> bool {
    constant_time_eq(a, b)
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;
    use super::constant_time_eq_32;

    #[test]
    fn eq_matches() {
        assert!(constant_time_eq(b"abc", b"abc"));
    }

    #[test]
    fn eq_mismatch_length() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn eq_mismatch_contents() {
        assert!(!constant_time_eq(b"abc", b"abd"));
    }

    #[test]
    fn eq_32() {
        let a = [1u8; 32];
        let mut b = [1u8; 32];
        assert!(constant_time_eq_32(&a, &b));
        b[0] = 2;
        assert!(!constant_time_eq_32(&a, &b));
    }
}
