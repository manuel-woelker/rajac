/// Hashes a string using the rapidhash algorithm.
pub fn hash_string(s: &str) -> u64 {
    hash_bytes(s.as_bytes())
}

/// Hashes a byte array using the rapidhash algorithm.
pub fn hash_bytes(bytes: &[u8]) -> u64 {
    rapidhash::v3::rapidhash_v3(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_string_matches_hash_bytes_for_utf8_input() {
        let input = "rajac-λ";

        assert_eq!(hash_string(input), hash_bytes(input.as_bytes()));
    }

    #[test]
    fn hash_empty_string_matches_empty_bytes() {
        assert_eq!(hash_string(""), hash_bytes(&[]));
    }

    #[test]
    fn hash_changes_for_different_inputs() {
        let hash_a = hash_string("alpha");
        let hash_b = hash_string("beta");

        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn hash_string_matches_expected_concrete_values() {
        assert_eq!(hash_string(""), 232177599295442350);
        assert_eq!(hash_string("alpha"), 6106490483247698475);
        assert_eq!(hash_string("beta"), 9996705554587609234);
        assert_eq!(hash_string("lambda"), 4005924631819820944);
    }

    #[test]
    fn hash_bytes_matches_expected_concrete_values() {
        assert_eq!(hash_bytes(&[]), 232177599295442350);
        assert_eq!(hash_bytes(b"alpha"), 6106490483247698475);
    }
}
