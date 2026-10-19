//! CISA Known Exploited Vulnerabilities catalog.
//! <https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json>
//!
//! OWNER: intel agent.

use std::path::Path;
use std::time::Duration;

use crate::model::Reason;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KevEntry {
    pub cve: String,
    pub vendor: String,
    pub product: String,
    pub name: String,
    /// YYYY-MM-DD
    pub date_added: String,
    pub ransomware: bool,
}

#[derive(Debug, Clone, Default)]
pub struct KevCatalog {
    pub entries: Vec<KevEntry>,
}

impl KevCatalog {
    /// Entries concerning a program, matched conservatively on its display
    /// name and publisher: every significant word of the KEV product must be
    /// in the name, and the vendor must match the publisher or the name.
    /// Generic products (`Windows`, `Office`, ...) only match Microsoft
    /// software with that exact product name.
    pub fn lookup(&self, name: &str, publisher: Option<&str>) -> Vec<&KevEntry> {
        let _ = (name, publisher);
        todo!("KevCatalog::lookup")
    }
}

/// Loads the catalog from `cache_dir/kev.json` when younger than `max_age`,
/// downloads it otherwise (when `online`), falls back to a stale cache.
pub fn load(cache_dir: &Path, max_age: Duration, online: bool) -> anyhow::Result<KevCatalog> {
    let _ = (cache_dir, max_age, online);
    todo!("kev::load")
}

/// Builds the `Reason::KnownExploited` for a set of hits (`None` if empty).
/// `today` is YYYY-MM-DD.
pub fn to_reason(hits: &[&KevEntry], today: &str) -> Option<Reason> {
    let _ = (hits, today);
    todo!("kev::to_reason")
}
