use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheEntry {
    pub id: String,
    pub bytes: u64,
    pub last_used_epoch: u64,
    pub pinned: bool,
}

#[must_use]
pub fn eviction_plan(entries: &[CacheEntry], limit_bytes: u64) -> Vec<String> {
    let mut used = entries.iter().map(|entry| entry.bytes).sum::<u64>();
    if used <= limit_bytes {
        return Vec::new();
    }

    let mut candidates = entries
        .iter()
        .filter(|entry| !entry.pinned)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|entry| (entry.last_used_epoch, &entry.id));

    let mut removals = Vec::new();
    for entry in candidates {
        if used <= limit_bytes {
            break;
        }
        used = used.saturating_sub(entry.bytes);
        removals.push(entry.id.clone());
    }
    removals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_oldest_unpinned_entries_until_under_limit() {
        let entries = vec![
            CacheEntry {
                id: "active".into(),
                bytes: 60,
                last_used_epoch: 1,
                pinned: true,
            },
            CacheEntry {
                id: "old".into(),
                bytes: 30,
                last_used_epoch: 2,
                pinned: false,
            },
            CacheEntry {
                id: "new".into(),
                bytes: 30,
                last_used_epoch: 3,
                pinned: false,
            },
        ];
        assert_eq!(eviction_plan(&entries, 90), vec!["old"]);
    }

    #[test]
    fn does_nothing_when_cache_is_under_limit() {
        let entries = vec![CacheEntry {
            id: "one".into(),
            bytes: 30,
            last_used_epoch: 1,
            pinned: false,
        }];
        assert!(eviction_plan(&entries, 30).is_empty());
    }
}
