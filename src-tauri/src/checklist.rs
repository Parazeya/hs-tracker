use std::{io::Cursor, sync::OnceLock, time::Duration};

use calamine::{open_workbook_auto_from_rs, Data, Range, Reader};
use serde::{Deserialize, Serialize};

use crate::{items, stats};

const MAX_WORKBOOK_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
pub struct CheckedItem {
    pub name: String,
    pub rarity: String,
    pub slot: String,
    /// Exact catalogue identity after checking name, rarity, and equipment slot.
    pub canonical: Option<String>,
}

#[derive(Serialize)]
pub struct ChecklistPreview {
    pub source: String,
    pub checked: Vec<CheckedItem>,
}

/// Three current Heroic weapons have no packet identity in the game's item
/// data. Keep them in the collection catalogue without inventing game IDs.
#[derive(Deserialize)]
pub struct ExtraItem {
    pub name: String,
    pub rarity: String,
    #[serde(rename = "type")]
    pub item_type: i64,
    pub weapon: i64,
}

fn extra_items() -> &'static [ExtraItem] {
    static ITEMS: OnceLock<Vec<ExtraItem>> = OnceLock::new();
    ITEMS.get_or_init(|| serde_json::from_str(include_str!("../../src/collection-extras.json"))
        .expect("collection extras are valid JSON"))
}

/// Fold harmless spelling differences in the community checklist. Exact
/// rarity and equipment slot are still checked before an item is accepted.
fn name_key(name: &str) -> String {
    let mut key = String::new();
    for ch in name.trim().to_lowercase().chars() {
        let folded = match ch {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'ø' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        };
        if folded.is_alphanumeric() { key.push(folded); }
    }
    key
}

fn checklist_name_key(name: &str) -> String {
    let key = name_key(name);
    match key.as_str() {
        // The source checklist has these three old spellings. Each maps to
        // one gear name after rarity and slot are checked below.
        "metalvolcalistsleathermask" => "metalvocalistsleathermask".into(),
        "aracnistswrath" => "arcanistswrath".into(),
        "graxysmomlure" => "graxysluckylure".into(),
        _ => key,
    }
}

fn catalogue() -> impl Iterator<Item = (&'static str, i64, &'static str)> {
    items::catalogue_items()
        .map(|(name, item_type, _, _, rarity)| (name, item_type, rarity))
        .chain(extra_items().iter().map(|item| (item.name.as_str(), item.item_type, item.rarity.as_str())))
}

pub fn extra_item(name: &str) -> Option<&'static ExtraItem> {
    let key = name_key(name);
    extra_items().iter().find(|item| name_key(&item.name) == key)
}

/// Validate a manually edited item against the same equippable catalogue.
pub fn collection_item(name: &str) -> Option<String> {
    let key = name_key(name);
    let mut found = None;
    for (canonical, item_type, rarity) in catalogue() {
        if name_key(canonical) != key || !stats::collection_eligible(item_type, rarity) { continue; }
        if found.as_deref().is_some_and(|previous: &str| previous != canonical) { return None; }
        found = Some(canonical.to_lowercase());
    }
    found
}

fn cell(range: &Range<Data>, row: u32, col: u32) -> String {
    match range.get_value((row, col)) {
        Some(Data::String(value)) => value.trim().to_string(),
        Some(value) => value.to_string().trim().to_string(),
        None => String::new(),
    }
}

fn checked(value: Option<&Data>) -> bool {
    match value {
        Some(Data::Bool(true)) | Some(Data::Int(1)) => true,
        Some(Data::Float(n)) if *n == 1.0 => true,
        Some(Data::String(s)) if s.trim().eq_ignore_ascii_case("true") => true,
        _ => false,
    }
}

fn slot_matches(slot: &str, item_type: i64) -> bool {
    matches!(
        (slot.trim().to_ascii_lowercase().as_str(), item_type),
        ("helmet", 0)
            | ("chest", 1)
            | ("boot", 2)
            | ("weapon", 3)
            | ("glove", 4)
            | ("amulet", 5)
            | ("shield", 6)
            | ("ring", 7)
            | ("belt", 8)
            | ("charm", 10)
            | ("potion", 18)
    )
}

fn resolve(name: &str, rarity: &str, slot: &str) -> Option<String> {
    let key = checklist_name_key(name);
    let mut found = None;
    for (canonical, item_type, known_rarity) in catalogue() {
        if name_key(canonical) != key
            || !stats::collection_eligible(item_type, known_rarity)
            || !slot_matches(slot, item_type)
        { continue; }
        // This one row calls Voodoo Doll Set; the game calls the sole charm of
        // that name Heroic. The name and slot still identify it exactly.
        let old_voodoo_rarity = key == "voodoodoll" && rarity.trim().eq_ignore_ascii_case("Set")
            && known_rarity == "Heroic";
        if !known_rarity.eq_ignore_ascii_case(rarity.trim()) && !old_voodoo_rarity { continue; }
        if found.as_deref().is_some_and(|previous: &str| previous != canonical) { return None; }
        found = Some(canonical.to_string());
    }
    found
}

pub fn resolve_checked_item(item: &CheckedItem) -> Option<String> {
    resolve(&item.name, &item.rarity, &item.slot)
}

fn parse_range(range: &Range<Data>, source: String) -> Result<ChecklistPreview, String> {
    let Some((start_row, start_col)) = range.start() else {
        return Err("Check List is empty".into());
    };
    let Some((end_row, end_col)) = range.end() else {
        return Err("Check List is empty".into());
    };
    let header = (start_row..=end_row.min(start_row + 30))
        .find_map(|row| {
            let columns = (start_col..=end_col)
                .map(|col| (cell(range, row, col).to_ascii_lowercase(), col))
                .collect::<Vec<_>>();
            let find = |label: &str| {
                columns
                    .iter()
                    .find(|(name, _)| name == label)
                    .map(|(_, col)| *col)
            };
            Some((
                row,
                find("name")?,
                find("rarity")?,
                find("slot")?,
                find("drop rate")?,
            ))
        })
        .ok_or("This workbook does not contain the expected Check List columns")?;
    let (header_row, name_col, rarity_col, slot_col, drop_rate_col) = header;
    // The hcjobo sheet leaves the checkbox heading blank, directly after
    // Drop Rate. Require actual boolean cells so a changed sheet fails safely.
    let checkbox_col = drop_rate_col + 1;
    if checkbox_col > end_col
        || !(header_row + 1..=end_row.min(header_row + 30))
            .any(|row| matches!(range.get_value((row, checkbox_col)), Some(Data::Bool(_))))
    {
        return Err("Check List checkbox column was not found".into());
    }
    let mut checked_items = Vec::new();
    for row in header_row + 1..=end_row {
        if !checked(range.get_value((row, checkbox_col))) {
            continue;
        }
        let name = cell(range, row, name_col);
        if name.is_empty() {
            continue;
        }
        let rarity = cell(range, row, rarity_col);
        let slot = cell(range, row, slot_col);
        let canonical = resolve(&name, &rarity, &slot);
        checked_items.push(CheckedItem {
            name,
            rarity,
            slot,
            canonical,
        });
    }
    Ok(ChecklistPreview {
        source,
        checked: checked_items,
    })
}

pub fn parse_workbook(bytes: Vec<u8>, source: String) -> Result<ChecklistPreview, String> {
    if bytes.len() as u64 > MAX_WORKBOOK_BYTES {
        return Err("The workbook is too large".into());
    }
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes))
        .map_err(|_| "Could not open the Excel workbook".to_string())?;
    let range = workbook
        .worksheet_range("Check List")
        .map_err(|_| "The workbook has no Check List tab".to_string())?;
    parse_range(&range, source)
}

pub async fn download_workbook(url: &str) -> Result<Vec<u8>, String> {
    // Accept a Google Sheets document link, never a caller-supplied download
    // host. This keeps the native HTTP client from following arbitrary URLs.
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| "Invalid Google Sheets link")?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("docs.google.com") {
        return Err("Use a Google Sheets link".into());
    }
    let path: Vec<_> = parsed
        .path_segments()
        .ok_or("Invalid Google Sheets link")?
        .collect();
    let id = match path.as_slice() {
        ["spreadsheets", "d", id, ..] => *id,
        _ => return Err("Invalid Google Sheets link".into()),
    };
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Invalid Google Sheets link".into());
    }
    let export = format!("https://docs.google.com/spreadsheets/d/{id}/export?format=xlsx");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(export)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|_| {
            "Could not download this sheet. Allow link viewing or import a downloaded .xlsx file."
                .to_string()
        })?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() as u64 + chunk.len() as u64 > MAX_WORKBOOK_BYTES {
            return Err("The workbook is too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_checked_rows_in_the_check_list_layout() {
        let mut range = Range::new((0, 1), (3, 7));
        for (col, value) in [(1, "Name"), (3, "Rarity"), (4, "Slot"), (6, "Drop Rate")] {
            range.set_value((0, col), Data::String(value.into()));
        }
        range.set_value((1, 1), Data::String("  Abomination's Brain  ".into()));
        range.set_value((1, 3), Data::String("Set".into()));
        range.set_value((1, 4), Data::String("Charm".into()));
        range.set_value((1, 7), Data::Bool(true));
        range.set_value((2, 1), Data::String("Absolute Zero".into()));
        range.set_value((2, 7), Data::Bool(false));
        let result = parse_range(&range, "test".into()).unwrap();
        assert_eq!(result.checked.len(), 1);
        assert_eq!(result.checked[0].name, "Abomination's Brain");
        assert_eq!(result.checked[0].slot, "Charm");
        assert_eq!(
            result.checked[0].canonical.as_deref(),
            Some("Abomination's Brain")
        );
    }

    #[test]
    fn rejects_wrong_rarity_slot_and_non_gear() {
        assert!(resolve("Abomination's Brain", "Satanic", "Charm").is_none());
        assert!(resolve("Abomination's Brain", "Set", "Helmet").is_none());
        assert!(resolve("Uncut Jewel", "Satanic", "Charm").is_none());
    }

    #[test]
    fn collector_checklist_spelling_and_identity_matches() {
        for (name, rarity, slot) in [
            ("Voodoo Doll", "Set", "Charm"),
            ("Metal Volcalist's Leather Mask", "Set", "Helmet"),
            ("Angel", "Set", "Weapon"),
            ("Death's Scythe", "Set", "Weapon"),
            ("Mevius Mighty Helmet", "Set", "Helmet"),
            ("Shrunken Head", "Satanic", "Charm"),
            ("Surstromming", "Satanic", "Potion"),
            ("Sarcasters Coffee Mug", "Satanic", "Potion"),
            ("Leviathan´s Spine", "Heroic", "Weapon"),
            ("Grimtide's Necklace", "Heroic", "Amulet"),
            ("Komodos Bloodstrap", "Heroic", "Belt"),
            ("Ethereal Musket", "Heroic", "Weapon"),
            ("Sahkopuimuri", "Heroic", "Weapon"),
            ("Ghostplunderer's Marchers", "Heroic", "Boot"),
            ("Graxy's Mom Lure", "Heroic", "Charm"),
            ("Captain's Anchor", "Heroic", "Charm"),
            ("Aracnist's Wrath", "Heroic", "Chest"),
            ("Conjured Tentacle", "Heroic", "Weapon"),
        ] {
            assert!(resolve(name, rarity, slot).is_some(), "{name} should match");
        }
        assert_eq!(resolve("Angel", "Set", "Weapon"), Some("Angel".into()));
        assert!(resolve("Angel", "Heroic", "Weapon").is_none());
        assert!(resolve("Voodoo Doll", "Set", "Helmet").is_none());
        assert!(collection_item("Conjured Tentacle").is_some());
        assert!(collection_item("Uncut Jewel").is_none());
    }

    #[test]
    #[ignore = "requires access to the public Google Sheets template"]
    fn preview_template_inside_async_runtime() {
        tauri::async_runtime::block_on(async {
            let url = "https://docs.google.com/spreadsheets/d/1HgPURFEHV47N2QroLu7hfqahDSQU2vt0shl_1HsTPE4/edit?usp=sharing";
            let bytes = download_workbook(url).await.unwrap();
            let preview = parse_workbook(bytes, url.into()).unwrap();
            assert!(!preview.checked.is_empty());
            assert!(preview.checked.iter().any(|item| item.canonical.is_some()));
        });
    }

    #[test]
    #[ignore = "set HS_CHECKLIST_TEST_URL to a viewable Google Sheets copy"]
    fn collector_copy_matches_checked_gear_inside_async_runtime() {
        tauri::async_runtime::block_on(async {
            let url = std::env::var("HS_CHECKLIST_TEST_URL").expect("set HS_CHECKLIST_TEST_URL");
            let bytes = download_workbook(&url).await.unwrap();
            let preview = parse_workbook(bytes, url).unwrap();
            assert!(!preview.checked.is_empty());
            let skipped = preview.checked.iter().filter(|item| item.canonical.is_none())
                .map(|item| item.name.as_str()).collect::<Vec<_>>();
            assert!(skipped.is_empty(), "unmatched checked gear: {skipped:?}");
        });
    }
}
