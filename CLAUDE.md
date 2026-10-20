# CLAUDE.md — repères pour travailler sur MAJestic

Guide condensé pour un agent Claude Code intervenant sur ce dépôt. Le contrat
détaillé (modules, commandes et événements UI ↔ backend, estimations de temps) est
dans [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) ; les fonctions, la
confidentialité et la politique de signature sont dans le [`README.md`](README.md).
Ce fichier résume l'essentiel et surtout les **règles** et les **pièges**.

## Quoi

Application **portable** Windows 10/11 (un seul `MAJestic.exe`, rien à installer) qui
trouve et installe les mises à jour de tous les logiciels du PC : winget, Microsoft
Store, Chocolatey, Scoop, et recherche en ligne pour les programmes non gérés. Elle
remplace le script de l'utilisateur `mise a jour.bat` (`winget upgrade --all` +
fermeture/relance de Nextcloud). Moteur en **Rust**, interface **Tauri 2** (WebView2,
HTML/CSS/JS sans étape de build), en **français et anglais**.

## Règles du projet

- **Parler à l'utilisateur en français**, toujours (y compris comptes rendus et
  récapitulatifs). Code, commentaires et messages de commit en anglais.
- **Identité publique** : pseudonyme **Belenos Toutatis** uniquement. Jamais le nom
  réel ni l'adresse e-mail personnelle dans un fichier du dépôt, une capture ou un
  commit. Les commits utilisent l'adresse noreply GitHub (déjà dans la config git
  locale du dépôt).
- **Ne jamais pousser** les branches locales `worktree-agent-*` : elles contiennent
  d'anciens commits avec l'adresse personnelle.
- **Captures d'écran** : uniquement avec les données fictives du mock (voir plus bas),
  jamais la vraie liste de logiciels de l'utilisateur.
- **Tests d'installation réels** : seulement sur quelques logiciels sans risque, et
  prévenir l'utilisateur avant chaque demande UAC.
- **Interface sans aide** : chaque élément s'explique par une bulle au survol
  (attribut `tip`). Thème clair, sombre ou automatique. Toute chaîne visible passe
  par `t()` et existe dans `ui/i18n/fr.js` **et** `ui/i18n/en.js` (mêmes clés).
- La liste des logiciels n'est **jamais** envoyée en ligne : une requête = un logiciel.
  Toute nouvelle connexion réseau doit être ajoutée au tableau « Privacy /
  Confidentialité » du README (c'est aussi la politique exigée par SignPath).

## Commandes

```bash
cargo test --workspace                 # ~160 tests, tous doivent passer
cargo clippy --workspace --all-targets # doit rester sans avertissement
cargo build --release -p majestic      # → target/release/MAJestic.exe
cargo run -p majestic-core --example scan -- --json      # analyse complète (--lookup : + recherche en ligne)
cargo run -p majestic-core --example job -- <source> <id> [version] [--from V --to V]
cargo run -p majestic-core --example stream -- winget download --id X -d C:\tmp
cargo run -p majestic-core --example selfupdate -- <copie\MAJestic.exe> 0.1.0   # auto-MAJ sans fenêtre
cargo run -p majestic-core --example wupdate -- [--online] [--open]          # mises à jour Windows en attente
python -m http.server -d ui            # interface dans un navigateur, backend simulé (.claude/launch.json : « ui-mock », sans cache)
```

- Fermer `MAJestic.exe` avant de recompiler (sinon « Accès refusé », exe verrouillé).
- Mock (`ui/js/mock.js`, actif hors Tauri) : `?speed=4`, `?mock=empty`,
  `?mock=scan-error`, `?uac=refuse`, `?appUpdate=1` (ou `=fail`, `=error`), `?updated=1`, `?wu=none|error|reboot`,
  `?schedule=missing`, `?downloads=1`.
- Modes de l'exe : sans argument = fenêtre ; `--worker <pipe> <pid>` = processus
  élevé ; `--check` = vérification planifiée (toast, sans fenêtre) ;
  `--after-update <pid> <version>` = fenêtre lancée par l'auto-mise à jour.
- Une seule fenêtre MAJestic à la fois : si l'utilisateur a déjà MAJestic ouvert, une
  copie de test passe la main à sa fenêtre et se ferme aussitôt.
- `gh` est dans `C:\Program Files\GitHub CLI\gh.exe` (pas dans le PATH de Bash).

## Architecture (résumé)

```
crates/core/   majestic-core, le moteur (bibliothèque, aucune GUI)
  model.rs       contrat partagé, sérialisé en camelCase vers l'UI
  version.rs     comparaison de versions façon winget (is_major_jump ignore les années)
  winget/        parseur de tableaux, commandes, codes de sortie → FailureKind
  pkgmgr/        Chocolatey, Scoop, Microsoft Store
  system/        registre, classification, versions de fichier, icônes, processus, élévation
  intel/         CISA KEV, manifestes winget-pkgs, exposition, priorités, recherches en ligne
  scan.rs        orchestration → ScanResult ; eta.rs estimations ; jobs.rs une installation
  scheduler.rs   tâche planifiée ; settings.rs / history.rs / paths.rs
app/src/       Tauri : commands.rs, batch.rs (file d'installations), worker.rs (élevé),
               notify.rs (toasts), check.rs (--check), logging.rs
ui/            modules ES vanilla (views/, components/), i18n/, css/, mock
```

Données dans `%LOCALAPPDATA%\MAJestic` : `settings.json`, `history.jsonl`, `last-check.json`, `cache/`,
`logs/app.log`, `worker.log`, `check.log`.

## Décisions et mécanismes clés

- **Élévation** : une seule demande UAC par lot. `JobRequest::needs_elevation()`
  décide (Chocolatey et scan Store : oui ; Scoop, Store, MSIX : non ; winget/registre :
  si portée ≠ utilisateur). Les tâches élevées partent au même exe lancé en
  `--worker` (ShellExecuteEx `runas`), via un tube nommé JSON-lines dont les deux
  côtés vérifient le PID de l'autre. Les applis sont fermées et **relancées par la
  fenêtre non élevée**, jamais en administrateur.
- **Parsing winget** : en-têtes localisés, colonnes par largeur d'affichage
  (unicode-width), plusieurs tableaux (le 2ᵉ de `upgrade` = paquets à cibler
  explicitement), pseudo-identifiants ARP/MSIX.
- **Progression** : winget n'affiche aucune progression quand sa sortie est
  redirigée → on mesure la taille de `%TEMP%\WinGet\<id>.<version>\` et
  `%TEMP%\chocolatey\<id>\` (`jobs::downloaded_bytes`, ticker de 500 ms dans `batch.rs`).
- **Temps restant** : `EtaModel` (historique : débit médian, durées par paquet, ratio
  de correction ; sinon modèle par type d'installateur) + `BatchTracker` (calibrage et
  lissage en direct).
- **Priorités** : Critique > Majeure > À vérifier > Recommandée > Courante.
  Correspondance KEV volontairement prudente. Taille de l'installateur par requête
  `HEAD` sur l'URL du manifeste winget.
- **Codes d'erreur** : `winget/codes.rs`. Les codes HTTP (`0x8019xxxx`) donnent
  `DownloadFailed` (ex. 403 du serveur de l'éditeur, cas réel de Xodo : pas un bug).
- **Fiches winget erronées** : `intel::manifest::stale_installer` repère une fiche dont
  l'URL d'installateur porte la version installée et pas celle annoncée (cas réel :
  VS Code 1.142.0 livrait le setup 1.141.2, la mise à jour revenait sans fin) → raison
  `StaleInstaller`, élément non coché. Après chaque mise à jour winget réussie,
  `jobs::unchanged_version` relit la version (`winget list --id`, 3 essais sur ~20 s) ;
  inchangée → `FailureKind::NoEffect`, avec le bouton « Ignorer la version ».
- **Langue de l'installation (winget)** : winget lit la valeur `Language` du
  programme installé ; un MSI qui met 0 est pris pour la langue de l'utilisateur
  (fr-FR), et un installateur seulement en anglais est alors refusé : « mise à jour
  non applicable » (`0x8A15002B`, cas réel de Murmure 2.0.3, alors que sa propre mise
  à jour interne marchait). `jobs::locale_retry` relance une fois avec `--locale`
  quand tous les installateurs de la fiche winget-pkgs ont la même langue
  (`manifest::single_locale`) : c'est le même fichier, aucun choix de langue n'est
  changé. Diagnostic : `winget upgrade --id X --verbose-logs`, puis la ligne
  « Locale Comparator » du journal de winget (`IsInstalledLocale: 1`).
- **Mise à jour de MAJestic lui-même** (`selfupdate.rs` dans le moteur, `appupdate.rs`
  dans l'app, `ui/js/appupdate.js`) : dernière version GitHub, téléchargement en
  `MAJestic.exe.new`, contrôle SHA-256 + version du fichier, exe renommé en `.old`,
  nouveau lancé avec `--after-update <pid> <ancienne version>`, qui attend la fin de
  l'ancien (verrou d'instance unique) puis supprime le `.old`. Les notes affichées
  s'arrêtent au marqueur `<!-- download-notes -->` de `.github/release-notes.md`.
  Respecte « Utiliser les sources en ligne ». Vérifiée au lancement, puis toutes les
  6 heures fenêtre ouverte (`startAppUpdateTimer`), et par le bouton « Rechercher une
  mise à jour » de Paramètres › À propos ; `check_app_update` renvoie une erreur
  quand GitHub ne répond pas (affichée seulement dans À propos).
- **Windows Update** (`system/windows_update.rs`, API COM wuapi, sans droits admin) :
  recherche en arrière-plan après chaque analyse (étape `windows_update`, événement
  `windows-updates`), classement par identifiants de catégorie (les noms sont
  traduits), redémarrage en attente, dates de Windows. MAJestic **n'installe pas** ces
  mises à jour : bouton `ms-settings:windowsupdate-action` (ou `-optionalupdates`).
  Une recherche hors ligne prend ~15 s, en ligne bien plus. Mock : `?wu=none|error|reboot`.
- **Désinstallation** (panneau détails) : la bonne méthode d'abord — `JobAction::Uninstall`
  par winget / Chocolatey / Scoop (file de travaux habituelle, historique, pas de
  relance du logiciel), sinon `run_uninstaller` (le `UninstallString` du registre,
  relu côté Rust à partir de `uninstallKey`, MSI `/I` → `/X`), sinon Paramètres ›
  Applications installées (apps du Store) ; toujours aussi « Programmes et
  fonctionnalités ». Les vraies désinstallations ne se testent qu'avec l'accord de
  l'utilisateur.
- **Logiciels non gérés** (`intel::lookup`) : version la plus récente cherchée sur
  GitHub quand un lien du registre mène à un dépôt, sinon dans le catalogue
  Chocolatey. `KNOWN_REPOS` relie à leur dépôt les logiciels dont le registre ne
  donne pas le bon lien : Euro-Office (fork d'ONLYOFFICE qui a gardé l'éditeur
  « Ascensio System SIA » et les liens onlyoffice.com) → `Euro-Office/DesktopEditors`.
  Ses étiquettes portent une révision (`v9.3.5-2` = 9.3.5.0 installé), lue comme
  « avant 9.3.5 » : pas de fausse alerte. FileZilla n'a pas de fiche winget et son
  site refuse les robots (403) : sa version est vérifiée via Chocolatey.
- **Retour arrière** : winget, Microsoft Store et Chocolatey (`choco::list_versions`).
- **Vérification planifiée** : tâche « MAJestic UpdateCheck » (XML UTF-16 LE avec BOM,
  `StartWhenAvailable`, lance `--check`). Toasts via l'AUMID `MAJestic.UpdateAssistant`
  et le protocole `majestic:` (HKCU) ; désactiver la planification supprime tout.
  La tâche n'est écrite **que** par `set_schedule` (boutons Activer / Enregistrer /
  Réparer / Désactiver des Paramètres) : jamais au démarrage ni par `save_settings`
  (alerte Defender `Behavior:Win32/Persistence.A!ml` en 0.1.2). Récapitulatif par
  `schedule_status` ; résultat du dernier `--check` dans `last-check.json`.
  **La tâche n'est écrite que quand l'utilisateur change l'option**, jamais au
  lancement : Microsoft Defender a classé MAJestic « Behavior:Win32/Persistence.A!ml »
  (20/10/2026) parce qu'il recréait sa tâche à chaque démarrage, et a supprimé l'exe
  et la tâche. Au démarrage, `app_info` lit seulement la tâche : absente → option
  désactivée + message (`scheduleNotice: missing`) ; autre copie → message (`moved`).
- **Pluriels français** : `Intl.PluralRules('fr')` range 0 dans « one » ; `i18n.js`
  utilise la forme `zero` (ou `other`) pour 0.
- **Métadonnées de l'exe** : `bundle.publisher` et `bundle.copyright` de
  `app/tauri.conf.json` (sinon CompanyName vaut « github », tiré de l'identifiant).

## Pièges de l'environnement (sandbox de Claude)

- Un MAJestic lancé depuis Bash écrit ses réglages dans une couche isolée, invisible
  du Planificateur de tâches. Pour tester la vérification planifiée, lancer l'exe via
  `explorer.exe`.
- Même chose en lecture : la session voit sa propre copie de
  `%LOCALAPPDATA%\MAJestic` (journaux anciens). Pour lire les vrais `logs\*.log` et
  `history.jsonl`, les faire copier par un `.bat` lancé via `explorer.exe` dans
  `.claude\` (ignoré par git), puis supprimer la copie. Les journaux de winget
  (`%LOCALAPPDATA%\Packages\Microsoft.DesktopAppInstaller_8wekyb3d8bbwe\LocalState\DiagOutputDir`)
  se lisent directement.
- Depuis Bash, écrire dans Images ou Bureau échoue ; Téléchargements fonctionne.
- ImageMagick : un préfixe `PNG24:` empêche la conversion du chemin `/c/…` ; passer
  par `cygpath -m`.
- Pour rendre un SVG en PNG : Chrome sans fenêtre (`--headless --screenshot`)
  fonctionne, Edge non. Un profil (`--user-data-dir`) distinct par rendu.

## Publication

- Dépôt public `Belenos-Toutatis/MAJestic`, licence MIT.
- `.github/workflows/build.yml` (windows-latest) : tests, compilation, artefact.
  Un tag `v*` crée la Release (`MAJestic.exe` + `MAJestic.exe.sha256`, notes tirées de
  `.github/release-notes.md`). Le tag doit égaler la version de `app/tauri.conf.json`
  (vérifié par la CI) : changer la version **là et** dans `Cargo.toml`
  (`[workspace.package]`).
- **Signature SignPath Foundation** : demande envoyée le 19/10/2026, en attente de
  réponse. L'étape de signature est en veille et s'active quand la variable de dépôt
  `SIGNPATH_ORGANIZATION_ID` existe (secret `SIGNPATH_API_TOKEN` : saisi par
  l'utilisateur lui-même, jamais par Claude). Configuration d'artefact :
  `.signpath/artifact-configuration.xml`. Après acceptation : retirer « Application in
  progress » du README et de `.github/release-notes.md`. Chaque tag attend ensuite
  une approbation manuelle dans SignPath (délai maximal 1 h).
