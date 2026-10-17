#!/usr/bin/env bash
set -euo pipefail

# Compare les captures de rendu (listes de commandes de dessin) aux références.
# Sans session graphique, le rendu est validé via des instantanés textuels
# produits par Vitest (src/renderer/*.spec.ts).

cd "$(dirname "$0")/.."

npx vitest run "$@"
