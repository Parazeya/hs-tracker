use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::stats::{CharacterInfo, CollectionSighting};

/// Season and hardcore are part of the identity: the game can reuse a name in
/// another mode, and the two collections must not silently merge.
#[derive(Clone, Serialize)]
pub struct CollectionIdentity {
    pub key: String,
    pub name: String,
    pub season: i64,
    pub hardcore: bool,
}

impl From<&CharacterInfo> for CollectionIdentity {
    fn from(character: &CharacterInfo) -> Self {
        Self {
            key: format!("{}:{}:{}", character.name.trim().to_lowercase(), character.season, character.hardcore),
            name: character.name.clone(),
            season: character.season,
            hardcore: character.hardcore,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CollectionCharacter {
    pub name: String,
    pub season: i64,
    pub hardcore: bool,
    pub enabled: bool,
    /// Lowercase English catalogue name -> first observed Unix milliseconds.
    pub items: BTreeMap<String, u64>,
}

impl CollectionCharacter {
    fn new(identity: &CollectionIdentity) -> Self {
        Self { name: identity.name.clone(), season: identity.season, hardcore: identity.hardcore, enabled: false, items: BTreeMap::new() }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CollectionStore {
    pub characters: BTreeMap<String, CollectionCharacter>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionEdit {
    ClearHistory,
    ClearCharacter,
    RemoveCharacter,
    RemoveAllCharacters,
}

#[derive(Serialize)]
pub struct CollectionView {
    pub current: Option<CollectionIdentity>,
    pub characters: BTreeMap<String, CollectionCharacter>,
    /// Shared progress across every character that has ever collected.
    pub items: BTreeMap<String, u64>,
}

impl CollectionStore {
    /// Merge a checklist into one character without changing that character's
    /// live collection toggle. Historical imports never trigger drop effects.
    pub fn import_items(&mut self, identity: &CollectionIdentity, names: &[String], ts_ms: u64) -> (usize, usize) {
        let shared_before = self.unique_count();
        let character = self.characters.entry(identity.key.clone()).or_insert_with(|| CollectionCharacter::new(identity));
        let mut added = 0;
        for name in names {
            if let std::collections::btree_map::Entry::Vacant(entry) = character.items.entry(name.clone()) {
                entry.insert(ts_ms);
                added += 1;
            }
        }
        (added, self.unique_count() - shared_before)
    }

    /// Unique finds across characters, including finds saved before a toggle
    /// was turned off. This is the same union the collection page shows.
    pub fn unique_count(&self) -> usize {
        self.characters.values()
            .flat_map(|character| character.items.keys().map(String::as_str))
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    pub fn view(&self, current: Option<&CharacterInfo>) -> CollectionView {
        let mut items: BTreeMap<String, u64> = BTreeMap::new();
        for character in self.characters.values() {
            for (name, first) in &character.items {
                let recorded = items.entry(name.clone()).or_insert(*first);
                *recorded = (*recorded).min(*first);
            }
        }
        CollectionView { current: current.map(CollectionIdentity::from), characters: self.characters.clone(), items }
    }

    pub fn any_enabled(&self) -> bool {
        self.characters.values().any(|c| c.enabled)
    }

    pub fn set_enabled(&mut self, identity: &CollectionIdentity, enabled: bool) {
        self.characters.entry(identity.key.clone()).or_insert_with(|| CollectionCharacter::new(identity)).enabled = enabled;
    }

    pub fn edit(&mut self, action: CollectionEdit, key: Option<&str>) -> Result<(), &'static str> {
        match action {
            CollectionEdit::ClearHistory => {
                for character in self.characters.values_mut() {
                    character.items.clear();
                }
            }
            CollectionEdit::ClearCharacter => {
                let key = key.ok_or("Character is required")?;
                self.characters.get_mut(key).ok_or("Character is not in the collection")?.items.clear();
            }
            CollectionEdit::RemoveCharacter => {
                let key = key.ok_or("Character is required")?;
                self.characters.remove(key).ok_or("Character is not in the collection")?;
            }
            CollectionEdit::RemoveAllCharacters => self.characters.clear(),
        }
        Ok(())
    }

    /// `None` means the character is disabled or already had this item.
    /// `Some(false)` records another character's find without announcing an
    /// item the shared collection already owns. Past finds survive disabling.
    pub fn record(&mut self, sighting: &CollectionSighting, ts_ms: u64) -> Option<bool> {
        if !crate::stats::collection_eligible(sighting.item_type, &sighting.rarity) {
            return None;
        }
        let key = CollectionIdentity::from(&sighting.character).key;
        let name = sighting.name.trim().to_lowercase();
        if name.is_empty() || !self.characters.get(&key).is_some_and(|c| c.enabled && !c.items.contains_key(&name)) {
            return None;
        }
        let first_for_collection = !self.characters.values().any(|c| c.items.contains_key(&name));
        self.characters.get_mut(&key).expect("checked above").items.insert(name, ts_ms);
        Some(first_for_collection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(name: &str, season: i64) -> CharacterInfo {
        CharacterInfo { name: name.into(), level: 1, herolevel: 0, difficulty: 0, hell_sub: 0, hardcore: false, season }
    }

    fn sighting(character: CharacterInfo) -> CollectionSighting {
        CollectionSighting { character, name: "Harlequinn's Crest".into(), rarity: "Satanic".into(), item_type: 0, item_id: 0, weapon_type: 0, tier: 6 }
    }

    #[test]
    fn toggling_preserves_finds_and_separates_characters() {
        let a = character("Hero", 9);
        let b = character("Hero", 10);
        let mut store = CollectionStore::default();
        store.set_enabled(&CollectionIdentity::from(&a), true);
        assert_eq!(store.record(&sighting(a.clone()), 12), Some(true));
        assert_eq!(store.record(&sighting(a.clone()), 13), None);
        store.set_enabled(&CollectionIdentity::from(&a), false);
        assert_eq!(store.record(&sighting(a.clone()), 14), None);
        store.set_enabled(&CollectionIdentity::from(&a), true);
        assert_eq!(store.record(&sighting(a), 15), None);
        store.set_enabled(&CollectionIdentity::from(&b), true);
        assert_eq!(store.record(&sighting(b.clone()), 16), Some(false));
        let mut second = sighting(b);
        second.name = "Godfather".into();
        second.item_type = 3;
        second.weapon_type = 1;
        assert_eq!(store.record(&second, 17), Some(true));
        assert_eq!(store.characters.len(), 2);
        assert_eq!(store.view(None).items.len(), 2);
        assert_eq!(store.view(None).items["harlequinn's crest"], 12);
        let restored: CollectionStore = serde_json::from_slice(&serde_json::to_vec(&store).unwrap()).unwrap();
        assert_eq!(restored.characters[&CollectionIdentity::from(&character("Hero", 9)).key].items["harlequinn's crest"], 12);
        assert!(restored.characters[&CollectionIdentity::from(&character("Hero", 10)).key].enabled);
    }

    #[test]
    fn clearing_and_removing_characters_updates_shared_progress() {
        let a = character("Alice", 9);
        let b = character("Bob", 9);
        let ka = CollectionIdentity::from(&a).key;
        let kb = CollectionIdentity::from(&b).key;
        let mut store = CollectionStore::default();
        store.set_enabled(&CollectionIdentity::from(&a), true);
        store.set_enabled(&CollectionIdentity::from(&b), true);
        assert_eq!(store.record(&sighting(a.clone()), 10), Some(true));
        assert_eq!(store.record(&sighting(b.clone()), 11), Some(false));
        assert!(store.edit(CollectionEdit::ClearCharacter, Some(&ka)).is_ok());
        assert!(store.characters[&ka].enabled);
        assert_eq!(store.view(None).items["harlequinn's crest"], 11);
        assert!(store.edit(CollectionEdit::RemoveCharacter, Some(&kb)).is_ok());
        assert!(store.view(None).items.is_empty());
        assert_eq!(store.record(&sighting(a.clone()), 12), Some(true));
        assert!(store.edit(CollectionEdit::ClearHistory, None).is_ok());
        assert!(store.view(None).items.is_empty());
        assert!(store.characters[&ka].enabled);
        assert!(store.edit(CollectionEdit::ClearCharacter, Some("unknown")).is_err());
        assert_eq!(store.characters.len(), 1);
        assert!(store.edit(CollectionEdit::RemoveAllCharacters, None).is_ok());
        assert!(store.characters.is_empty());
        assert!(!store.any_enabled());
    }

    #[test]
    fn unique_count_reaches_666_only_for_a_new_shared_find() {
        let a = character("Alice", 9);
        let b = character("Bob", 9);
        let mut store = CollectionStore::default();
        store.set_enabled(&CollectionIdentity::from(&a), true);
        store.set_enabled(&CollectionIdentity::from(&b), true);
        for n in 0..665 {
            let mut find = sighting(a.clone());
            find.name = format!("Unique gear {n}");
            assert_eq!(store.record(&find, n), Some(true));
        }
        assert_eq!(store.unique_count(), 665);
        let mut duplicate = sighting(b.clone());
        duplicate.name = "Unique gear 42".into();
        assert_eq!(store.record(&duplicate, 665), Some(false));
        assert_eq!(store.unique_count(), 665);
        let mut milestone = sighting(b);
        milestone.name = "The 666th item".into();
        assert_eq!(store.record(&milestone, 666), Some(true));
        assert_eq!(store.unique_count(), 666);
        assert_eq!(store.record(&milestone, 667), None);
        assert_eq!(store.unique_count(), 666);
    }

    #[test]
    fn checklist_merge_preserves_first_find_and_toggle() {
        let a = CollectionIdentity::from(&character("Alice", 9));
        let b = CollectionIdentity::from(&character("Bob", 9));
        let mut store = CollectionStore::default();
        assert_eq!(store.import_items(&a, &["hat".into(), "boots".into()], 100), (2, 2));
        assert_eq!(store.import_items(&a, &["hat".into(), "ring".into()], 200), (1, 1));
        assert_eq!(store.characters[&a.key].items["hat"], 100);
        assert!(!store.characters[&a.key].enabled);
        assert_eq!(store.import_items(&b, &["hat".into()], 300), (1, 0));
        assert_eq!(store.unique_count(), 3);
    }
}
