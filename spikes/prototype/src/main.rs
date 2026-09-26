//! T-110 — end-to-end prototype spike.
//!
//! Proves the crypto primitives from T-104/T-105 compose correctly end to end:
//! create -> import a 1 GiB file + a small "scan" item -> search -> close -> reopen
//! offline (re-derive keys from the password, not from anything cached) -> export
//! byte-identical -> wrong password rejected -> one corruption detected.
//!
//! Scope boundary (documented, not silently skipped): this does NOT implement the
//! exact `vault.pvh` header byte layout from `docs/format-spec.md` (that's real
//! `pv-format` work, gated behind GATE-R1). It uses the same key hierarchy contexts
//! and the same object-file chunk layout, stored in a throwaway sidecar file instead
//! of the real header. No PDFium/OCR integration either — the "scan" item is a
//! catalog row with searchable text, not an actual rendered/OCR'd PDF.

use libsodium_sys::*;
use rand::RngCore;
use rusqlite::Connection;
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const CHUNK_SIZE: usize = 65536;
const KDF_CTX_DBWRAP: &[u8; 8] = b"PVdbwrap";
const KDF_CTX_DKWRAP: &[u8; 8] = b"PVdkwrap";
const KDF_CTX_DIGEST: &[u8; 8] = b"PVdigest";

fn argon2id_derive(password: &str, salt: &[u8; 16], opslimit: u64, memlimit: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    let pw = CString::new(password).unwrap();
    let rc = unsafe {
        crypto_pwhash(
            out.as_mut_ptr(),
            out.len() as u64,
            pw.as_ptr(),
            password.len() as u64,
            salt.as_ptr(),
            opslimit,
            memlimit,
            crypto_pwhash_ALG_ARGON2ID13 as i32,
        )
    };
    assert_eq!(rc, 0, "Argon2id derivation failed (bad params?)");
    out
}

fn kdf_derive(root_key: &[u8; 32], ctx: &[u8; 8], subkey_id: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    let ctx_c = CString::new(ctx.to_vec()).unwrap();
    let rc = unsafe {
        crypto_kdf_derive_from_key(
            out.as_mut_ptr(),
            out.len(),
            subkey_id,
            ctx_c.as_ptr(),
            root_key.as_ptr(),
        )
    };
    assert_eq!(rc, 0, "crypto_kdf_derive_from_key failed");
    out
}

fn keyed_digest(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let rc = unsafe {
        crypto_generichash(
            out.as_mut_ptr(),
            out.len(),
            data.as_ptr(),
            data.len() as u64,
            key.as_ptr(),
            key.len(),
        )
    };
    assert_eq!(rc, 0, "crypto_generichash failed");
    out
}

/// Encrypts `src` into a `.pvo`-shaped object file at `dst`, per docs/format-spec.md §6:
/// magic "PVOB" · format_version u16 · object_id[16] · version_no u32 · secretstream header[24]
/// then chunks of up to 65536 plaintext bytes + 17-byte auth tag, last chunk tagged FINAL.
/// Associated data on every chunk: vault_id || object_id || version_no (SR-09).
fn encrypt_object(
    src: &Path,
    dst: &Path,
    data_key: &[u8; 32],
    vault_id: &[u8; 16],
    object_id: &[u8; 16],
    version_no: u32,
) -> std::io::Result<()> {
    let mut state = std::mem::MaybeUninit::<crypto_secretstream_xchacha20poly1305_state>::uninit();
    let mut header = [0u8; crypto_secretstream_xchacha20poly1305_HEADERBYTES as usize];
    let rc = unsafe {
        crypto_secretstream_xchacha20poly1305_init_push(
            state.as_mut_ptr(),
            header.as_mut_ptr(),
            data_key.as_ptr(),
        )
    };
    assert_eq!(rc, 0, "init_push failed");
    let mut state = unsafe { state.assume_init() };

    let mut ad = Vec::with_capacity(16 + 16 + 4);
    ad.extend_from_slice(vault_id);
    ad.extend_from_slice(object_id);
    ad.extend_from_slice(&version_no.to_le_bytes());

    let mut out = File::create(dst)?;
    out.write_all(b"PVOB")?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(object_id)?;
    out.write_all(&version_no.to_le_bytes())?;
    out.write_all(&header)?;

    let mut infile = File::open(src)?;
    let total_len = infile.metadata()?.len();
    let mut read_so_far: u64 = 0;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut cbuf = vec![0u8; CHUNK_SIZE + crypto_secretstream_xchacha20poly1305_ABYTES as usize];

    loop {
        let n = infile.read(&mut buf)?;
        read_so_far += n as u64;
        let is_final = read_so_far >= total_len;
        let tag = if is_final {
            crypto_secretstream_xchacha20poly1305_TAG_FINAL as u8
        } else {
            crypto_secretstream_xchacha20poly1305_TAG_MESSAGE as u8
        };
        let mut clen: u64 = 0;
        let rc = unsafe {
            crypto_secretstream_xchacha20poly1305_push(
                &mut state,
                cbuf.as_mut_ptr(),
                &mut clen,
                buf.as_ptr(),
                n as u64,
                ad.as_ptr(),
                ad.len() as u64,
                tag,
            )
        };
        assert_eq!(rc, 0, "secretstream push failed");
        out.write_all(&cbuf[..clen as usize])?;
        if is_final {
            break;
        }
    }
    Ok(())
}

#[derive(Debug)]
enum DecryptError {
    Auth(String),
    Io(std::io::Error),
}
impl From<std::io::Error> for DecryptError {
    fn from(e: std::io::Error) -> Self {
        DecryptError::Io(e)
    }
}

fn decrypt_object(
    src: &Path,
    dst: &Path,
    data_key: &[u8; 32],
    vault_id: &[u8; 16],
    version_no: u32,
) -> Result<(), DecryptError> {
    let mut infile = File::open(src)?;
    let mut magic = [0u8; 4];
    infile.read_exact(&mut magic)?;
    assert_eq!(&magic, b"PVOB");
    let mut u16buf = [0u8; 2];
    infile.read_exact(&mut u16buf)?;
    let mut object_id = [0u8; 16];
    infile.read_exact(&mut object_id)?;
    let mut u32buf = [0u8; 4];
    infile.read_exact(&mut u32buf)?;
    let file_version_no = u32::from_le_bytes(u32buf);
    assert_eq!(file_version_no, version_no);
    let mut header = [0u8; crypto_secretstream_xchacha20poly1305_HEADERBYTES as usize];
    infile.read_exact(&mut header)?;

    let mut state = std::mem::MaybeUninit::<crypto_secretstream_xchacha20poly1305_state>::uninit();
    let rc = unsafe {
        crypto_secretstream_xchacha20poly1305_init_pull(
            state.as_mut_ptr(),
            header.as_ptr(),
            data_key.as_ptr(),
        )
    };
    assert_eq!(rc, 0, "init_pull failed");
    let mut state = unsafe { state.assume_init() };

    let mut ad = Vec::with_capacity(16 + 16 + 4);
    ad.extend_from_slice(vault_id);
    ad.extend_from_slice(&object_id);
    ad.extend_from_slice(&version_no.to_le_bytes());

    let mut out = File::create(dst)?;
    let mut cbuf = vec![0u8; CHUNK_SIZE + crypto_secretstream_xchacha20poly1305_ABYTES as usize];
    let mut mbuf = vec![0u8; CHUNK_SIZE];
    let mut saw_final = false;

    loop {
        let n = infile.read(&mut cbuf)?;
        if n == 0 {
            break;
        }
        let mut mlen: u64 = 0;
        let mut tag: u8 = 0;
        let rc = unsafe {
            crypto_secretstream_xchacha20poly1305_pull(
                &mut state,
                mbuf.as_mut_ptr(),
                &mut mlen,
                &mut tag,
                cbuf.as_ptr(),
                n as u64,
                ad.as_ptr(),
                ad.len() as u64,
            )
        };
        if rc != 0 {
            return Err(DecryptError::Auth(format!(
                "authentication failed decrypting a chunk (rc={rc}) - tampered or wrong key"
            )));
        }
        out.write_all(&mbuf[..mlen as usize])?;
        if tag == crypto_secretstream_xchacha20poly1305_TAG_FINAL as u8 {
            saw_final = true;
        }
    }
    if !saw_final {
        return Err(DecryptError::Auth(
            "stream ended without a FINAL tag - truncated object".to_string(),
        ));
    }
    Ok(())
}

fn main() {
    assert!(unsafe { sodium_init() } >= 0, "sodium_init failed"); // 0 or 1 = ok, <0 = failure
    let work = std::env::temp_dir().join("pv-prototype-spike");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();

    println!("=== T-110 end-to-end prototype ===");
    println!("scratch dir: {}", work.display());

    // --- 1. Create: password -> Argon2id (T-104's recommended opslimit=20 @ 256 MiB) ---
    let password = "correct horse battery staple example passphrase";
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let opslimit = 20u64;
    let memlimit = 256usize * 1024 * 1024;

    let t0 = std::time::Instant::now();
    let root_key = argon2id_derive(password, &salt, opslimit, memlimit);
    println!("unlock (Argon2id opslimit=20 @ 256MiB): {:.3}s", t0.elapsed().as_secs_f64());

    let db_key = kdf_derive(&root_key, KDF_CTX_DBWRAP, 0);
    let mut vault_id = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut vault_id);

    // --- 2. Import a small "scan" item (catalog-searchable) + a 1 GiB file ---
    let catalog_path = work.join("catalog.pvdb");
    let conn = Connection::open(&catalog_path).unwrap();
    conn.pragma_update(None, "key", format!("x'{}'", hex(&db_key))).unwrap();
    conn.pragma_update(None, "cipher_memory_security", "ON").unwrap();
    conn.pragma_update(None, "temp_store", "MEMORY").unwrap();
    conn.pragma_update(None, "secure_delete", "ON").unwrap();
    conn.execute(
        "CREATE TABLE item (id INTEGER PRIMARY KEY, object_id BLOB, version_no INTEGER, title TEXT, note TEXT, byte_length INTEGER)",
        [],
    ).unwrap();
    conn.execute("CREATE VIRTUAL TABLE fts USING fts5(title, note)", []).unwrap();

    // scan item: a small synthetic "note", standing in for OCR'd text (T-108 already
    // proved OCR works; wiring OCR output into this catalog is future work, not this spike).
    let scan_object_id = random_id();
    let scan_key = kdf_derive(&root_key, KDF_CTX_DKWRAP, 1);
    let scan_src = work.join("scan_source.txt");
    std::fs::write(&scan_src, b"Invoice #A-2026-0913. Check phrase: quantum lighthouse cascade.").unwrap();
    let scan_obj = work.join(format!("{}.pvo", hex(&scan_object_id)));
    encrypt_object(&scan_src, &scan_obj, &scan_key, &vault_id, &scan_object_id, 1).unwrap();
    conn.execute(
        "INSERT INTO item (object_id, version_no, title, note, byte_length) VALUES (?1, 1, ?2, ?3, ?4)",
        rusqlite::params![scan_object_id.to_vec(), "Invoice A-2026-0913", "quantum lighthouse cascade", scan_src.metadata().unwrap().len() as i64],
    ).unwrap();
    conn.execute(
        "INSERT INTO fts (rowid, title, note) VALUES (last_insert_rowid(), ?1, ?2)",
        rusqlite::params!["Invoice A-2026-0913", "quantum lighthouse cascade"],
    ).unwrap();

    // the 1 GiB file
    println!("generating 1 GiB source file...");
    let big_object_id = random_id();
    let big_key = kdf_derive(&root_key, KDF_CTX_DKWRAP, 2);
    let big_src = work.join("big_source.bin");
    write_random_file(&big_src, 1024 * 1024 * 1024);
    let t1 = std::time::Instant::now();
    let big_obj = work.join(format!("{}.pvo", hex(&big_object_id)));
    encrypt_object(&big_src, &big_obj, &big_key, &vault_id, &big_object_id, 1).unwrap();
    println!("encrypted 1 GiB in {:.2}s", t1.elapsed().as_secs_f64());
    conn.execute(
        "INSERT INTO item (object_id, version_no, title, note, byte_length) VALUES (?1, 1, ?2, ?3, ?4)",
        rusqlite::params![big_object_id.to_vec(), "Large test file", "1 GiB random payload", 1024i64 * 1024 * 1024],
    ).unwrap();
    conn.execute(
        "INSERT INTO fts (rowid, title, note) VALUES (last_insert_rowid(), ?1, ?2)",
        rusqlite::params!["Large test file", "1 GiB random payload"],
    ).unwrap();

    let source_digest = keyed_digest(&kdf_derive(&root_key, KDF_CTX_DIGEST, 0), &std::fs::read(&big_src).unwrap());

    // --- 3. Search (before close) ---
    let hit: String = conn
        .query_row("SELECT title FROM fts WHERE fts MATCH 'lighthouse'", [], |r| r.get(0))
        .unwrap();
    println!("search 'lighthouse' -> {hit:?} (expect the invoice item)");
    assert_eq!(hit, "Invoice A-2026-0913");

    // --- 4. Close: drop everything, zero the in-memory keys ---
    drop(conn);
    let mut root_key = root_key;
    unsafe { sodium_memzero(root_key.as_mut_ptr() as *mut _, root_key.len()) };
    println!("closed. root key zeroed. scratch object/catalog files remain on disk (as ciphertext).");

    // --- 5. Reopen offline: re-derive from the password + stored salt, nothing cached ---
    let root_key2 = argon2id_derive(password, &salt, opslimit, memlimit);
    let db_key2 = kdf_derive(&root_key2, KDF_CTX_DBWRAP, 0);
    let conn2 = Connection::open(&catalog_path).unwrap();
    conn2.pragma_update(None, "key", format!("x'{}'", hex(&db_key2))).unwrap();
    let count: i64 = conn2.query_row("SELECT count(*) FROM item", [], |r| r.get(0)).unwrap();
    println!("reopened offline: {count} items visible (expect 2)");
    assert_eq!(count, 2);

    // --- 6. Export the 1 GiB object, verify byte-identical ---
    let big_key2 = kdf_derive(&root_key2, KDF_CTX_DKWRAP, 2);
    let export_path = work.join("big_export.bin");
    decrypt_object(&big_obj, &export_path, &big_key2, &vault_id, 1).unwrap();
    let export_digest = keyed_digest(&kdf_derive(&root_key2, KDF_CTX_DIGEST, 0), &std::fs::read(&export_path).unwrap());
    println!(
        "export digest matches source digest: {} (source={}, export={})",
        source_digest == export_digest,
        hex(&source_digest)[..16].to_string(),
        hex(&export_digest)[..16].to_string()
    );
    assert_eq!(source_digest, export_digest, "exported bytes must be identical to the source");
    let identical_bytes = files_identical(&big_src, &export_path);
    println!("byte-for-byte file comparison: {identical_bytes}");
    assert!(identical_bytes);

    // --- 7. Wrong password rejected ---
    let wrong_root = argon2id_derive("definitely the wrong passphrase", &salt, opslimit, memlimit);
    let wrong_key = kdf_derive(&wrong_root, KDF_CTX_DKWRAP, 2);
    let wrong_export = work.join("wrong_export.bin");
    match decrypt_object(&big_obj, &wrong_export, &wrong_key, &vault_id, 1) {
        Err(DecryptError::Auth(msg)) => println!("wrong password correctly rejected: {msg}"),
        other => panic!("expected an auth error decrypting with the wrong password, got {other:?}"),
    }

    // --- 8. Corruption detection: flip one byte in the middle of the object, expect failure ---
    corrupt_one_byte(&scan_obj);
    let corrupt_export = work.join("scan_export_corrupt.bin");
    match decrypt_object(&scan_obj, &corrupt_export, &scan_key, &vault_id, 1) {
        Err(DecryptError::Auth(msg)) => println!("corruption correctly detected: {msg}"),
        other => panic!("expected an auth error on a corrupted object, got {other:?}"),
    }

    println!("=== ALL CHECKS PASSED ===");
    std::fs::remove_dir_all(&work).ok();
}

fn random_id() -> [u8; 16] {
    let mut id = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut id);
    id
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn write_random_file(path: &Path, len: usize) {
    let mut f = File::create(path).unwrap();
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut written = 0usize;
    let mut rng = rand::thread_rng();
    while written < len {
        let take = CHUNK_SIZE.min(len - written);
        rng.fill_bytes(&mut buf[..take]);
        f.write_all(&buf[..take]).unwrap();
        written += take;
    }
}

fn files_identical(a: &Path, b: &Path) -> bool {
    let (mut fa, mut fb) = (File::open(a).unwrap(), File::open(b).unwrap());
    if fa.metadata().unwrap().len() != fb.metadata().unwrap().len() {
        return false;
    }
    let (mut ba, mut bb) = (vec![0u8; CHUNK_SIZE], vec![0u8; CHUNK_SIZE]);
    loop {
        let na = fa.read(&mut ba).unwrap();
        let nb = fb.read(&mut bb).unwrap();
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

fn corrupt_one_byte(path: &PathBuf) {
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(path).unwrap();
    let len = f.metadata().unwrap().len();
    let mid = len / 2;
    f.seek(SeekFrom::Start(mid)).unwrap();
    let mut b = [0u8; 1];
    f.read_exact(&mut b).unwrap();
    f.seek(SeekFrom::Start(mid)).unwrap();
    f.write_all(&[b[0] ^ 0xFF]).unwrap();
}
