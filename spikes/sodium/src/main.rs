use libsodium_sys::*;
use std::time::Instant;

fn main() {
    unsafe { assert_eq!(sodium_init(), 0); }
    let salt = [7u8; 16];
    let pw = b"correct horse battery staple example passphrase\0";
    let mut out = [0u8; 32];
    for ops in [12u64, 20] {
        let t0 = Instant::now();
        let rc = unsafe {
            crypto_pwhash(
                out.as_mut_ptr(), 32,
                pw.as_ptr() as *const i8, (pw.len()-1) as u64,
                salt.as_ptr(),
                ops, 256*1024*1024,
                crypto_pwhash_ALG_ARGON2ID13 as i32,
            )
        };
        assert_eq!(rc, 0);
        println!("opslimit={ops} -> {:.3}s (via libsodium-sys-stable's own build)", t0.elapsed().as_secs_f64());
    }
}
