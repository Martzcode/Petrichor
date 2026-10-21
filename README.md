# Petrichor

<details>
<summary><b>English</b></summary>

Petrichor is a desktop customization tool for Linux. It lets you create, install and bring to
life **skins**: sets of measures (CPU, memory, network, media, time…) and graphic elements
(text, bars, graphs, images, shapes…) rendered in transparent windows placed on the desktop.

## Tech stack

- **Angular** — user interface and skin rendering (WebView).
- **Tauri** — application shell, windows, system tray, OS integration.
- **Rust** — engine: parsing, measures, plugins, system integrations.

## Features

- `.ini`-based skinning language (variables, formulas, actions).
- Many measure and meter types, command bangs, mouse actions.
- Native plugin system (`.so`) and embedded scripting.
- Media library via MPRIS, native Linux network/audio/system integrations (X11 & Wayland).
- Skin and layout management, Game Mode, system tray, installer and package creation.
- Multilingual localization.

## Documentation

The full specification is available in [`docs/cahier-des-charges.md`](docs/cahier-des-charges.md).
The detailed development plan (phases, tasks, MVP) is available in
[`docs/plan-de-developpement.md`](docs/plan-de-developpement.md).

## Requirements

- Rust (stable) and Cargo
- Node.js 22+ and npm
- A Linux desktop environment (X11 or Wayland)
- System dependencies (Debian/Ubuntu):

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libssl-dev build-essential pkg-config
```

## Development

```bash
npm install
npm run desktop:dev     # Angular server + Tauri window (hot reload)
```

To run only the web part (without Tauri): `npm run start` then open
<http://localhost:4200>. Without the Tauri backend, the application switches to browser mode.

From the main interface, the **Skins** section lists the `skins-demo/` corpus; each button
toggles the skin display: the first click creates the window, another click hides it, and a
final click shows it again. A given skin therefore never has more than one window. The
**Close** button destroys the window and clears the skin state (measures, variables, options).
Each skin window is an undecorated Tauri window, automatically resized
(`DynamicWindowSize`).

In a development build, the `PETRICHOR_OPEN_SKIN` variable opens one or more skins at startup
(comma-separated paths) and lets you check rendering without interaction:

```bash
PETRICHOR_OPEN_SKIN="$PWD/skins-demo/Clock/Clock.ini,$PWD/skins-demo/Counter/Counter.ini" \
  npm run desktop:dev
```

`PETRICHOR_TEST_BANG` executes a bang chain at startup, targeted at a window
(`<label>|<bangs>`):

```bash
PETRICHOR_TEST_BANG='skin-1|!Log "test"[!Move "300" "200"][!ZPos "1"]'
```

Both variables are compiled in debug, or in release with the `dev-harness` feature:

```bash
cargo build --release -p petrichor --features custom-protocol,dev-harness
```

## Skin rendering

The engine computes a _render model_ (`RenderModel`) in Rust: options merged with `MeterStyle`,
resolved variables, geometry (absolute `X`/`Y`, `r`, `R`), background and measure values. The
Angular frontend (`src/renderer/`) computes the layout then draws on a `canvas` (background,
`String`, `Image`, `Bar`, `Histogram`, `Line`, `Rotator`, `RoundLine`, `Bitmap`, `Button` and
`Shape` meters). `String` supports inline formatting (`InlineSetting`/`InlinePattern`),
`Shape` supports `Rectangle`/`Ellipse`/`Line`/`Arc`/`Curve`/`Path`, per-shape
`Offset`/`Rotate`/`Scale`/`Skew` and nested `Combine` modes (`Union`, `Xor`, `Intersect`,
`Exclude`). Nested `Container` meters clip recursively, `TransformationMatrix` transforms them,
`BevelType`/`AntiAlias` control background rendering, images support `Crop`, `Greyscale`,
`TintColor`, `ColorMatrix`, `EXIF`, `Flip`, `Rotate`, `ScaleMargins` (9-slice) and
`PreserveAspectRatio`, `BitmapTransitionFrames` animates bitmap frames, `Button` hit-testing
samples image alpha, and `ToolTip*` options display a tooltip on hover. Meters not yet supported are logged as such and ignored during
rendering.

```bash
./scripts/snapshot.sh           # rendering snapshots (draw command lists)
```

## Measures

The engine (`crates/petrichor-core/src/measure.rs`) maintains per-skin state and updates values
on each cycle. Supported measures: `Time`, `Uptime`, `CPU`, `Memory`, `PhysicalMemory`,
`SwapMemory`/`VirtualMemory`, `FreeDiskSpace`, `NetIn`/`NetOut`/`NetTotal`, `Process`,
`RecycleManager`, `SysInfo`, `WifiStatus`, `WebParser`, `Registry`, `MediaKey`, `String`,
`Calc` and `Loop`.

Common options: `UpdateDivider`, `Disabled`, `Paused`, `MinValue`, `MaxValue`,
`InvertMeasure`, `AverageSize` (sliding average), `Substitute` and `RegExpSubstitute`
(`"pattern":"replacement"` pairs), `OnUpdateAction`/`OnChangeAction`, `IfAbove*`, `IfBelow*`,
`IfEqual*`, `IfCondition[N]`/`IfTrueAction[N]`/`IfFalseAction[N]` (with `IfConditionMode`) and
`IfMatch[N]`/`IfMatchAction[N]`/`IfNotMatchAction[N]` (with `IfMatchMode`). `Time` accepts
`Format` (`chrono` syntax, default `%H:%M:%S`), `CPU` accepts `Processor` (1 = first core,
0/absent = global), `Loop` accepts `Start`/`Step`/`End`/`Reset`, and `Calc` accepts `Formula`
(measure references, `counter`).

`FreeDiskSpace` uses `Drive`, `Type`, `Label`, `Total`, `IgnoreRemovable` and `DiskQuota`;
`Net*` use `Interface` (`BEST` by default), `Cumulative`, `UseBits`, `NetInSpeed`/`NetOutSpeed`
and persist cumulative totals under `$XDG_STATE_HOME/petrichor/`. `Process` uses
`ProcessName`; `RecycleManager` uses `RecycleType` (`COUNT`/`SIZE`) and supports
`!EmptyBin`/`!EmptyBinSilent`/`!OpenBin`. `SysInfo` uses `SysInfoType`/`SysInfoData` (Annex A;
monitor geometry comes from `xrandr`, with `PETRICHOR_SCREEN_GEOMETRY`/`PETRICHOR_SCREEN_SIZE`
as fallback; idle time uses `xprintidle`, XScreenSaver, Mutter or logind hints, logon time comes
from logind and sleep/wake times from the journal in the background). `WifiStatus` uses
`WifiIntfID`/`WifiInfoType`/`WifiListStyle`/`WifiListLimit`, backed by NetworkManager D-Bus and
`iw` (with `nmcli` fallback). `WebParser` supports asynchronous `Url` fetches (no UI blocking),
parent/child chaining (`Url=[Parent]`), `Header*`, `RegExp` (PCRE-style via `fancy-regex`),
`StringIndex`/`StringIndex2`, `UpdateRate`, `CodePage`, `DecodeCharacterReference`,
`DecodeCodePoints`, `Download`/`DownloadFile`, `ErrorString`, `Debug`/`Debug2File`, `Flags`
(`ForceReload`, `PragmaNoCache`, redirect, secure, certificate and `TempFile` flags) and its
error/finish actions. `Registry` reads the virtual registry in
`$XDG_CONFIG_HOME/petrichor/registry.json` (override with `PETRICHOR_REGISTRY`). `MediaKey`
responds to `!CommandMeasure "Measure" "PlayPause"` through MPRIS D-Bus, with PipeWire
(`wpctl`), PulseAudio (`pactl`) and ALSA (`amixer`) fallbacks for volume.

Values are exposed through the `read_measures` IPC as a snapshot (`strings` for display and
normalized `values` for graphs), then fed back into rendering on each `Update` interval. `OnUpdateAction` and `OnChangeAction` actions are executed by the bang engine (see
below).

### Known limitations

- `SysInfo` `IDLE_TIME` relies on `xprintidle`, XScreenSaver, Mutter or logind hints. KDE
  Wayland does not expose idle seconds, so the value stays `0` when none of these answers.
- `LAST_SLEEP_TIME`/`LAST_WAKE_TIME` are read asynchronously from the systemd journal; the
  first tick returns `0`. Timestamps use Rainmeter's 1601 epoch.
- `WifiStatus` gets RX/TX rates and PHY from `iw` when available; without it, D-Bus still
  provides TX and an estimated PHY, while `RXRATE` is `0`.
- `WebParser` accepts `NoCookies`, `NoCacheWrite`, `NoAuth`, `Hyperlink` and `Resync` but does
  not act on them. Downloads and HTTP fetches run outside the render thread.
- `MediaKey` playback uses MPRIS D-Bus; volume relies on PipeWire, PulseAudio or ALSA tools.
- `Registry` is a JSON-backed virtual registry; GSettings/dconf mapping is planned for Phase 10.

## Bangs and interactions

The bang parser (`crates/petrichor-core/src/bang.rs`) accepts `[…]` strings, quoted arguments
and the `!Rainmeter` prefix. MVP bangs are executed by `src-tauri/src/bangs.rs`:

| Bang                                                                            | Effect                                  |
| ------------------------------------------------------------------------------- | --------------------------------------- |
| `!Refresh`, `!Redraw`, `!Update`                                                | Reload, redraw or immediate update      |
| `!Hide`, `!Show`, `!Toggle`                                                     | Window visibility                       |
| `!Move`, `!ZPos`, `!ClickThrough`, `!Draggable`                                 | Window geometry and options             |
| `!SetVariable`, `!SetOption`, `!CommandMeasure`                                 | Variable/option change, measure command |
| `!EmptyBin`, `!EmptyBinSilent`, `!OpenBin`                                      | Empty or open the XDG trash             |
| `!Log`, `!About`, `!Manage`                                                     | Log, About window or management         |
| `!Quit`, `!RefreshApp`, `!ActivateConfig`, `!DeactivateConfig`, `!ToggleConfig` | Lifecycle                               |

External bangs (`Bangs/` folder) are ignored with a warning. `#Variable#` references in
arguments are resolved at execution time; `!SetOption` accepts a value containing spaces.

Supported mouse actions: `LeftMouseUpAction`, `MouseOverAction` and `MouseLeaveAction`. A
draggable skin can be dragged with the mouse (hold the click); its position is saved on release.

## Persistence and About window

Skin state (position, `AlwaysOnTop`, `ClickThrough`, `Draggable`, `SavePosition`) is stored in
`$XDG_CONFIG_HOME/Petrichor/Petrichor.ini` (or `~/.config/Petrichor/…`), with one section per
skin (`Folder\File.ini`). It is saved on drag release and via `!Move`.

The **About** window (main menu) displays the version and the configuration, skins and log paths
provided by `get_paths`.

> Under Wayland, a client cannot position its own windows: `!Move` and position restoration are
> effective under X11, while the compositor decides the position under Wayland.

## Demo skins

`skins-demo/` contains the MVP recipe:

- `Clock/Clock.ini` — `Time` clock updated every second;
- `System/Monitor.ini` — CPU and memory (`CPU`, `Memory`, `Bar`);
- `System/Resources.ini` — disk, network, hostname and Wi-Fi (`FreeDiskSpace`, `NetIn`/`NetOut`,
  `SysInfo`, `WifiStatus`);
- `Counter/Counter.ini` — animated counter (`Calc`, `Loop`, `Update=250`); a click logs
  "Counter clicked" and hovering logs the cycle.

All four enable `!Draggable "1"` via `OnRefreshAction`, so they can be dragged directly with a
held left click, without prior configuration.

## Performance

Release measurements on a 4-core machine (3 skins at `Update=1000`, 20 s warm-up, median of
3 × 10 s samples): about **4–5 % of one core** (≈ 1.1 % of the machine), **930 MB RSS** total
(including the WebKitGTK process footprint). An inactive skin window uses less than 0.2 %.
Dynamic resizing is applied only when the size changes, and the canvas is reallocated only when
dimensions change.

To check a build embedding the frontend (without the Angular server):

```bash
npm run build
cargo run -p petrichor --features custom-protocol
```

## Repository structure

```
crates/
  petrichor-core/     # configuration, skin model, measures, meters, bangs
  petrichor-sys/      # Linux backends (CPU, memory, network, disk, media…)
  petrichor-plugin/   # native plugin host (C ABI)
  petrichor-script/   # script engine integration (Lua)
  petrichor-compat/   # compatibility layer (registry, aliases, mapping)
src-tauri/            # Tauri shell: windows, tray, IPC, menus
src/                  # Angular: app/, renderer/, shared/
```

## Tests and checks

```bash
cargo test --workspace          # Rust unit and integration tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check

npm run test:ci                 # Angular unit tests (Vitest)
npm run build                   # production frontend build
npm run format:check            # formatting check (Prettier)
```

CI (GitHub Actions) runs all these checks, see
[`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## License

See the [LICENSE](LICENSE) file.

</details>

<details>
<summary><b>Français</b></summary>

Petrichor est un outil de personnalisation du bureau pour Linux. Il permet de créer, installer et
faire vivre des **skins** : des ensembles de mesures (CPU, mémoire, réseau, média, temps…) et
d'éléments graphiques (texte, barres, courbes, images, formes…) rendus dans des fenêtres
transparentes posées sur le bureau.

## Stack technique

- **Angular** — interface utilisateur et rendu des skins (WebView).
- **Tauri** — coquille applicative, fenêtres, system tray, intégration OS.
- **Rust** — moteur : parsing, mesures, plugins, intégrations système.

## Fonctionnalités

- Langage de skinning basé sur des fichiers `.ini` (variables, formules, actions).
- Nombreux types de mesures et de meters, bangs de commande, actions souris.
- Système de plugins natifs (`.so`) et scripting embarqué.
- Médiathèque via MPRIS, intégrations réseau/audio/système natives Linux (X11 & Wayland).
- Gestion des skins et des layouts, Game Mode, system tray, installateur et création de paquets.
- Localisation multilingue.

## Documentation

Le cahier des charges complet se trouve dans [`docs/cahier-des-charges.md`](docs/cahier-des-charges.md).
Le plan de développement détaillé (phases, tâches, MVP) se trouve dans
[`docs/plan-de-developpement.md`](docs/plan-de-developpement.md).

## Prérequis

- Rust (stable) et Cargo
- Node.js 22+ et npm
- Un environnement de bureau Linux (X11 ou Wayland)
- Dépendances système (Debian/Ubuntu) :

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libssl-dev build-essential pkg-config
```

## Développement

```bash
npm install
npm run desktop:dev     # serveur Angular + fenêtre Tauri (rechargement à chaud)
```

Pour ne lancer que la partie web (sans Tauri) : `npm run start` puis ouvrir
<http://localhost:4200>. Sans backend Tauri, l'application bascule en mode navigateur.

Depuis l'interface principale, la section **Skins** liste le corpus `skins-demo/` ; chaque bouton
bascule l'affichage du skin : le premier clic crée la fenêtre, un nouveau clic la masque, un
dernier clic la réaffiche. Un même skin n'a donc jamais qu'une seule fenêtre. Le bouton
**Fermer** détruit la fenêtre et purge l'état du skin (mesures, variables, options). Chaque
fenêtre de skin est un Tauri sans décor, redimensionnée automatiquement
(`DynamicWindowSize`).

En build de développement, la variable `PETRICHOR_OPEN_SKIN` ouvre un ou plusieurs skins au
démarrage (chemins séparés par des virgules) et permet de vérifier le rendu sans interaction :

```bash
PETRICHOR_OPEN_SKIN="$PWD/skins-demo/Clock/Clock.ini,$PWD/skins-demo/Counter/Counter.ini" \
  npm run desktop:dev
```

`PETRICHOR_TEST_BANG` exécute une chaîne de bangs au démarrage, ciblée sur une fenêtre
(`<label>|<bangs>`) :

```bash
PETRICHOR_TEST_BANG='skin-1|!Log "test"[!Move "300" "200"][!ZPos "1"]'
```

Ces deux variables sont compilées en debug, ou en release avec la feature `dev-harness` :

```bash
cargo build --release -p petrichor --features custom-protocol,dev-harness
```

## Rendu des skins

Le moteur calcule en Rust un _modèle de rendu_ (`RenderModel`) : options fusionnées avec les
`MeterStyle`, variables résolues, géométrie (`X`/`Y` absolu, `r`, `R`), fond et valeurs de
mesures. Le frontend Angular (`src/renderer/`) calcule la mise en page puis dessine sur un
`canvas` (fond, meters `String`, `Image`, `Bar`, `Histogram`, `Line`, `Rotator`, `RoundLine`,
`Bitmap`, `Button` et `Shape`). Les `String` acceptent le formatage inline
(`InlineSetting`/`InlinePattern`), les `Shape` gèrent `Rectangle`/`Ellipse`/`Line`/`Arc`/`Curve`/
`Path`, les transformations `Offset`/`Rotate`/`Scale`/`Skew` et les combinaisons imbriquées
`Union`, `Xor`, `Intersect`, `Exclude`. Les `Container` imbriqués découpent récursivement les
enfants, `TransformationMatrix` les transforme, `BevelType`/`AntiAlias` contrôlent le fond, les
images acceptent `Crop`, `Greyscale`, `TintColor`, `ColorMatrix`, `EXIF`, `Flip`, `Rotate`,
`ScaleMargins` (9 tranches) et `PreserveAspectRatio`, `BitmapTransitionFrames` anime les frames
Bitmap, le hit-test des `Button` échantillonne l'alpha de l'image, et les options `ToolTip*`
affichent une infobulle au survol. Les meters non encore supportés sont journalisés comme tels et ignorés
au rendu.

```bash
./scripts/snapshot.sh           # instantanés de rendu (listes de commandes de dessin)
```

## Mesures

Le moteur (`crates/petrichor-core/src/measure.rs`) maintient un état par skin et met à jour les
valeurs à chaque cycle. Mesures prises en charge : `Time`, `Uptime`, `CPU`, `Memory`,
`PhysicalMemory`, `SwapMemory`/`VirtualMemory`, `FreeDiskSpace`, `NetIn`/`NetOut`/`NetTotal`,
`Process`, `RecycleManager`, `SysInfo`, `WifiStatus`, `WebParser`, `Registry`, `MediaKey`,
`String`, `Calc` et `Loop`.

Options communes : `UpdateDivider`, `Disabled`, `Paused`, `MinValue`, `MaxValue`,
`InvertMeasure`, `AverageSize` (moyenne glissante), `Substitute` et `RegExpSubstitute`
(paires `"motif":"remplacement"`), `OnUpdateAction`/`OnChangeAction`, `IfAbove*`, `IfBelow*`,
`IfEqual*`, `IfCondition[N]`/`IfTrueAction[N]`/`IfFalseAction[N]` (avec `IfConditionMode`) et
`IfMatch[N]`/`IfMatchAction[N]`/`IfNotMatchAction[N]` (avec `IfMatchMode`). `Time` accepte
`Format` (syntaxe `chrono`, par défaut `%H:%M:%S`), `CPU` accepte `Processor` (1 = premier
cœur, 0/absent = global), `Loop` accepte `Start`/`Step`/`End`/`Reset`, et `Calc` accepte
`Formula` (références aux mesures, `counter`).

`FreeDiskSpace` utilise `Drive`, `Type`, `Label`, `Total`, `IgnoreRemovable` et `DiskQuota` ;
`Net*` utilisent `Interface` (`BEST` par défaut), `Cumulative`, `UseBits`,
`NetInSpeed`/`NetOutSpeed` et persistent les cumuls dans `$XDG_STATE_HOME/petrichor/`.
`Process` utilise `ProcessName` ; `RecycleManager` utilise `RecycleType` (`COUNT`/`SIZE`) et
accepte `!EmptyBin`/`!EmptyBinSilent`/`!OpenBin`. `SysInfo` utilise
`SysInfoType`/`SysInfoData` (Annexe A ; géométrie des moniteurs via `xrandr`, avec
`PETRICHOR_SCREEN_GEOMETRY`/`PETRICHOR_SCREEN_SIZE` en repli ; temps d'inactivité via
`xprintidle`, XScreenSaver, Mutter ou les indications de logind, heure de connexion via logind
et heures de veille/réveil via le journal en arrière-plan). `WifiStatus` utilise
`WifiIntfID`/`WifiInfoType`/`WifiListStyle`/`WifiListLimit`, adossé à NetworkManager D-Bus et
`iw` (repli `nmcli`). `WebParser` gère les `Url` asynchrones (sans bloquer l'interface), le
chaînage parent/enfant (`Url=[Parent]`), `Header*`, `RegExp` (style PCRE via `fancy-regex`),
`StringIndex`/`StringIndex2`, `UpdateRate`, `CodePage`, `DecodeCharacterReference`,
`DecodeCodePoints`, `Download`/`DownloadFile`, `ErrorString`, `Debug`/`Debug2File`, `Flags`
(`ForceReload`, `PragmaNoCache`, redirections, secure, certificats et `TempFile`) ainsi que ses
actions d'erreur/fin. `Registry` lit le registre virtuel
`$XDG_CONFIG_HOME/petrichor/registry.json` (surcharge : `PETRICHOR_REGISTRY`). `MediaKey`
répond à `!CommandMeasure "Mesure" "PlayPause"` via MPRIS D-Bus, avec repli PipeWire
(`wpctl`), PulseAudio (`pactl`) et ALSA (`amixer`) pour le volume.

Les valeurs sont exposées via l'IPC `read_measures` sous forme d'instantané (`strings` pour
l'affichage et `values` normalisées pour les graphiques), puis réinjectées dans le rendu à
chaque intervalle `Update`. Les actions `OnUpdateAction` et `OnChangeAction` sont exécutées par le
moteur de bangs (voir ci-dessous).

### Limites connues

- `SysInfo` `IDLE_TIME` s'appuie sur `xprintidle`, XScreenSaver, Mutter ou les indications de
  logind. KDE Wayland n'expose pas le temps d'inactivité en secondes : la valeur reste `0`
  si aucune de ces sources ne répond.
- `LAST_SLEEP_TIME`/`LAST_WAKE_TIME` sont lus de façon asynchrone dans le journal systemd ; le
  premier cycle renvoie `0`. Les horodatages utilisent l'époque Rainmeter (1601).
- `WifiStatus` obtient les débits RX/TX et le PHY via `iw` lorsqu'il est installé ; sinon le
  D-Bus fournit le TX et un PHY estimé, et `RXRATE` vaut `0`.
- `WebParser` accepte `NoCookies`, `NoCacheWrite`, `NoAuth`, `Hyperlink` et `Resync` sans les
  appliquer. Les téléchargements et requêtes HTTP s'exécutent hors du thread de rendu.
- `MediaKey` utilise MPRIS D-Bus pour la lecture ; le volume dépend des outils PipeWire,
  PulseAudio ou ALSA.
- `Registry` est un registre virtuel JSON ; le mapping GSettings/dconf est prévu en Phase 10.

## Bangs et interactions

Le parseur de bangs (`crates/petrichor-core/src/bang.rs`) accepte les chaînes `[…]`, les
arguments entre guillemets et le préfixe `!Rainmeter`. Les bangs MVP sont exécutés par
`src-tauri/src/bangs.rs` :

| Bang                                                                            | Effet                                           |
| ------------------------------------------------------------------------------- | ----------------------------------------------- |
| `!Refresh`, `!Redraw`, `!Update`                                                | Rechargement, redessin ou mise à jour immédiate |
| `!Hide`, `!Show`, `!Toggle`                                                     | Visibilité de la fenêtre                        |
| `!Move`, `!ZPos`, `!ClickThrough`, `!Draggable`                                 | Géométrie et options de fenêtre                 |
| `!SetVariable`, `!SetOption`, `!CommandMeasure`                                 | Variable/option ou commande de mesure           |
| `!EmptyBin`, `!EmptyBinSilent`, `!OpenBin`                                      | Vide ou ouvre la corbeille XDG                  |
| `!Log`, `!About`, `!Manage`                                                     | Journal, fenêtre À propos ou gestion            |
| `!Quit`, `!RefreshApp`, `!ActivateConfig`, `!DeactivateConfig`, `!ToggleConfig` | Cycle de vie                                    |

Les bangs externes (dossier `Bangs/`) sont ignorés avec un avertissement. Les variables
`#Variable#` des arguments sont résolues au moment de l'exécution ; `!SetOption` accepte une
valeur contenant des espaces.

Actions souris prises en charge : `LeftMouseUpAction`, `MouseOverAction` et
`MouseLeaveAction`. Un skin déplaçable peut être glissé à la souris (maintenir le clic) ; sa
position est enregistrée au relâchement.

## Persistance et fenêtre À propos

L'état des skins (position, `AlwaysOnTop`, `ClickThrough`, `Draggable`, `SavePosition`) est
conservé dans `$XDG_CONFIG_HOME/Petrichor/Petrichor.ini` (ou `~/.config/Petrichor/…`), avec une
section par skin (`Dossier\Fichier.ini`). Il est enregistré au relâchement du glisser et via `!Move`.

La fenêtre **À propos** (menu principal) affiche la version et les chemins de configuration,
de skins et de journal fournis par `get_paths`.

> Sous Wayland, un client ne peut pas positionner lui-même ses fenêtres : `!Move` et la
> restauration de position sont effectifs sous X11, alors que le compositeur décide de la
> position sous Wayland.

## Skins de démonstration

`skins-demo/` contient la recette MVP :

- `Clock/Clock.ini` — horloge `Time` actualisée chaque seconde ;
- `System/Monitor.ini` — CPU et mémoire (`CPU`, `Memory`, `Bar`) ;
- `System/Resources.ini` — disque, réseau, nom d'hôte et Wi-Fi (`FreeDiskSpace`,
  `NetIn`/`NetOut`, `SysInfo`, `WifiStatus`) ;
- `Counter/Counter.ini` — compteur animé (`Calc`, `Loop`, `Update=250`) ; un clic journalise
  « Compteur cliqué » et le survol journalise le cycle.

Les quatre activent `!Draggable "1"` via `OnRefreshAction` : ils peuvent donc être déplacés
directement au clic gauche maintenu, sans réglage préalable.

## Performances

Mesures release sur une machine 4 cœurs (3 skins à `Update=1000`, 20 s de chauffe, médiane de
3 échantillons de 10 s) : environ **4 à 5 % d'un cœur** (≈ 1,1 % de la machine), **930 Mo RSS**
cumulés (dont l'empreinte des processus WebKitGTK). Une fenêtre de skin inactive consomme
moins de 0,2 %. Le redimensionnement dynamique n'est appliqué que lorsque la taille change et
le canvas n'est réalloué qu'en cas de changement de dimensions.

Pour vérifier un build embarquant le frontend (sans serveur Angular) :

```bash
npm run build
cargo run -p petrichor --features custom-protocol
```

## Structure du dépôt

```
crates/
  petrichor-core/     # configuration, modèle de skin, mesures, meters, bangs
  petrichor-sys/      # backends Linux (cpu, mémoire, réseau, disque, média…)
  petrichor-plugin/   # hôte de plugins natifs (ABI C)
  petrichor-script/   # intégration du moteur de script (Lua)
  petrichor-compat/   # couche de compatibilité (registre, alias, mapping)
src-tauri/            # coquille Tauri : fenêtres, tray, IPC, menus
src/                  # Angular : app/, renderer/, shared/
```

## Tests et vérifications

```bash
cargo test --workspace          # tests unitaires et d'intégration Rust
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check

npm run test:ci                 # tests unitaires Angular (Vitest)
npm run build                   # build de production du frontend
npm run format:check            # vérification du formatage (Prettier)
```

La CI (GitHub Actions) exécute l'ensemble de ces vérifications, voir
[`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## Licence

Voir le fichier [LICENSE](LICENSE).

</details>
