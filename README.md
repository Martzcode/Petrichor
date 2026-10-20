# Petrichor

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
