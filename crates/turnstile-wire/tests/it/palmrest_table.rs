//! The key table is a copy of palmrest's `keys.rs::NAMES` plus four modifiers. When palmrest's source
//! sits next to this repository (the lane worktree), a code palmrest has and this table lacks fails
//! here; without it (CI of this repository alone) the comparison is skipped and says so.

use std::path::PathBuf;
use turnstile_wire::keytable::{KEY_TABLE, class_of};

/// Every `("name", code)` pair between `pub const NAMES` and the closing `];`.
fn palmrest_names(source: &str) -> Vec<(String, u16)> {
    let body = source
        .split("pub const NAMES")
        .nth(1)
        .and_then(|rest| rest.split("];").next())
        .unwrap_or_default();
    body.split('(')
        .skip(1)
        .filter_map(|chunk| {
            let (name, tail) = chunk.split_once('"')?.1.split_once('"')?;
            let code = tail.trim_start_matches(',').split(')').next()?.trim();
            Some((name.to_owned(), code.parse().ok()?))
        })
        .collect()
}

#[test]
fn the_table_has_no_duplicate_names_or_codes() {
    for (i, a) in KEY_TABLE.iter().enumerate() {
        for b in &KEY_TABLE[i + 1..] {
            assert!(
                a.name != b.name && a.code != b.code,
                "{} and {}",
                a.name,
                b.name
            );
        }
    }
}

#[test]
fn every_key_palmrest_can_name_is_in_the_table() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../palmrest/crates/palmrest/src/keys.rs");
    let Ok(source) = std::fs::read_to_string(&path) else {
        eprintln!("skipped: {} is not there", path.display());
        return;
    };
    let names = palmrest_names(&source);
    assert!(
        names.len() > 50,
        "parsed only {} names from palmrest",
        names.len()
    );
    for (name, code) in names {
        let ours = KEY_TABLE.iter().find(|e| e.code == code);
        assert_eq!(
            ours.map(|e| e.name),
            Some(name.as_str()),
            "palmrest's {name} ({code})"
        );
        assert!(class_of(code).is_some());
    }
    for modifier in [29, 42, 56, 125] {
        assert!(class_of(modifier).is_some(), "modifier {modifier}");
    }
}
