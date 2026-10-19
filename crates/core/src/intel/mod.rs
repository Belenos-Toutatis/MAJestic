//! Update intelligence: which updates matter, and what to propose when the
//! automatic path is not available.
//!
//! OWNER: intel agent.
//!
//! Privacy rule: catalogs are *downloaded*; the list of installed software is
//! never sent anywhere. Per-package requests (winget-pkgs manifests,
//! Chocolatey/GitHub lookups) only carry one package name or id each.

pub mod assess;
pub mod exposure;
pub mod http;
pub mod kev;
pub mod lookup;
pub mod manifest;
