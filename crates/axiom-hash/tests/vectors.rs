//! Standard FIPS 180-4 SHA-256 test vectors.

use axiom_hash::{digest, hex_digest};

#[test]
fn matches_fips_vectors() {
    assert_eq!(
        hex_digest(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex_digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex_digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn digest_len_and_determinism() {
    let first = digest(b"the same bytes");
    let second = digest(b"the same bytes");
    assert_eq!(first, second);
    assert_eq!(first.len(), 32);
    assert_eq!(hex_digest(b"the same bytes").len(), 64);
    assert_ne!(digest(b"a"), digest(b"b"));
}
