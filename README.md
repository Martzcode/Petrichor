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
`String`, `Image`, `Bar` meters). Meters not yet supported are logged as such and ignored
during rendering.

```bash
./scripts/snapshot.sh           # rendering snapshots (draw command lists)
```

## Measures

The engine (`crates/petrichor-core/src/measure.rs`) maintains per-skin state and updates values
on each cycle. Supported measures: `Time`, `Uptime`, `CPU`, `Memory`, `PhysicalMemory`,
`String`, `Calc` and `Loop`.

Common options: `UpdateDivider`, `Disabled`, `Paused`, `MinValue`, `MaxValue`,
`InvertMeasure`, `Substitute` and `RegExpSubstitute` (`"pattern":"replacement"` pairs),
`OnUpdateAction` and `OnChangeAction`. `Time` accepts `Format` (`chrono` syntax, default
`%H:%M:%S`), `CPU` accepts `Processor` (core index starting at 0), `Loop` accepts
`Start`/`Step`/`End`/`Reset`, and `Calc` accepts `Formula` (measure references, `counter`).

Values are exposed through the `read_measures` IPC and fed back into rendering on each `Update`
interval. `OnUpdateAction` and `OnChangeAction` actions are executed by the bang engine (see
below).

## Bangs and interactions

The bang parser (`crates/petrichor-core/src/bang.rs`) accepts `[…]` strings, quoted arguments
and the `!Rainmeter` prefix. MVP bangs are executed by `src-tauri/src/bangs.rs`:

| Bang                                                                            | Effect                                        |
| ------------------------------------------------------------------------------- | --------------------------------------------- |
| `!Refresh`, `!Redraw`, `!Update`                                                | Reload, redraw or immediate update            |
| `!Hide`, `!Show`, `!Toggle`                                                     | Window visibility                             |
| `!Move`, `!ZPos`, `!ClickThrough`, `!Draggable`                                 | Window geometry and options                   |
| `!SetVariable`, `!SetOption`                                                    | Variable or section option changed at runtime |
| `!Log`, `!About`, `!Manage`                                                     | Log, About window or management               |
| `!Quit`, `!RefreshApp`, `!ActivateConfig`, `!DeactivateConfig`, `!ToggleConfig` | Lifecycle                                     |

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
- `Counter/Counter.ini` — animated counter (`Calc`, `Loop`, `Update=250`); a click logs
  "Counter clicked" and hovering logs the cycle.

All three enable `!Draggable "1"` via `OnRefreshAction`, so they can be dragged directly with a
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
`canvas` (fond, meters `String`, `Image`, `Bar`). Les meters non encore supportés sont
journalisés comme tels et ignorés au rendu.

```bash
./scripts/snapshot.sh           # instantanés de rendu (listes de commandes de dessin)
```

## Mesures

Le moteur (`crates/petrichor-core/src/measure.rs`) maintient un état par skin et met à jour les
valeurs à chaque cycle. Mesures prises en charge : `Time`, `Uptime`, `CPU`, `Memory`,
`PhysicalMemory`, `String`, `Calc` et `Loop`.

Options communes : `UpdateDivider`, `Disabled`, `Paused`, `MinValue`, `MaxValue`,
`InvertMeasure`, `Substitute` et `RegExpSubstitute` (paires `"motif":"remplacement"`),
`OnUpdateAction` et `OnChangeAction`. `Time` accepte `Format` (syntaxe `chrono`, par défaut
`%H:%M:%S`), `CPU` accepte `Processor` (indice de cœur à partir de 0), `Loop` accepte
`Start`/`Step`/`End`/`Reset`, et `Calc` accepte `Formula` (références aux mesures, `counter`).

Les valeurs sont exposées via l'IPC `read_measures` et réinjectées dans le rendu à chaque
intervalle `Update`. Les actions `OnUpdateAction` et `OnChangeAction` sont exécutées par le
moteur de bangs (voir ci-dessous).

## Bangs et interactions

Le parseur de bangs (`crates/petrichor-core/src/bang.rs`) accepte les chaînes `[…]`, les
arguments entre guillemets et le préfixe `!Rainmeter`. Les bangs MVP sont exécutés par
`src-tauri/src/bangs.rs` :

| Bang                                                                            | Effet                                           |
| ------------------------------------------------------------------------------- | ----------------------------------------------- |
| `!Refresh`, `!Redraw`, `!Update`                                                | Rechargement, redessin ou mise à jour immédiate |
| `!Hide`, `!Show`, `!Toggle`                                                     | Visibilité de la fenêtre                        |
| `!Move`, `!ZPos`, `!ClickThrough`, `!Draggable`                                 | Géométrie et options de fenêtre                 |
| `!SetVariable`, `!SetOption`                                                    | Variable ou option de section modifiée à chaud  |
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
- `Counter/Counter.ini` — compteur animé (`Calc`, `Loop`, `Update=250`) ; un clic journalise
  « Compteur cliqué » et le survol journalise le cycle.

Les trois activent `!Draggable "1"` via `OnRefreshAction` : ils peuvent donc être déplacés
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
