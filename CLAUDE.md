# OshO Squad

Fork perso d'Escouade (`upstream` = guillaume-gagnaire/escouade), installé à côté de l'app officielle.

## Branches et remotes
- `osho-squad` : la branche de travail, toujours rebasée sur `upstream/main`.
- `main` : copie de `upstream/main`, jamais modifiée à la main.
- Une modif à proposer au dev part de `upstream/main` sur sa propre branche, pas de `osho-squad`.

## Ce qui rend l'app séparée de l'officielle (ne pas annuler)
- `src-tauri/tauri.osho.conf.json` : nom, identifiant `dev.oshoez.squad`, mises à jour vers le fork. Fusionné au build par `--config`, pour ne jamais toucher `tauri.conf.json` (sa ligne `version` change à chaque release du dev).
- `src-tauri/src/paths.rs` : `DATA_DIR_NAME = ".osho-squad"`, `IDENTIFIER = "dev.oshoez.squad"`.
- `src-tauri/src/mcp/install.rs` : `NAME = "osho-squad"`.

## Commandes
- Récupérer la dernière version d'Escouade : `scripts/osho-sync.sh`
- Compiler et installer dans `/Applications` : `scripts/osho-install.sh` (demande avant de quitter l'app)
- Lancer en dev, sans toucher aux vraies données :
  `ESCOUADE_DATA_DIR="$TMPDIR/osho-sandbox" npm run tauri dev -- --config src-tauri/tauri.osho.conf.json`

## Conflits pendant `osho-sync.sh`
Garder la version du dev partout, puis ré-appliquer les modifs OshO par-dessus. Pour les constantes ci-dessus, garder les valeurs OshO. Puis `git add` et `git rebase --continue`.

## Tests
Les tests Rust qui attendent `.escouade` / `escouade` échouent à cause du renommage : c'est attendu, pas une régression.
