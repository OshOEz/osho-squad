# Mode vocal : étude de faisabilité

Étude du 2026-10-15. Deux ambitions comparées : une simple dictée (parler au lieu de taper dans le composeur d'un agent), ou une vraie conversation avec « Escouade », qui voit tous les projets, les agents et le Kanban, agit dessus et répond à voix haute.

Verdict court : **les deux sont faisables**, et la conversation est moins loin qu'il n'y paraît, parce que le serveur MCP de la 1.7 fait déjà presque tout le travail du « cerveau ». Ce qui coûte, c'est l'audio (capture, transcription, synthèse) sur trois OS, pas l'intelligence.

## Ce qui existe déjà

| Brique | Où | Ce qu'elle apporte au vocal |
|---|---|---|
| Process `claude` en stream-json, longue durée | `src-tauri/src/core.rs` (`CLAUDE_BASE_ARGS`), `claude.rs` | Un « chef d'escouade » est un process `claude` de plus, chaud, sans relance à chaque phrase. `--include-partial-messages` donne le texte au fil de l'eau : la voix peut commencer dès la première phrase. |
| Question ponctuelle à Claude (`ask_claude`) | `core.rs` | Le modèle d'un process minimal : `--system-prompt` à soi, `--tools` réduit, `--strict-mcp-config`, `--setting-sources ""`. Un contexte court, donc rapide et peu coûteux. |
| Serveur MCP d'Escouade | `src-tauri/src/mcp/` | 16 outils ouverts à tout client (deux de plus réservés aux agents) : lister projets, agents, tickets, demandes en attente ; résumer un agent ; créer, modifier, ordonner, lancer un ticket ; créer un agent, lui écrire, l'interrompre ; répondre à une autorisation, à une question, approuver un plan ; lire les quotas. Avec ses garde-fous (pas de « Toujours autoriser », pas de commande, pas de suppression) et son journal. |
| `Caller` (External / Agent) | `mcp/mod.rs` | Un troisième appelant, « Escouade », se signe dans les conversations (« Message from Escouade », « Allowed by Escouade through Escouade »). |
| Notifications typées | `notify.rs` | La matière des annonces parlées : « tests-e2e demande d'autoriser npm test ». |
| Vue Claude Code (TUI dans un pty) | `tui.rs` | `/voice` de Claude Code y marche déjà (voir plus bas), avec une réserve sur macOS. |

Ce qui manque à Escouade aujourd'hui : rien ne capte le micro, rien ne parle. Pas de `NSMicrophoneUsageDescription` dans `src-tauri/Info.plist`, pas d'entitlement `com.apple.security.device.audio-input` (indispensable : l'app est signée et notarisée, donc en hardened runtime), et la CSP (`connect-src 'self' ipc:`) interdit tout service de transcription appelé depuis la page, ce qui est très bien : l'audio passera par Rust.

## Niveau 0 : ce qui marche sans une ligne de code

- **La dictée du système** dans le composeur : `Win+H` sous Windows (WebView2), la dictée de macOS (`fn` `fn`) dans WKWebView. Qualité correcte en français, rien à maintenir. Linux n'a pas d'équivalent.
- **`/voice` de Claude Code dans la vue Claude Code** : la dictée officielle, transcrite par Anthropic, gratuite (ne compte pas dans les quotas), français compris, avec un compte claude.ai (pas avec une clé API). Sous macOS, le process hérite de l'identité d'Escouade pour les autorisations du micro : sans `NSMicrophoneUsageDescription` et l'entitlement audio, macOS refuse sans même demander. Ajouter ces deux lignes rend `/voice` utilisable dans la vue Claude Code ; c'est le premier pas de toute façon.

Ce que ce niveau ne donne pas : la dictée dans la vue native d'Escouade (le service de transcription de Claude Code n'a pas d'API publique ; s'y brancher avec le jeton OAuth du compte serait fragile et hors des conditions d'usage, à écarter), et aucune réponse parlée.

## Niveau 1 : la dictée dans Escouade

Un bouton micro dans le composeur et un raccourci maintenu (push-to-talk), qui insère le texte transcrit comme `/voice` le fait dans le TUI.

### Capter le son

| Option | Pour | Contre |
|---|---|---|
| `getUserMedia` dans la webview | Annulation d'écho et réduction de bruit du navigateur, gratuites (utile au niveau 3). | Trois webviews, trois comportements : WebView2 bien ; WKWebView demande les entrées Info.plist et l'entitlement ; WebKitGTK ne l'active que si l'app règle `enable-media-stream` sur la webview. Ne marche que fenêtre au premier plan. |
| **`cpal` côté Rust** (recommandé) | Une seule API pour WASAPI, CoreAudio et ALSA/PulseAudio ; marche fenêtre en arrière-plan avec un raccourci global (`tauri-plugin-global-shortcut`) ; l'audio ne passe jamais par la page. | Pas d'annulation d'écho : à compenser par le push-to-talk au niveau 3. |

`webkitSpeechRecognition` (Web Speech API) est à oublier : exposée mais pas fiable dans WebView2 (elle dépend d'un service Google réservé à Chrome), absente des autres webviews embarquées.

### Transcrire

| Option | Qualité en français | Latence | Coût / vie privée | Remarques |
|---|---|---|---|---|
| **Whisper local** (`whisper-rs`, whisper.cpp) — recommandé par défaut | Bonne avec `large-v3-turbo` (≈ 550 Mo quantifié), correcte avec `small` (≈ 190 Mo) | 0,5 à 2 s après la fin de la phrase (Metal sur Mac, CPU ailleurs) | Gratuit, tout reste sur la machine : fidèle à « local et transparent » | Modèle téléchargé à la demande dans `~/.escouade/models/`, pas dans l'installeur. whisper.cpp se compile en C++ dans les trois jobs de CI (cmake requis) ; prévoir quelques minutes de build en plus. Une invite (`initial_prompt`) avec le nom du projet, de la branche et des agents améliore les termes techniques, comme le fait Claude Code. |
| Moteurs du système (SFSpeechRecognizer / SpeechAnalyzer, Windows.Media.SpeechRecognition) | Bonne sur macOS récent, moyenne sur Windows | Streaming, rapide | Gratuit ; Windows envoie parfois au cloud de Microsoft | Rien sous Linux, deux bindings natifs à maintenir. |
| Service en ligne avec la clé de l'utilisateur (OpenAI, Mistral Voxtral, Deepgram, ElevenLabs Scribe) | Très bonne, streaming | < 500 ms | Payant, l'audio sort de la machine | Une clé de plus dans le trousseau (comme les intégrations) et une ligne au `docs/registre-rgpd.md`. Bonne option facultative, pas par défaut. |

### Effort

Une à deux semaines : capture `cpal` + détection de fin de parole simple (énergie, ou Silero VAD en ONNX), `whisper-rs`, gestionnaire de modèles (téléchargement, progression, choix de la taille dans Réglages), bouton et raccourci, entrées macOS, réglage du WebKitGTK si on passe par la page, tests. Valeur immédiate, et toutes les briques resservent aux niveaux suivants.

## Niveau 2 : « Escouade », le chef d'escouade, d'abord au clavier

Avant la voix, le cerveau. Un panneau de conversation avec Escouade elle-même, qui n'est pas un agent d'un projet mais un process `claude` à part :

- **lancé comme `ask_claude`** mais en stream-json persistant : `--system-prompt` dédié, `--tools ""` (aucun outil de fichier ni de commande), `--strict-mcp-config` avec **le seul serveur MCP d'Escouade**, un modèle rapide (Haiku ou Sonnet, au choix), le compte choisi comme pour un nouvel agent ;
- **un `Caller::Escouade`** pour signer ses actions et appliquer les mêmes garde-fous que pour un client externe ;
- **quelques outils de plus**, réservés à lui :
  - montrer : `focus_agent`, `open_ticket`, `open_view` (Kanban, Vue d'ensemble, Stats) — « montre-moi l'agent du ticket ESC-42 » ouvre l'écran pendant qu'il répond ;
  - ce que l'interface fait et que le MCP ne fait pas encore : valider un ticket « À tester » (merge, pull request, push, avec confirmation), archiver un agent, lire le coût du jour et les stats, mettre le pilote auto en pause ou le relancer ;
- **un prompt système court** : l'état du moment en tête (projets, agents qui attendent), réponses brèves, et confirmation explicite avant toute action qui dépense du quota ou touche à git.

Ce panneau a de la valeur sans micro (« qu'est-ce qui attend ? », « lance les trois tickets prioritaires de Fides »), il se teste au clavier, et il fixe ce que la voix appellera. Effort : environ une semaine, l'essentiel étant déjà dans `mcp/`.

Coût : chaque échange passe par le quota du compte. Avec `--tools ""`, sans réglages ni CLAUDE.md, le contexte reste petit et mis en cache par Claude Code ; sur Haiku, quelques dizaines d'échanges par jour sont négligeables devant un agent au travail. À afficher dans les stats comme un « agent » à part.

## Niveau 3 : la conversation vocale

Niveau 1 en entrée + niveau 2 au milieu + une voix en sortie.

### Parler

| Option | Qualité | Remarques |
|---|---|---|
| Voix du système (crate `tts` : SAPI/WinRT, AVSpeechSynthesizer, speech-dispatcher) | Correcte sur macOS, datée sur Windows | Zéro téléchargement ; bon défaut. |
| **Piper** (ONNX, local) — recommandé | Bonne, plusieurs voix françaises (≈ 60 Mo) | Rapide sur CPU, même logique de modèle téléchargé que Whisper. |
| Service en ligne (ElevenLabs, OpenAI) | Excellente | Clé de l'utilisateur, audio en ligne, comme pour la transcription. |

Le texte de Claude arrive en morceaux (`--include-partial-messages`) : on le coupe en phrases et on en lit une pendant que la suivante se génère. Le markdown est retiré avant la voix ; le panneau garde le texte complet.

### Le tour de parole

- **Push-to-talk d'abord** (raccourci global maintenu, ou clic maintenu sur le bouton) : il évite d'avoir à détecter la fin de parole et à annuler l'écho de sa propre voix, les deux sources principales d'erreurs. Parler pendant qu'Escouade parle la coupe (barge-in simple : on arrête la lecture dès que la touche est pressée).
- **Mains libres ensuite**, en option : détection de voix (Silero VAD) et annulation d'écho (WebRTC AEC, ou capture via la page qui l'a gratuitement). Un mot de réveil (« Escouade ») demande un modèle dédié (openWakeWord) : à garder pour la fin.

### Escouade qui prend la parole

La fonction la plus utile : quand une notification part, Escouade peut la dire (« tests-e2e demande d'autoriser `npm test`. J'autorise ? ») et attendre la réponse, qui part par `answer_permission`. Réglage à part (« Annoncer à voix haute »), avec les mêmes types que « Me prévenir pour ». Un `arg` de commande se lit, un diff ou un plan long se résume et renvoie à l'écran.

### Latence réaliste

| Étape | Durée |
|---|---|
| Fin de parole (relâcher la touche) | ≈ 0 |
| Transcription Whisper locale | 0,5 à 2 s |
| Premier mot de Claude (process chaud, Haiku/Sonnet) | 1 à 2 s |
| Chaque appel d'outil MCP intermédiaire | + 1 à 2 s |
| Synthèse de la première phrase (Piper) | 0,1 à 0,3 s |

Soit **2 à 4 s** pour une réponse simple, **4 à 8 s** quand il lit puis agit. Ce n'est pas le temps réel d'un modèle audio natif (< 1 s), qu'Anthropic ne propose pas en API : on est sur un talkie-walkie efficace, pas sur une conversation au téléphone. Pour que l'attente ne paraisse pas vide, une courte phrase dite dès le premier appel d'outil (« je regarde le Kanban de Fides… »), et le panneau qui montre l'écran concerné.

### Garde-fous propres à la voix

- Une transcription peut se tromper : toute action qui écrit (lancer, valider, autoriser, écrire à un agent) est **répétée et confirmée** avant de partir (« Je lance ESC-42 sur Opus, d'accord ? »). Les lectures partent sans confirmation.
- Les limites du MCP restent celles du vocal : pas de « Toujours autoriser », pas de commande, pas de suppression.
- Indicateur micro bien visible dans la barre de titre ; rien n'écoute hors push-to-talk tant que le mains libres n'est pas activé.

Effort : deux à trois semaines au-dessus des niveaux 1 et 2 (synthèse, découpe en phrases, push-to-talk global, annonces, barge-in, réglages, tests sur les trois OS).

## Risques

- **Build et taille** : whisper.cpp et onnxruntime (Piper, VAD) compilés ou embarqués pour Windows, macOS universel (deux architectures) et Linux. Les modèles se téléchargent à part ; les bibliothèques ajoutent quelques dizaines de Mo au binaire.
- **Linux** : PulseAudio/PipeWire via `cpal` marche, mais l'AppImage doit trouver `libasound` ; speech-dispatcher n'est pas toujours installé (Piper l'évite).
- **WSL** : sans objet, l'audio est capté par Escouade côté Windows.
- **Langue** : Whisper et Piper suivent la langue de l'interface (ou celle « pour Claude »), mais un mélange français et termes anglais reste le cas difficile ; l'invite de Whisper avec les noms du projet aide.
- **Quota** : le chef d'escouade consomme le quota comme un agent ; il doit respecter la pause du pilote auto, ou tourner sur un compte choisi.

## Recommandation

1. **Tout de suite** (une demi-journée) : `NSMicrophoneUsageDescription` et l'entitlement audio sur macOS, pour que `/voice` marche dans la vue Claude Code, et une mention de `Win+H` / dictée macOS dans la doc.
2. **Le chef d'escouade au clavier** (niveau 2) : la plus grosse valeur par jour de travail, sans risque technique, et la base de tout le reste.
3. **La dictée locale** (niveau 1) : `cpal` + Whisper, branchée sur le composeur des agents et sur le panneau d'Escouade.
4. **La voix** (niveau 3) en push-to-talk avec Piper et les annonces parlées ; le mains libres et le mot de réveil seulement si l'usage le demande.

Total pour une conversation vocale complète et soignée : de l'ordre de **5 à 6 semaines**, dont la moitié est utile avant même que la voix existe.
