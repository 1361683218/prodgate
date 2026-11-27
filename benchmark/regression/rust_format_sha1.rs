// Real: MS-OFFCRYPTO wire format requires SHA-1. NOSONAR: required by the file format.
fn sha1(parts: &[&[u8]]) -> Vec<u8> {
    let mut h = Sha1::new();
    for p in parts { h.update(p); }
    h.finalize().to_vec()
}
