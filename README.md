<p align="center">
  <img src="ui/assets/logo.svg" width="96" alt="MAJestic logo">
</p>

<h1 align="center">MAJestic</h1>

<p align="center">
  <b>Toutes vos mises à jour, majestueusement.</b><br>
  <i>All your software updates, majestically.</i>
</p>

<p align="center">
  <a href="https://github.com/Belenos-Toutatis/MAJestic/releases/latest"><b>⬇ Télécharger / Download MAJestic.exe</b></a>
</p>

<p align="center">
  <img src="docs/screenshots/results-light.png" width="49%" alt="Liste des mises à jour">
  <img src="docs/screenshots/progress-dark.png" width="49%" alt="Mise à jour en cours">
</p>

---

🇫🇷 [Français](#-français) · 🇬🇧 [English](#-english)

## 🇫🇷 Français

**MAJestic** met à jour en une fois tous les logiciels de votre PC Windows 11.
C'est un programme **portable** : un seul fichier `.exe`, rien à installer.

### Ce qu'il fait

- **Trouve les mises à jour** de vos logiciels via **winget** (le gestionnaire de
  paquets de Windows), le **Microsoft Store**, **Chocolatey** et **Scoop** s'ils
  sont présents.
- **Vous conseille** : chaque mise à jour est classée.
  - 🔴 **Critique** : faille de sécurité activement exploitée (catalogue CISA KEV)
    ou correctif de sécurité annoncé dans les notes de version ;
  - 🟣 **Évolution majeure** : nouvelle version majeure, à décider en connaissance de cause ;
  - 🟠 **Recommandée** : logiciel exposé à Internet (navigateur, lecteur PDF, messagerie…) ;
  - ⚪ **Courante** ; 🔘 **À vérifier** quand la version installée est inconnue.
- **Installe tout en un clic**, ou seulement votre sélection, avec **une seule
  demande d'autorisation administrateur** pour tout le lot.
- **Ferme puis relance** les logiciels ouverts qui bloqueraient leur mise à jour
  (après vous avoir demandé), sans jamais les relancer en administrateur.
- **Progression en direct** : barre globale et barre par logiciel, avec un
  **temps restant réaliste** (taille réelle des installateurs, débit mesuré,
  durées observées sur votre PC lors des mises à jour précédentes).
- **Gère les cas difficiles** :
  - version installée inconnue → lecture de la version dans le fichier du
    programme, ou réinstallation de la dernière version ;
  - logiciel inconnu des gestionnaires de paquets → recherche d'une version plus
    récente (GitHub, catalogue Chocolatey), lien vers le site de l'éditeur ;
  - jeux, applications web, pilotes, logiciels qui se mettent à jour seuls →
    expliqués et rangés à part ;
  - téléchargement refusé, application ouverte, redémarrage nécessaire… →
    explication claire et solution proposée.
- **Windows Update aussi** : affiche les mises à jour de Windows en attente
  (sécurité, pilotes, antivirus Defender) et signale un redémarrage nécessaire,
  avec un bouton qui ouvre Windows Update et lance la recherche. C'est Windows qui
  les installe, comme d'habitude.
- **Désinstaller proprement** : le panneau de chaque logiciel propose la bonne
  méthode (winget, Chocolatey ou Scoop quand ils l'ont installé, sinon le
  désinstalleur du logiciel lui-même), plus les écrans de Windows « Applications
  installées » et « Programmes et fonctionnalités ».
- **Revenir à une version précédente** depuis l'historique (winget et Chocolatey).
- **Vérification planifiée** (quotidienne, hebdomadaire ou mensuelle) avec une
  notification Windows quand une mise à jour importante est disponible.
- **Se met à jour lui-même** : quand une nouvelle version de MAJestic sort, un
  bouton « Nouvelle version » apparaît ; un clic suffit (empreinte SHA-256
  vérifiée avant de remplacer quoi que ce soit, réglages et historique conservés).
- **Aucune aide nécessaire** : chaque élément de l'interface s'explique au
  survol. Thème clair, sombre ou automatique ; français et anglais.

### Captures d'écran

| | |
|---|---|
| ![Détail d'une mise à jour critique](docs/screenshots/details-light.png) | ![Bulles d'aide au survol](docs/screenshots/tooltip-light.png) |
| *Le détail explique pourquoi une mise à jour compte.* | *Chaque élément s'explique au survol.* |
| ![Logiciels hors gestionnaire de paquets](docs/screenshots/others-light.png) | ![Bilan des mises à jour](docs/screenshots/summary-dark.png) |
| *Les logiciels qu'aucun outil ne gère sont rangés et expliqués.* | *Le bilan propose une solution pour chaque échec.* |

### Utilisation

1. Téléchargez `MAJestic.exe` depuis les [Releases](https://github.com/Belenos-Toutatis/MAJestic/releases/latest).
2. Lancez-le (voir [Premier lancement](#premier-lancement) si Windows affiche un
   écran bleu). L'analyse démarre toute seule (quelques secondes).
3. Vérifiez la sélection proposée, puis cliquez sur **Mettre à jour**.

**Prérequis** : Windows 10 ou 11 avec winget (« Programme d'installation
d'application », présent par défaut sur Windows 11) et WebView2 (présent par défaut).

### Premier lancement

Au premier lancement, Windows peut afficher un écran bleu **« Windows a protégé
votre ordinateur »**. C'est normal pour un programme récent : MAJestic n'est pas
encore signé numériquement et peu de personnes l'ont lancé jusqu'ici, alors
Windows ne le connaît pas. Cela ne veut pas dire qu'il est dangereux.

Pour le lancer :

1. cliquez sur **Informations complémentaires** ;
2. vérifiez que l'application s'appelle bien `MAJestic.exe`, puis cliquez sur
   **Exécuter quand même**.

Pour ne plus voir cet écran avec ce fichier : clic droit sur `MAJestic.exe` →
**Propriétés** → cochez **Débloquer** → **OK**. Pour la même raison, la fenêtre
d'autorisation administrateur indique « Éditeur : inconnu ».

**Vérifier que le fichier est authentique** (facultatif). L'exécutable est
compilé par [GitHub Actions](https://github.com/Belenos-Toutatis/MAJestic/actions)
directement à partir du code de ce dépôt, et chaque version publie son empreinte
dans `MAJestic.exe.sha256`. Dans PowerShell, depuis le dossier du fichier :

```powershell
(Get-FileHash .\MAJestic.exe -Algorithm SHA256).Hash
```

Le résultat doit être identique (aux majuscules près) au contenu du fichier `.sha256`.

**Contrôle intelligent des applications** : si cette protection de Windows 11 est
activée sur votre PC, elle bloque les programmes non signés sans proposer de les
exécuter.

**Bientôt signé** : une signature de code gratuite a été demandée à SignPath
Foundation (voir [Code signing policy](#code-signing-policy)). Une fois signé,
MAJestic affichera un éditeur vérifié et l'avertissement devrait disparaître
progressivement.

### Désinstaller

MAJestic ne s'installe pas. Pour l'enlever complètement :

1. si vous avez activé la **vérification planifiée**, désactivez-la dans les
   Paramètres : cela supprime la tâche Windows et l'inscription des notifications ;
2. supprimez `MAJestic.exe` et le dossier `%LOCALAPPDATA%\MAJestic` (réglages,
   historique, journaux).

### Confidentialité

MAJestic n'a ni compte, ni publicité, ni mesure d'audience. Réglages, historique
et journaux restent sur votre PC, dans `%LOCALAPPDATA%\MAJestic`.

Pour vous conseiller, il consulte des sources publiques :

| Service | Pourquoi | Ce que contient la requête |
|---|---|---|
| `www.cisa.gov` | Catalogue des failles activement exploitées (KEV) | Rien sur vos logiciels : le catalogue entier est téléchargé |
| `raw.githubusercontent.com`, `api.github.com` | Manifestes winget (notes de version, installateur) ; dernières versions publiées sur GitHub | L'identifiant d'**un seul** logiciel |
| `community.chocolatey.org` | Catalogue Chocolatey, pour les logiciels qu'aucun gestionnaire ne suit | Le nom d'**un seul** logiciel |
| Serveur de l'éditeur | Taille de l'installateur, pour estimer le temps restant (requête `HEAD`, sans téléchargement) | L'adresse officielle de l'installateur, lue dans le manifeste winget |
| `api.github.com`, `github.com` | Savoir s'il existe une nouvelle version de MAJestic, et la télécharger si vous le demandez | Rien sur vos logiciels |
| Service Windows Update (Windows lui-même) | Savoir quelles mises à jour de Windows sont en attente | MAJestic ne contacte rien : c'est le service Windows Update qui interroge Microsoft, comme lors d'une recherche dans les Paramètres |

**La liste de vos logiciels n'est jamais envoyée.** Chaque requête ne porte que
sur un logiciel et ne contient aucun identifiant personnel ; comme toute
connexion, elle laisse voir votre adresse IP au service contacté. Les options
« Utiliser les sources en ligne », « Chercher les versions des logiciels non
gérés » et « Afficher les mises à jour de Windows » des Paramètres désactivent ces
requêtes. Les mises à jour elles-mêmes
sont téléchargées par winget, le Microsoft Store, Chocolatey ou Scoop, depuis
leurs sources habituelles, et seulement quand vous les lancez.

## 🇬🇧 English

**MAJestic** updates every program on your Windows 11 PC in one go. It is
**portable**: a single `.exe`, nothing to install.

- Finds updates through **winget**, the **Microsoft Store**, **Chocolatey** and **Scoop**.
- Ranks them: **critical** (actively exploited vulnerabilities from the CISA KEV
  catalog, security fixes in release notes), **major**, **recommended**
  (Internet-exposed software), **normal**, **to check** (unknown installed version).
- Installs everything or your selection with **a single UAC prompt**; closes and
  relaunches running programs (never elevated).
- Live progress per program and overall, with a **realistic time remaining**
  learned from your own PC.
- Handles hard cases: unknown versions (read from the executable), programs no
  package manager knows (GitHub / Chocolatey lookups, publisher site), refused
  downloads, running apps, reboots.
- Roll back to a previous version from the history; scheduled checks with Windows
  notifications; light/dark/auto theme; French and English; every control
  explains itself with a tooltip.
- Updates itself in one click when a new version is out (SHA-256 checked before
  anything is replaced; settings and history kept).
- Uninstalls the right way from each program's panel: winget, Chocolatey or Scoop
  when they installed it, otherwise the program's own uninstaller, plus Windows'
  “Installed apps” and “Programs and Features” screens.
- Shows what Windows Update has waiting (security, drivers, Defender) and pending
  restarts, with a button that opens Windows Update and starts a search; Windows
  installs them as usual.
- Privacy: public catalogs are downloaded; your software list is never sent anywhere.

Download `MAJestic.exe` from the [Releases](https://github.com/Belenos-Toutatis/MAJestic/releases/latest)
and run it.

### First launch

Windows may show a blue **“Windows protected your PC”** screen. This is normal for
a new program: MAJestic is not code-signed yet and few people have run it so far,
so Windows does not know it. It does not mean the file is harmful.

1. Click **More info**.
2. Check that the app is `MAJestic.exe`, then click **Run anyway**.

To stop seeing this screen for that file: right-click `MAJestic.exe` →
**Properties** → tick **Unblock** → **OK**. For the same reason, the UAC prompt
shows “Publisher: Unknown”.

**Check the file** (optional). The executable is built by
[GitHub Actions](https://github.com/Belenos-Toutatis/MAJestic/actions) from this
repository's source code, and each release publishes its hash in
`MAJestic.exe.sha256`. In PowerShell, in the file's folder:

```powershell
(Get-FileHash .\MAJestic.exe -Algorithm SHA256).Hash
```

The output must match the content of the `.sha256` file (case aside).

**Smart App Control**: when this Windows 11 protection is on, it blocks unsigned
programs without offering to run them.

**Signing is coming**: free code signing has been requested from SignPath
Foundation (see [Code signing policy](#code-signing-policy)). Once signed,
MAJestic will show a verified publisher and the warning should fade out over time.

### Uninstall

MAJestic installs nothing. To remove it completely:

1. if you turned on the **scheduled check**, turn it off in the Settings: this
   deletes the Windows scheduled task and the notification registration;
2. delete `MAJestic.exe` and the `%LOCALAPPDATA%\MAJestic` folder (settings,
   history, logs).

### Privacy

MAJestic has no account, no ads and no analytics. Settings, history and logs stay
on your PC, in `%LOCALAPPDATA%\MAJestic`.

To advise you, it reads public sources:

| Service | Why | What the request contains |
|---|---|---|
| `www.cisa.gov` | Catalog of actively exploited vulnerabilities (KEV) | Nothing about your software: the whole catalog is downloaded |
| `raw.githubusercontent.com`, `api.github.com` | winget manifests (release notes, installer); latest releases published on GitHub | The identifier of **one** program |
| `community.chocolatey.org` | Chocolatey catalog, for programs no package manager tracks | The name of **one** program |
| Publisher's server | Installer size, to estimate the time remaining (`HEAD` request, nothing is downloaded) | The official installer address, read from the winget manifest |
| `api.github.com`, `github.com` | Whether a new version of MAJestic exists, and downloading it when you ask | Nothing about your software |
| Windows Update service (Windows itself) | Which Windows updates are waiting | MAJestic contacts nothing: the Windows Update service asks Microsoft, as when you search in Settings |

**Your software list is never sent.** Each request is about a single program and
carries no personal identifier; like any connection, it reveals your IP address to
the service contacted. The “Use online sources”, “Look up versions of unmanaged
software” and “Show Windows updates” settings turn these requests off. The updates themselves are downloaded
by winget, the Microsoft Store, Chocolatey or Scoop from their usual sources, and
only when you start them.

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io),
certificate by [SignPath Foundation](https://signpath.org).
*Application in progress: until it is accepted, releases are not signed.*

- **What is signed**: only `MAJestic.exe`, built by the
  [GitHub Actions workflow](.github/workflows/build.yml) of this repository on
  GitHub-hosted runners, from the tagged source code. Every signing request is
  approved manually.
- **Team roles**
  - Committers and reviewers: [Belenos-Toutatis](https://github.com/Belenos-Toutatis)
  - Approvers: [Belenos-Toutatis](https://github.com/Belenos-Toutatis)
- **Privacy**: MAJestic only contacts the systems listed in [Privacy](#privacy),
  and these requests can be turned off in the settings.
- **System changes**: MAJestic installs updates, closes programs or creates its
  scheduled task only after you confirm it in the interface; see
  [Uninstall](#uninstall) to remove everything.

## Développement / Development

Rust (engine) + [Tauri 2](https://tauri.app) (WebView2 UI, plain HTML/CSS/JS, no build step).

```bash
cargo test --workspace
cargo build --release -p majestic   # → target/release/MAJestic.exe
```

The UI can be previewed in a browser with a mock backend: serve `ui/` with any
static server (`python -m http.server -d ui`) and open it.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design (modules,
UI ↔ backend API, elevated worker, time estimates).

## Licence / License

[MIT](LICENSE) © 2026 Belenos Toutatis
