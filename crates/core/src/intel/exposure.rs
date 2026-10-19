//! Curated list of Internet-exposed software families.
//!
//! OWNER: intel agent.

use crate::model::ExposureCategory;

/// Family of a program from its name and (winget) id, if it is one of the
/// high-exposure families: browsers, PDF/document readers, Java/.NET/Python
/// runtimes, archivers, media players, mail/chat/visio, remote access, VPN,
/// password managers, office suites, dev tools that handle remote content
/// (Git, Node.js, PuTTY, WinSCP, FileZilla...), cloud sync clients, network
/// tools.
pub fn exposure(name: &str, id: &str) -> Option<ExposureCategory> {
    let _ = (name, id);
    todo!("exposure::exposure")
}
