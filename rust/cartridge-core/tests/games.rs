use cartridge_core::{
    games,
    rom::{self, BOARDS},
    storage::Cancel,
};
use rusqlite::{params, Connection};
use serde_json::json;
use sha1::{Digest, Sha1};
fn db() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA user_version=1; CREATE TABLE catalog (key TEXT,value TEXT); INSERT INTO catalog VALUES ('provider','test'); CREATE TABLE games(id TEXT,platform TEXT,name TEXT,size INTEGER,sha1 TEXT,metadata TEXT);").unwrap();
    c
}
fn add(db: &Connection, data: &[u8], id: &str, platform: &str) {
    let game = json!({"name":"A & B (World)","platform":platform,"publisher":"Test publisher"});
    db.execute(
        "INSERT INTO games VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            id,
            platform,
            "A & B (World)",
            data.len() as i64,
            hex::encode(Sha1::digest(data)),
            game.to_string()
        ],
    )
    .unwrap();
}
#[test]
fn identity_requires_exact_hash_size_and_platform() {
    let c = db();
    let data = b"synthetic bytes";
    add(&c, data, "one", "gbc");
    let r = games::identify_with(&c, data, "gameboy").unwrap();
    assert_eq!(r["game"]["id"], "one");
    assert_eq!(r["match"]["method"], "file");
    assert_eq!(
        games::identify_with(&c, b"synthetic bytes!", "gameboy").unwrap()["status"],
        "not_found"
    );
    c.execute("UPDATE games SET size=size+1", []).unwrap();
    assert_eq!(
        games::identify_with(&c, data, "gameboy").unwrap()["status"],
        "not_found"
    );
}
#[test]
fn ambiguous_matches_are_not_arbitrarily_selected() {
    let c = db();
    add(&c, b"sample", "one", "gb");
    add(&c, b"sample", "two", "gbc");
    let r = games::identify_with(&c, b"sample", "gameboy").unwrap();
    assert_eq!(r["status"], "ambiguous");
    assert!(r.get("game").is_none());
}
#[test]
fn nes_payload_match_does_not_discard_trainers_or_trim_mirrors() {
    let c = db();
    let prg = vec![0x42; 16384];
    add(&c, &prg, "nes", "nes");
    let data = rom::nes_build(&prg, &[], BOARDS[0], "horizontal").unwrap();
    assert_eq!(
        games::identify_with(&c, &data, "famicom").unwrap()["match"]["method"],
        "nes_payload"
    );
    let double = rom::nes_build(&prg.repeat(2), &[], BOARDS[0], "horizontal").unwrap();
    assert_eq!(
        games::identify_with(&c, &double, "famicom").unwrap()["status"],
        "not_found"
    );
    let mut trained = data.clone();
    trained[6] |= 4;
    trained.splice(16..16, [0u8; 512]);
    assert_eq!(
        games::identify_with(&c, &trained, "famicom").unwrap()["status"],
        "not_found"
    );
}
fn png() -> Vec<u8> {
    hex::decode("89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000c49444154789c63689db90a00038f0199e7dd63cd0000000049454e44ae426082").unwrap()
}
#[test]
fn png_validation_rejects_truncation_oversize_and_bad_crc() {
    let mut p = png(); // Compute a valid synthetic IDAT CRC, independent of image decode.
    let end = p.len() - 16;
    let crc = crc32fast::hash(&p[37..end]);
    p[end..end + 4].copy_from_slice(&crc.to_be_bytes());
    games::validate_png(&p).unwrap();
    let mut bad = p.clone();
    bad[20] = 1;
    assert!(games::validate_png(&bad).is_err());
    assert!(games::validate_png(&p[..p.len() - 1]).is_err());
    let mut bad = p;
    bad[42] ^= 1;
    assert!(games::validate_png(&bad).is_err());
    assert!(games::validate_png(&vec![0; 4 * 1024 * 1024 + 1]).is_err());
}
#[test]
fn artwork_urls_cannot_select_a_host_or_local_path() {
    let g = json!({"platform":"gb","name":"../../evil?file&x / <name>"});
    let u = games::thumbnail_url(&g, "boxart").unwrap();
    assert!(u.starts_with("https://raw.githubusercontent.com/libretro-thumbnails/Nintendo_-_Game_Boy/master/Named_Boxarts/"));
    assert!(!u.contains("../"));
    assert!(!u.contains('?'));
    assert!(games::thumbnail_url(&json!({"platform":"evil","name":"name"}), "boxart").is_err());
}
#[test]
fn offline_artwork_never_requires_a_network_or_writable_cache() {
    let temp = tempfile::tempdir().unwrap();
    let mut r = json!({"status":"identified","game":{"platform":"gb","name":"A & B"}});
    games::artwork(
        &mut r,
        &temp.path().join("missing"),
        None,
        false,
        false,
        &Cancel::default(),
    );
    for (k, _, _) in games::ART_TYPES {
        assert_eq!(r["artwork"][k]["status"], "not_downloaded");
    }
    assert!(!temp.path().join("missing").exists());
}
#[test]
fn optional_metadata_failure_cannot_turn_a_saved_read_into_failure() {
    let temp = tempfile::tempdir().unwrap();
    let mut r = json!({"status":"complete","operation":"read","platform":"gameboy","output":temp.path().join("missing.gb"),"directory":temp.path()});
    games::after_read(&mut r, temp.path(), false, &Cancel::default(), &mut |_| {});
    assert_eq!(r["status"], "complete");
    assert!(r["game_warning"].is_string());
}

#[test]
fn gba_catalog_identity_requires_full_hash_and_correct_platform() {
    let c = db();
    let bytes = b"synthetic gba image";
    add(&c, bytes, "gba", "gba");
    assert_eq!(
        games::identify_with(&c, bytes, "gba").unwrap()["status"],
        "identified"
    );
    assert_eq!(
        games::identify_with(&c, bytes, "gameboy").unwrap()["status"],
        "not_found"
    );
    assert_eq!(
        games::identify_with(&c, &bytes[..5], "gba").unwrap()["status"],
        "not_found"
    );
    assert!(
        games::thumbnail_url(&json!({"platform":"gba","name":"A & B"}), "boxart")
            .unwrap()
            .contains("Nintendo_-_Game_Boy_Advance")
    );
}
