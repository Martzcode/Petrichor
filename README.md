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
dernier clic la réaffiche. Un même skin n'a donc jamais qu'une seule fenêtre. Chaque fenêtre de
skin est un Tauri sans décor, redimensionnée automatiquement (`DynamicWindowSize`).

En build de développement, la variable `PETRICHOR_OPEN_SKIN` ouvre un skin au démarrage et
permet de vérifier le rendu sans interaction :

```bash
PETRICHOR_OPEN_SKIN="$PWD/skins-demo/Clock/Clock.ini" npm run desktop:dev
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
