#!/bin/zsh
# Fait entrer la dernière version d'Escouade (upstream/main) dans la branche osho-squad.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ -n "$(git status --porcelain)" ]]; then
  echo "Modifs non commitées : commite ou stash avant de synchroniser." >&2
  exit 1
fi

git fetch upstream --tags
git checkout osho-squad
before=$(git rev-parse HEAD)

if ! git rebase upstream/main; then
  echo "\nConflit pendant le rebase. Résous-le (ou demande à Claude), puis :" >&2
  echo "  git add <fichiers> && git rebase --continue   # ou git rebase --abort" >&2
  exit 1
fi

# Le rebase réécrit l'historique : force-with-lease refuse d'écraser un push que tu n'as pas récupéré.
git push --force-with-lease origin osho-squad
git push origin upstream/main:main

echo "\nÀ jour sur $(git describe --tags --abbrev=0 upstream/main). Nouveautés reçues :"
git log --oneline "$before..upstream/main" 2>/dev/null | head -20 || true
echo "\nÉtape suivante : scripts/osho-install.sh"
