//! Scan orchestration: builds the full [`ScanResult`].
//!
//! 1. Environment (tools present, rights, connectivity).
//! 2. In parallel: `winget upgrade`, `winget list` (+ pins), registry,
//!    Chocolatey, Scoop.
//! 3. Items: winget/Store packages (matched to their registry entry for
//!    publisher, folder, icon, links), Chocolatey and Scoop packages, and
//!    programs no package manager knows ("unmanaged").
//! 4. Running processes of the programs to update.
//! 5. Intelligence: CISA KEV catalog, winget-pkgs manifests (release notes,
//!    links, installer type and size).
//! 6. Assessment (priority, default selection), ignore rules, stats, order.
//!
//! [`lookup_unmanaged`] is a second, slower phase run in the background.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::intel::{self, kev::KevCatalog, lookup::LookupInput};
use crate::model::*;
use crate::pkgmgr::{choco, scoop};
use crate::settings::Settings;
use crate::system::registry::{self, InstalledProgram};
use crate::system::{classify, elevation, fileinfo, procs, shell};
use crate::{paths, version, winget};

const KEV_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
const INTEL_THREADS: usize = 8;
const LOOKUP_THREADS: usize = 6;

type Progress<'a> = &'a (dyn Fn(ScanProgress) + Sync);

fn report(progress: Progress, step: ScanStep, state: StepState, detail: Option<String>, fraction: Option<f32>) {
    progress(ScanProgress { step, state, detail, fraction });
}

/// Runs `f` for a step, reporting Started/Done/Failed.
fn step<T>(progress: Progress, step: ScanStep, f: impl FnOnce() -> anyhow::Result<T>) -> Option<T> {
    report(progress, step, StepState::Started, None, None);
    match f() {
        Ok(v) => {
            report(progress, step, StepState::Done, None, None);
            Some(v)
        }
        Err(e) => {
            log::warn!("scan step {step:?} failed: {e:#}");
            report(progress, step, StepState::Failed, Some(format!("{e:#}")), None);
            None
        }
    }
}

fn skip(progress: Progress, step: ScanStep) {
    report(progress, step, StepState::Skipped, None, None);
}

/// Lowercase, single-spaced: how display names are compared.
fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn alnum(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

/// Detects the tools and context.
pub fn environment(settings: &Settings) -> EnvInfo {
    std::thread::scope(|s| {
        let winget = s.spawn(winget::version);
        let choco = s.spawn(|| if settings.use_chocolatey { choco::version() } else { None });
        let scoop = s.spawn(|| settings.use_scoop && scoop::detect());
        let online = s.spawn(|| settings.online_intel && intel::http::is_online());
        let os = s.spawn(shell::os_description);
        EnvInfo {
            app_version: crate::APP_VERSION.to_string(),
            winget: winget.join().ok().flatten(),
            chocolatey: choco.join().ok().flatten(),
            scoop: scoop.join().unwrap_or(false),
            elevated: elevation::is_elevated(),
            admin_member: elevation::is_admin_member(),
            os: os.join().unwrap_or_default(),
            online: online.join().unwrap_or(false),
        }
    })
}

struct Builder {
    items: Vec<UpdateItem>,
    index: HashMap<String, usize>,
    /// Registry entry behind an item, by item key.
    programs: HashMap<String, InstalledProgram>,
}

impl Builder {
    fn get_or_insert(&mut self, item: UpdateItem) -> &mut UpdateItem {
        let i = match self.index.get(&item.key) {
            Some(&i) => i,
            None => {
                self.index.insert(item.key.clone(), self.items.len());
                self.items.push(item);
                self.items.len() - 1
            }
        };
        &mut self.items[i]
    }
}

/// Fills registry-derived fields of an item.
fn attach_program(item: &mut UpdateItem, p: &InstalledProgram) {
    item.publisher = item.publisher.take().or_else(|| p.publisher.clone());
    item.install_location = p.install_location.clone().filter(|s| !s.trim().is_empty());
    item.scope = Some(p.scope());
    item.icon_hint = p
        .display_icon
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| fileinfo::main_executable(p).map(|e| e.to_string_lossy().into_owned()));
    item.links.homepage = item.links.homepage.take().or_else(|| p.url_info_about.clone());
    item.links.download = item.links.download.take().or_else(|| p.url_update_info.clone());
    item.links.support = item.links.support.take().or_else(|| p.help_link.clone());
    if p.uninstall_string.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        item.uninstall_key = Some(p.reg_id());
    }
}

/// Sets installed version and certainty from what the package manager printed.
fn set_installed(item: &mut UpdateItem, raw: &str, program: Option<&InstalledProgram>) {
    if version::is_unknown(raw) {
        match program.and_then(fileinfo::detect_version) {
            Some(v) => {
                item.installed_version = Some(v);
                item.version_certainty = VersionCertainty::FromFile;
            }
            None => {
                item.installed_version = None;
                item.version_certainty = VersionCertainty::Unknown;
            }
        }
        if !item.alternatives.contains(&Alternative::ForceReinstall) {
            item.alternatives.push(Alternative::ForceReinstall);
        }
    } else {
        let (v, approx) = version::strip_approx(raw);
        item.installed_version = Some(v);
        item.version_certainty = if approx { VersionCertainty::Approximate } else { VersionCertainty::Exact };
    }
}

fn source_of(row: &winget::WingetRow) -> Source {
    match row.source.as_deref() {
        Some(s) if s.eq_ignore_ascii_case("msstore") => Source::MsStore,
        _ => Source::Winget,
    }
}

fn winget_key(source: Source, id: &str) -> String {
    match source {
        Source::MsStore => format!("msstore:{id}"),
        _ => format!("winget:{id}"),
    }
}

fn is_pseudo_id(id: &str) -> bool {
    id.starts_with("ARP\\") || id.starts_with("MSIX\\")
}

/// Chocolatey ids are lowercase words (`python313`, `notepadplusplus`): a
/// readable name for display.
fn pretty_choco_name(id: &str) -> String {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn unmanaged_alternatives(p: &InstalledProgram, category: &Category) -> Vec<Alternative> {
    let mut alts = Vec::new();
    match category {
        Category::AutoUpdating { by } => alts.push(Alternative::ManagedElsewhere { by: by.clone() }),
        Category::Game { launcher } => alts.push(Alternative::ManagedElsewhere { by: launcher.clone() }),
        Category::WebApp { browser } => alts.push(Alternative::ManagedElsewhere { by: browser.clone() }),
        Category::Driver | Category::SystemComponent => {
            alts.push(Alternative::ManagedElsewhere { by: "Windows Update".into() })
        }
        Category::StoreApp => alts.push(Alternative::StoreUpdate),
        Category::Application => {}
    }
    let mut push_url = |purpose: UrlPurpose, url: &Option<String>| {
        if let Some(u) = url.as_deref().map(str::trim).filter(|u| u.starts_with("http")) {
            alts.push(Alternative::OpenUrl { purpose, url: u.to_string() });
        }
    };
    push_url(UrlPurpose::Download, &p.url_update_info);
    push_url(UrlPurpose::Homepage, &p.url_info_about);
    push_url(UrlPurpose::Support, &p.help_link);
    if matches!(category, Category::Application | Category::Driver) {
        alts.push(Alternative::OpenUrl { purpose: UrlPurpose::Search, url: intel::lookup::web_search_url(&p.display_name) });
    }
    alts
}

fn kev_reason(kev: Option<&KevCatalog>, item: &UpdateItem, today: &str) -> Option<Reason> {
    let kev = kev?;
    let hits = kev.lookup(&item.name, item.publisher.as_deref());
    intel::kev::to_reason(&hits, today)
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Full scan. Reports progress through `progress` (may be called from
/// several threads).
pub fn scan(settings: &Settings, progress: Progress) -> ScanResult {
    let mut warnings: Vec<ScanWarning> = Vec::new();
    let warn = |warnings: &mut Vec<ScanWarning>, code: &str, detail: Option<String>| {
        warnings.push(ScanWarning { code: code.into(), detail });
    };

    report(progress, ScanStep::Environment, StepState::Started, None, None);
    let env = environment(settings);
    report(progress, ScanStep::Environment, StepState::Done, None, None);
    if env.winget.is_none() {
        warn(&mut warnings, "winget_missing", None);
    }
    if settings.online_intel && !env.online {
        warn(&mut warnings, "offline", None);
    }

    // --- Collection, in parallel -------------------------------------------
    let has_winget = env.winget.is_some();
    let (upgrades, installed, programs, choco_rows, scoop_rows) = std::thread::scope(|s| {
        let upgrades = s.spawn(|| {
            if has_winget {
                step(progress, ScanStep::WingetUpgrade, || winget::list_upgrades(settings.include_unknown))
            } else {
                skip(progress, ScanStep::WingetUpgrade);
                None
            }
        });
        let installed = s.spawn(|| {
            if has_winget {
                step(progress, ScanStep::WingetList, || {
                    let rows = winget::list_installed()?;
                    let pins = winget::list_pins().unwrap_or_default();
                    Ok((rows, pins))
                })
            } else {
                skip(progress, ScanStep::WingetList);
                None
            }
        });
        let programs = s.spawn(|| step(progress, ScanStep::Registry, || Ok(registry::installed_programs())));
        let choco_rows = s.spawn(|| {
            if env.chocolatey.is_some() {
                step(progress, ScanStep::Chocolatey, choco::outdated)
            } else {
                skip(progress, ScanStep::Chocolatey);
                None
            }
        });
        let scoop_rows = s.spawn(|| {
            if env.scoop {
                step(progress, ScanStep::Scoop, scoop::status)
            } else {
                skip(progress, ScanStep::Scoop);
                None
            }
        });
        (
            upgrades.join().ok().flatten(),
            installed.join().ok().flatten(),
            programs.join().ok().flatten(),
            choco_rows.join().ok().flatten(),
            scoop_rows.join().ok().flatten(),
        )
    });
    if has_winget && (upgrades.is_none() || installed.is_none()) {
        warn(&mut warnings, "winget_failed", None);
    }
    if env.chocolatey.is_some() && choco_rows.is_none() {
        warn(&mut warnings, "choco_failed", None);
    }
    if env.scoop && scoop_rows.is_none() {
        warn(&mut warnings, "scoop_failed", None);
    }
    let programs = programs.unwrap_or_else(|| {
        warn(&mut warnings, "registry_failed", None);
        Vec::new()
    });
    let (installed, pins) = installed.unwrap_or_default();
    let pins: HashSet<String> = pins.into_iter().map(|p| p.to_lowercase()).collect();

    // --- Items --------------------------------------------------------------
    let mut by_name: HashMap<String, Vec<&InstalledProgram>> = HashMap::new();
    for p in &programs {
        by_name.entry(norm(&p.display_name)).or_default().push(p);
    }
    let find_program = |name: &str, version: &str| -> Option<InstalledProgram> {
        let candidates = by_name.get(&norm(name))?;
        let v = version::strip_approx(version).0;
        candidates
            .iter()
            .find(|p| p.display_version.as_deref() == Some(v.as_str()))
            .or_else(|| candidates.first())
            .map(|p| (*p).clone())
    };

    let mut b = Builder { items: Vec::new(), index: HashMap::new(), programs: HashMap::new() };
    let mut stats = ScanStats::default();
    let mut unmatched_arp: HashSet<String> = HashSet::new();

    for row in &installed {
        if row.id.starts_with("MSIX\\") {
            stats.store_apps += 1;
            continue;
        }
        if row.id.starts_with("ARP\\") {
            unmatched_arp.insert(row.id.to_lowercase());
            continue;
        }
        let source = source_of(row);
        let key = winget_key(source, &row.id);
        if b.index.contains_key(&key) {
            continue;
        }
        let program = find_program(&row.name, &row.version);
        let item = b.get_or_insert(UpdateItem::new(key.clone(), source, row.id.clone(), row.name.clone()));
        if let Some(p) = &program {
            attach_program(item, p);
        }
        set_installed(item, &row.version, program.as_ref());
        item.alternatives.push(Alternative::InstallVersion);
        if source == Source::MsStore {
            item.category = Category::StoreApp;
            item.alternatives.push(Alternative::StoreUpdate);
        }
        if let Some(p) = program {
            b.programs.insert(key, p);
        }
    }

    if let Some(list) = &upgrades {
        let rows = list.upgrades.iter().map(|r| (r, false)).chain(list.explicit.iter().map(|r| (r, true)));
        for (row, explicit) in rows {
            if is_pseudo_id(&row.id) {
                continue;
            }
            let source = source_of(row);
            let key = winget_key(source, &row.id);
            let known = b.index.contains_key(&key);
            let program = b.programs.get(&key).cloned().or_else(|| find_program(&row.name, &row.version));
            let item = b.get_or_insert(UpdateItem::new(key.clone(), source, row.id.clone(), row.name.clone()));
            if !known {
                if let Some(p) = &program {
                    attach_program(item, p);
                }
                item.alternatives.push(Alternative::InstallVersion);
            }
            set_installed(item, &row.version, program.as_ref());
            item.available_version = row.available.clone().filter(|a| !a.trim().is_empty());
            item.explicit_only = explicit;
            item.kind = ItemKind::Update;
            // The executable says it is already current: not an update after all.
            if item.version_certainty == VersionCertainty::FromFile {
                if let (Some(i), Some(a)) = (&item.installed_version, &item.available_version) {
                    if !version::is_newer(a, i) {
                        item.kind = ItemKind::UpToDate;
                    }
                }
            }
            if let Some(p) = program {
                b.programs.entry(key).or_insert(p);
            }
        }
    }

    for item in &mut b.items {
        if pins.contains(&item.id.to_lowercase()) {
            item.pinned = true;
        }
    }

    for row in choco_rows.unwrap_or_default() {
        let key = format!("choco:{}", row.id);
        let item = b.get_or_insert(UpdateItem::new(key, Source::Chocolatey, row.id.clone(), pretty_choco_name(&row.id)));
        item.installed_version = Some(row.installed.clone());
        item.available_version = Some(row.available.clone());
        item.kind = ItemKind::Update;
        item.scope = Some(Scope::Machine);
        item.pinned = row.pinned;
        item.alternatives.push(Alternative::InstallVersion);
    }

    for row in scoop_rows.unwrap_or_default() {
        let key = format!("scoop:{}", row.app);
        let item = b.get_or_insert(UpdateItem::new(key, Source::Scoop, row.app.clone(), row.app.clone()));
        item.installed_version = Some(row.installed.clone());
        item.available_version = Some(row.available.clone());
        item.kind = ItemKind::Update;
        item.scope = Some(Scope::User);
    }

    // Same software updated by two managers (e.g. Python via winget and choco).
    let choco_ids: Vec<(usize, String)> = b
        .items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.source == Source::Chocolatey && i.kind == ItemKind::Update)
        .map(|(n, i)| (n, alnum(&i.id)))
        .filter(|(_, id)| id.len() >= 4)
        .collect();
    for (ci, cid) in choco_ids {
        let dup = b.items.iter().position(|i| {
            i.source == Source::Winget && i.kind == ItemKind::Update && alnum(&i.id).contains(&cid)
        });
        if let Some(wi) = dup {
            b.items[wi].assessment.reasons.push(Reason::DuplicateManager { other: Source::Chocolatey });
            b.items[ci].assessment.reasons.push(Reason::DuplicateManager { other: Source::Winget });
        }
    }

    // Programs nobody manages. The same program can be registered twice
    // (machine and user entries): list it once.
    let mut seen_unmanaged: HashSet<(String, Option<String>)> = HashSet::new();
    for p in &programs {
        if p.system_component || p.is_update {
            continue;
        }
        if has_winget && !installed.is_empty() && !unmatched_arp.contains(&p.winget_arp_id().to_lowercase()) {
            continue;
        }
        if !seen_unmanaged.insert((norm(&p.display_name), p.display_version.clone())) {
            continue;
        }
        let key = format!("arp:{}", p.reg_id());
        if b.index.contains_key(&key) {
            continue;
        }
        let category = classify::classify(p);
        let mut item = UpdateItem::new(key.clone(), Source::Registry, p.key_name.clone(), p.display_name.clone());
        attach_program(&mut item, p);
        match p.display_version.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => item.installed_version = Some(v.to_string()),
            None => match fileinfo::detect_version(p) {
                Some(v) => {
                    item.installed_version = Some(v);
                    item.version_certainty = VersionCertainty::FromFile;
                }
                None => item.version_certainty = VersionCertainty::Unknown,
            },
        }
        item.alternatives = unmanaged_alternatives(p, &category);
        item.category = category;
        item.kind = ItemKind::Unmanaged;
        b.get_or_insert(item);
        b.programs.insert(key, p.clone());
    }

    // --- Running processes --------------------------------------------------
    report(progress, ScanStep::Processes, StepState::Started, None, None);
    let snapshot = procs::snapshot();
    for item in b.items.iter_mut().filter(|i| i.kind == ItemKind::Update) {
        if let Some(p) = b.programs.get(&item.key) {
            item.install_dirs = p.install_dirs();
            if !item.install_dirs.is_empty() {
                item.running = procs::running_under(&item.install_dirs, &snapshot);
            }
        }
    }
    report(progress, ScanStep::Processes, StepState::Done, None, None);

    // --- Intelligence -------------------------------------------------------
    let cache = paths::cache_dir();
    let kev = if settings.online_intel {
        step(progress, ScanStep::SecurityCatalog, || intel::kev::load(&cache, KEV_MAX_AGE, env.online))
    } else {
        skip(progress, ScanStep::SecurityCatalog);
        None
    };
    if settings.online_intel && kev.is_none() {
        warn(&mut warnings, "kev_failed", None);
    }

    if settings.online_intel && env.online {
        enrich_from_manifests(&mut b.items, &cache, progress);
    } else {
        skip(progress, ScanStep::ReleaseNotes);
    }

    // --- Assessment ---------------------------------------------------------
    // Every item gets one: up-to-date and unmanaged programs keep informative
    // reasons (version read from the file, known exploited flaws...).
    let today = today();
    for item in b.items.iter_mut() {
        item.ignored = settings.is_ignored(&item.key, item.available_version.as_deref());
        let kev = kev_reason(kev.as_ref(), item, &today);
        let notes = item.release_notes.clone();
        let extra = std::mem::take(&mut item.assessment.reasons);
        item.assessment = intel::assess::assess(item, kev, notes.as_deref());
        for r in extra {
            if !item.assessment.reasons.contains(&r) {
                item.assessment.reasons.push(r);
            }
        }
        // winget leaves these out of `upgrade --all` on purpose (the
        // publisher asked for it): the user decides. A stale installer would
        // change nothing.
        let stale = item.assessment.reasons.iter().any(|r| matches!(r, Reason::StaleInstaller { .. }));
        if item.ignored || item.explicit_only || stale {
            item.assessment.preselect = false;
        }
    }

    let mut items = b.items;
    sort_items(&mut items);
    compute_stats(&items, &mut stats);

    ScanResult { items, stats, warnings, environment: env, scanned_at: chrono::Local::now().to_rfc3339() }
}

fn kind_rank(k: ItemKind) -> u8 {
    match k {
        ItemKind::Update => 0,
        ItemKind::Manual => 1,
        ItemKind::Unmanaged => 2,
        ItemKind::UpToDate => 3,
    }
}

pub fn sort_items(items: &mut [UpdateItem]) {
    items.sort_by(|a, b| {
        kind_rank(a.kind)
            .cmp(&kind_rank(b.kind))
            .then(a.assessment.priority.cmp(&b.assessment.priority))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

pub fn compute_stats(items: &[UpdateItem], stats: &mut ScanStats) {
    let store_apps = stats.store_apps;
    *stats = ScanStats { store_apps, ..ScanStats::default() };
    for i in items {
        if i.ignored && matches!(i.kind, ItemKind::Update | ItemKind::Manual) {
            stats.ignored += 1;
            continue;
        }
        match i.kind {
            ItemKind::Update => {
                stats.updates += 1;
                match i.assessment.priority {
                    Priority::Critical => stats.critical += 1,
                    Priority::Major => stats.major += 1,
                    Priority::Recommended => stats.recommended += 1,
                    Priority::Uncertain => stats.uncertain += 1,
                    Priority::Normal => {}
                }
            }
            ItemKind::Manual => stats.manual += 1,
            ItemKind::Unmanaged => stats.unmanaged += 1,
            ItemKind::UpToDate => stats.up_to_date += 1,
        }
    }
}

/// Release notes, links, installer type and size from winget-pkgs manifests,
/// for winget updates. Parallel, bounded.
fn enrich_from_manifests(items: &mut [UpdateItem], cache: &std::path::Path, progress: Progress) {
    let targets: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.kind == ItemKind::Update && i.source == Source::Winget && i.available_version.is_some())
        .map(|(n, _)| n)
        .collect();
    report(progress, ScanStep::ReleaseNotes, StepState::Started, None, Some(0.0));
    if targets.is_empty() {
        report(progress, ScanStep::ReleaseNotes, StepState::Done, None, Some(1.0));
        return;
    }

    struct Found {
        meta: Option<intel::manifest::ManifestMeta>,
        installer_type: Option<String>,
        url: Option<String>,
        size: Option<u64>,
    }
    let inputs: Vec<(usize, String, String, Option<Scope>)> = targets
        .iter()
        .map(|&n| (n, items[n].id.clone(), items[n].available_version.clone().unwrap_or_default(), items[n].scope))
        .collect();
    let results: Mutex<Vec<(usize, Found)>> = Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let total = inputs.len();

    std::thread::scope(|s| {
        for _ in 0..INTEL_THREADS.min(total) {
            s.spawn(|| loop {
                let k = next.fetch_add(1, Ordering::SeqCst);
                let Some((n, id, ver, scope)) = inputs.get(k) else { break };
                let meta = intel::manifest::fetch(id, ver, cache)
                    .map_err(|e| log::debug!("manifest {id} {ver}: {e:#}"))
                    .ok();
                let installer = intel::manifest::fetch_installers(id, ver, cache)
                    .map_err(|e| log::debug!("installers {id} {ver}: {e:#}"))
                    .ok()
                    .and_then(|list| intel::manifest::pick_installer(&list, *scope).cloned());
                let size = installer.as_ref().and_then(|i| intel::http::content_length(&i.url));
                let (url, installer_type) = installer.map(|i| (Some(i.url), i.installer_type)).unwrap_or_default();
                let found = Found { meta, installer_type, url, size };
                if let Ok(mut r) = results.lock() {
                    r.push((*n, found));
                }
                let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                report(progress, ScanStep::ReleaseNotes, StepState::Started, None, Some(d as f32 / total as f32));
            });
        }
    });

    for (n, found) in results.into_inner().unwrap_or_default() {
        let item = &mut items[n];
        if let Some(meta) = found.meta {
            item.release_notes = meta.release_notes;
            item.links.release_notes = meta.release_notes_url;
            if let Some(u) = meta.package_url.or(meta.publisher_url) {
                item.links.homepage = Some(u);
            }
            if let Some(u) = meta.support_url {
                item.links.support = Some(u);
            }
            if item.publisher.is_none() {
                item.publisher = meta.publisher;
            }
        }
        item.installer_type = found.installer_type;
        item.download_size = found.size;
        if let (Some(url), Some(installed), Some(available), VersionCertainty::Exact) =
            (found.url.as_deref(), item.installed_version.as_deref(), item.available_version.as_deref(), item.version_certainty)
        {
            if let Some(v) = intel::manifest::stale_installer(url, installed, available) {
                log::info!("{}: the {available} manifest ships the {v} installer ({url})", item.key);
                item.assessment.reasons.push(Reason::StaleInstaller { installer_version: v });
            }
        }
    }
    report(progress, ScanStep::ReleaseNotes, StepState::Done, None, Some(1.0));
}

/// Background phase: looks for newer versions of unmanaged applications and
/// calls `on_item` with every item that changed (turned into `Manual`).
pub fn lookup_unmanaged(
    items: &[UpdateItem],
    progress: Progress,
    on_item: &(dyn Fn(UpdateItem) + Sync),
) {
    let targets: Vec<&UpdateItem> = items
        .iter()
        .filter(|i| {
            i.kind == ItemKind::Unmanaged
                && i.category == Category::Application
                && i.installed_version.is_some()
                && i.version_certainty != VersionCertainty::Unknown
        })
        .collect();
    report(progress, ScanStep::Lookup, StepState::Started, None, Some(0.0));
    let total = targets.len();
    if total == 0 {
        report(progress, ScanStep::Lookup, StepState::Done, None, Some(1.0));
        return;
    }
    let kev = intel::kev::load(&paths::cache_dir(), Duration::from_secs(u64::MAX / 4), false).ok();
    let today = today();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);

    std::thread::scope(|s| {
        for _ in 0..LOOKUP_THREADS.min(total) {
            s.spawn(|| loop {
                let k = next.fetch_add(1, Ordering::SeqCst);
                let Some(item) = targets.get(k) else { break };
                let mut urls: Vec<String> = Vec::new();
                for u in [&item.links.homepage, &item.links.download, &item.links.support].into_iter().flatten() {
                    urls.push(u.clone());
                }
                let input = LookupInput {
                    name: item.name.clone(),
                    publisher: item.publisher.clone(),
                    version: item.installed_version.clone(),
                    urls,
                };
                if let Some(alt) = intel::lookup::latest_elsewhere(&input) {
                    let mut updated = (*item).clone();
                    if let Alternative::LatestElsewhere { version, .. } = &alt {
                        updated.available_version = Some(version.clone());
                    }
                    updated.alternatives.insert(0, alt);
                    updated.kind = ItemKind::Manual;
                    let kev = kev_reason(kev.as_ref(), &updated, &today);
                    updated.assessment = intel::assess::assess(&updated, kev, None);
                    on_item(updated);
                }
                let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                report(progress, ScanStep::Lookup, StepState::Started, None, Some(d as f32 / total as f32));
            });
        }
    });
    report(progress, ScanStep::Lookup, StepState::Done, None, Some(1.0));
}
