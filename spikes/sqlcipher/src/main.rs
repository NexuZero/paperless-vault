use rusqlite::Connection;

fn main() -> rusqlite::Result<()> {
    let db_path = std::env::temp_dir().join("pv-spike-sqlcipher-test.db");
    let _ = std::fs::remove_file(&db_path);

    let conn = Connection::open(&db_path)?;

    // Raw-key interface: 32 bytes as a 64-hex-char literal, never derive our own KDF here
    // (SR-06 says the app derives via Argon2id then hands SQLCipher the raw key; this spike just
    // proves the raw-key PRAGMA path works, using a fixed test-only key).
    let raw_key_hex = "x'0101010101010101010101010101010101010101010101010101010101010101'";
    conn.pragma_update(None, "key", raw_key_hex)?;

    // Required SQLCipher settings, Brief §8 / SR-07:
    conn.pragma_update(None, "cipher_memory_security", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "secure_delete", "ON")?;

    // A trivial query forces SQLCipher to actually open/verify the page format with this key.
    conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)", [])?;
    conn.execute("INSERT INTO t (v) VALUES ('hello')", [])?;

    println!("--- verifying required settings (Brief §8 / SR-07) ---");
    let hmac: i32 = conn.query_row("PRAGMA cipher_use_hmac", [], |r| r.get(0))?;
    println!("cipher_use_hmac (page HMAC auth)  = {hmac} (expect 1)");

    let mem_sec: i32 = conn.query_row("PRAGMA cipher_memory_security", [], |r| r.get(0))?;
    println!("cipher_memory_security            = {mem_sec} (expect 1)");

    let temp_store: i32 = conn.query_row("PRAGMA temp_store", [], |r| r.get(0))?;
    println!("temp_store                        = {temp_store} (expect 2 = MEMORY)");

    let secure_delete: i32 = conn.query_row("PRAGMA secure_delete", [], |r| r.get(0))?;
    println!("secure_delete                      = {secure_delete} (expect 1)");

    let plaintext_header: i32 = conn
        .query_row("PRAGMA cipher_plaintext_header_size", [], |r| r.get(0))
        .unwrap_or(0);
    println!("cipher_plaintext_header_size      = {plaintext_header} (expect 0 = never enabled)");

    let version: String = conn.query_row("PRAGMA cipher_version", [], |r| r.get(0))?;
    println!("cipher_version                     = {version}");

    // FTS5 availability (SR-10: full-text index must live inside SQLCipher itself).
    println!("--- FTS5 ---");
    conn.execute("CREATE VIRTUAL TABLE fts USING fts5(body)", [])?;
    conn.execute("INSERT INTO fts (body) VALUES ('the quick brown fox')", [])?;
    let hit: String = conn.query_row(
        "SELECT body FROM fts WHERE fts MATCH 'quick'",
        [],
        |r| r.get(0),
    )?;
    println!("FTS5 match ok: {hit}");

    // Wrong-key rejection (SR-06 whole-header MAC behavior at a basic level).
    drop(conn);
    let conn2 = Connection::open(&db_path)?;
    conn2.pragma_update(
        None,
        "key",
        "x'0202020202020202020202020202020202020202020202020202020202020202'",
    )?;
    let wrong_key_result: rusqlite::Result<i64> =
        conn2.query_row("SELECT count(*) FROM t", [], |r| r.get(0));
    println!("wrong-key open+query result: {wrong_key_result:?} (expect an error, not data)");

    let _ = std::fs::remove_file(&db_path);
    println!("--- done ---");
    Ok(())
}
