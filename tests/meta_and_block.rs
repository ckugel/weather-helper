//! Frontmatter parsing and block upsert tests
//!
//! Covers:
//! - extract_meta success and error paths
//! - upsert_weather_block append/insert/replace idempotency

use std::fs;
use std::path::PathBuf;

use weather_helper::extract_meta;

fn write_temp_file(name: &str, contents: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("{}_{}", name, std::process::id()));
    fs::write(&p, contents).expect("write temp");
    p
}

#[test]
fn extract_meta_ok_and_errors() {
    let path_ok = write_temp_file(
        "meta_ok.md",
        r#"---
city: Rome
arrival: 2025-08-20
departure: 2025-08-25
---

# Title
"#,
    );
    let meta = extract_meta(&path_ok).expect("meta ok");
    assert_eq!(meta.city, "Rome");
    assert_eq!(meta.arrival.to_string(), "2025-08-20");
    assert_eq!(meta.departure.to_string(), "2025-08-25");
    let _ = fs::remove_file(&path_ok);

    let path_missing = write_temp_file(
        "meta_missing.md",
        r#"---
city: Rome
---
"#,
    );
    let err = extract_meta(&path_missing).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("missing 'arrival'") || msg.contains("missing 'departure'"));
    let _ = fs::remove_file(&path_missing);
}

#[test]
fn upsert_block_variants() {
    let new_block = "## Weather Forecast\n<!-- WEATHER:BEGIN -->\nNEW\n<!-- WEATHER:END -->\n";

    // Append when no heading
    let mut content = String::from("# Title\n\nBody\n");
    weather_helper::upsert_weather_block(&mut content, new_block).unwrap();
    assert!(content.contains("## Weather Forecast"));
    assert!(content.contains("NEW"));

    // Insert after heading with empty block
    let mut content2 = String::from("Intro\n\n## Weather Forecast\n");
    weather_helper::upsert_weather_block(&mut content2, new_block).unwrap();
    assert!(content2.contains("NEW"));

    // Replace existing
    let mut content3 =
        String::from("## Weather Forecast\n<!-- WEATHER:BEGIN -->\nOLD\n<!-- WEATHER:END -->\n");
    weather_helper::upsert_weather_block(&mut content3, new_block).unwrap();
    assert!(content3.contains("NEW"));
    assert!(!content3.contains("OLD"));
}
