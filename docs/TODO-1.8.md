# Escouade 1.8 — à faire

Deux chantiers :
- Codex, GitHub Copilot CLI, Cursor CLI et OpenCode comme moteurs d'agents, à côté de Claude Code (chapitre 1 ; Cursor ajouté le 2026-10-14) ;
- GitHub, GitLab, Bitbucket et Gitea comme hébergeurs de code, pour suivre les PR, leur CI et les déploiements (chapitre 2).

Tickets du Kanban d'Escouade, créés le 2026-10-14 par son serveur MCP, dans l'ordre de ce fichier :
- ESC-11 à ESC-21, puis ESC-86, ESC-87 et ESC-89 : « Aussi pour la 1.8 » ;
- ESC-22 à ESC-25 : essais du point 0 ;
- ESC-26 à ESC-36, puis ESC-88 : refonte multi-moteurs ;
- ESC-37 à ESC-52 : les moteurs, leurs vues terminal, docs et essai réel ;
- ESC-53 à ESC-70 : hébergeurs de code.

- L'avancée d'un agent qui exécute un plan (tâches, sous-agents, ce qui est en cours, essai avec superpowers), d'abord notée ici, est passée dans la 1.7 : voir `docs/TODO-1.7.md`, chantier 5.
- Une seule forme d'infobulle dans toute l'app, d'abord notée ici, est passée dans la 1.7.1 : voir `CHANGELOG.md`.

Fait pour la 1.8 :
- [x] **Vue Claude Code** (2026-10-12) : un agent peut tourner dans l'interface de Claude Code elle-même, dans un terminal, à la place de la conversation d'Escouade (bouton de l'en-tête, réglage pour les nouveaux agents). Escouade le suit par ses hooks HTTP et son transcript. Voir `docs/PROTOCOL.md`, « Vue Claude Code ».
  - [ ] À essayer sous Windows (ConPTY, `claude.exe` natif) et macOS avec un vrai compte : collage d'un message, Échap, `/clear`, une compaction, un sous-agent.
  - [ ] La même vue pour les autres moteurs (demandé le 2026-10-14) : voir le chapitre 1, « Vue terminal de chaque moteur ».
  - [ ] **Changer de vue aussi au clic droit sur un agent** (demandé le 2026-10-12) : dans le menu de sa carte de la barre latérale (`Sidebar.svelte`, à côté de « Dupliquer la conversation », « Ouvrir un terminal »…), « Vue Claude Code » ou « Vue Escouade » selon sa vue du moment. Mêmes règles que le bouton de l'en-tête (`Conversation.svelte`) : jamais pour un agent de ticket ni un agent archivé, même appel `api.setAgentView`. Textes fr et en.

Aussi pour la 1.8 :
- [ ] **Les sous-agents de chaque agent dans la barre latérale, et leur conversation** (demandé le 2026-10-13, puis complété le même jour : les dérouler, entrer dans la conversation d'un sous-agent et agir sur lui).
  - **Dans la barre latérale** : sous la carte d'un agent qui a des sous-agents, un chevron les déroule. D'abord ceux en cours, chacun avec son titre, son modèle et ce qu'il fait (« Édite src/a.ts »), puis le nombre de ceux qui sont finis, qui se déroulent à leur tour. Replié quand aucun ne tourne. Les données existent déjà : `PlanState.agents` (`src/lib/types.ts`, `SubAgent` : `title`, `kind`, `model`, `status`, `doing`, `tools`, `tokens`), tenu par le backend depuis le flux de l'agent. Aujourd'hui, seul le bandeau « Plan » de la conversation les montre, et la carte dit seulement « Plan 3/7 · 2 sous-agents » (`plan.miniSubs`, `Sidebar.svelte`). Penser à la hauteur de la barre (beaucoup de sous-agents en parallèle) et à la vue d'ensemble, qui pourrait les montrer de la même façon.
  - **Entrer dans sa conversation** : un clic sur un sous-agent (dans la barre, dans le bandeau « Plan » ou sur la ligne de son appel dans la conversation) ouvre sa conversation en grand, à la place de celle de l'agent, avec un fil d'Ariane « agent › sous-agent » pour revenir.
    - Ce sont les éléments dont le `parent` est l'appel qui l'a lancé (`SubAgent.id`). Aujourd'hui, ils sont repliés sous la ligne de cet appel dans la conversation de l'agent (`Conversation.svelte`, `children` ; `ToolRow.svelte`, `childrenOf`), qui n'en montre que les outils et le dernier texte.
    - En tête : son type, son modèle, sa consigne, ses tokens, sa durée. La consigne est le `prompt` de l'appel, que le backend ne garde pas aujourd'hui (`docs/PROTOCOL.md`, « Les sous-agents ») : à garder, borné.
    - À la fin : son rapport (`handback_report` de la `task_notification`).
    - Les sous-agents qu'il lance lui-même s'y déroulent de la même façon.
  - **Agir sur lui** :
    - l'arrêter : un bouton « Arrêter », par la requête de contrôle `stop_task` (listée dans `docs/PROTOCOL.md`, jamais utilisée par Escouade : à essayer) ;
    - ses demandes de permission et ses questions apparaissent aussi dans sa vue, et on y répond ;
    - lui écrire : à étudier. On ne connaît pas de moyen d'écrire à un sous-agent par le flux de Claude Code. Ce qu'on sait : l'agent peut relancer un sous-agent fini (`canContinueAgent` dans le `tool_use_result` de l'appel, outil `SendMessage` vers son `agentId`). À chercher dans le binaire, puis à essayer : une requête de contrôle qui écrit à une tâche. Sinon, un message à l'agent qui lui demande de transmettre, présenté comme tel dans l'interface.
  - **Après coup** : la conversation d'un sous-agent fini reste lisible, même après un redémarrage (le journal d'Escouade, `conv.rs`, garde les éléments avec leur `parent`). La barre n'en garde que les derniers (`PlanState.agents`, 30 au plus) ; les plus anciens restent dans la conversation de l'agent.
  - **Vue Claude Code** : les hooks donnent les sous-agents ; leur conversation est dans leur propre transcript (`transcript.rs` l'écarte aujourd'hui, `isSidechain`), à lire pour une vue en lecture seule. Pour agir, on passe par le terminal de Claude Code.
  - Textes fr et en.
- [ ] **Les tâches de fond de chaque agent, en détail** (demandé le 2026-10-13) : voir ce qui tourne en arrière-plan et ce qui s'y passe.
  - **Aujourd'hui** : Claude Code annonce chaque tâche (`task_started`, `task_updated`, `task_progress`, `task_notification`), et la liste entière à chaque changement (`background_tasks_changed { tasks }`) ; voir `docs/PROTOCOL.md`, tableau des tâches. Mais :
    - le réducteur du plan (`plan.rs`) ne garde que les sous-agents (`local_agent`) et les workflows (`local_workflow`) ;
    - il ignore une commande lancée en arrière-plan (`local_bash`), une surveillance (`monitor_mcp`), un agent distant (`remote_agent`) et un coéquipier (`in_process_teammate`) ;
    - `background_tasks_changed` est ignoré ;
    - la fin d'une tâche n'est qu'une ligne discrète dans la conversation (`EventMessage.svelte`, « Tâche de fond terminée »).
  - **À faire** :
    - sur la carte de l'agent, le nombre de tâches en fond (« 2 en fond »), et leur liste dans la barre latérale, sous ses sous-agents, repliable de la même façon ;
    - une vue détaillée (panneau, ou section du bandeau « Plan ») : pour chaque tâche, son type (commande, surveillance, sous-agent, workflow), sa commande ou sa description, depuis quand elle tourne, son état et, à la fin, son résumé ;
    - sa sortie en direct : Claude Code l'écrit dans un fichier (`output_file` de la `task_notification`, `outputFile` du `tool_use_result`), à suivre en lecture seule comme un terminal ;
    - « Arrêter » par `stop_task`, comme pour un sous-agent ;
    - une commande au premier plan reste une ligne d'outil, jusqu'à ce qu'elle passe en fond (`task_updated` avec `is_backgrounded`) ;
    - les tâches finies restent un moment dans la liste (les 10 dernières ?), qui se vide à une nouvelle session, comme le plan.
  - **À mesurer d'abord** sur un vrai flux (une commande en fond, une surveillance, un sous-agent) : ce qui dit le mieux ce qui tourne, les débuts et fins (`task_started` / `task_notification`) ou la liste entière (`background_tasks_changed`), et ce que donne `output_file` pendant que la tâche tourne.
  - **Vue Claude Code** : savoir si les hooks ou le transcript disent les tâches de fond (à vérifier).
  - Textes fr et en.
- [ ] **Coller une image dans un ticket du Kanban** (demandé le 2026-10-13) : une capture du bug ou une maquette, collée (ou glissée) dans la fenêtre d'un nouveau ticket, et aussi en le modifiant tant qu'il est « À faire ».
  - dans la fenêtre du ticket (`TicketForm.svelte`) : vignettes sous la description, retirables, avec les formats et limites des pièces jointes du chat (`src/lib/attachments.ts` : PNG, JPEG, GIF, WebP) ;
  - gardées avec le ticket, en fichiers à part (dossier du ticket sous `~/.escouade/`), pas dans le JSON des tickets ; effacées avec le ticket ;
  - l'agent du ticket les reçoit en blocs image avec son premier message (`board::first_message`), comme les pièces jointes du chat ; elles restent dans sa session pour les boucles suivantes ;
  - la carte du Kanban montre qu'il y en a (trombone et nombre) ; l'outil MCP `create_ticket` pourrait en accepter plus tard, et les images jointes des tickets importés (Jira, GitHub, Linear, Trello) suivre ;
  - textes fr et en.
- [ ] **Coller une image dans le prompt, ailleurs** : le compositeur de la vue Escouade prend déjà les fichiers du presse-papiers (`Composer.svelte` `onPaste`) : l'essayer sous Windows et macOS avec une capture (image seule). En vue Claude Code, Ctrl+V (Cmd+V sous macOS) colle du texte par Escouade (`src/lib/terminals.ts`) et Claude Code ne voit jamais la touche qui lui fait lire l'image du presse-papiers : quand le presse-papiers tient une image, laisser passer la touche, ou enregistrer l'image dans un fichier temporaire et taper son chemin.
- [x] **La connexion à un compte Claude referme le navigateur sous macOS et Linux** (corrigé le 2026-10-15, ESC-17) : à sa fin, le programme de connexion tue son groupe de processus (`pty.rs`, `spawn_program`), navigateur ouvert par `xdg-open` ou `open` compris quand il en est un enfant. Sous Windows, il épargne depuis le 2026-10-13 les programmes qui ont une fenêtre (`Job::terminate_windowless`).
  - **Pourquoi pas « ne tuer que ce qui n'a pas de fenêtre »** : quand le programme de connexion se termine, le noyau envoie SIGHUP au groupe de premier plan de son terminal, avant qu'Escouade ne tue quoi que ce soit. Vu sous Linux : un processus resté dans ce groupe meurt à la fin du programme, même si personne ne tue le groupe, et seul celui qui ignore SIGHUP survit. Un navigateur resté dans le groupe part donc de toute façon. Rien ne le sauve après coup, et Unix ne dit pas de façon sûre quel processus a une fenêtre (rien sous Wayland).
  - **Ce qui est fait** : le navigateur part hors du terminal. Claude Code (2.1.296, Linux et macOS) ouvre la page par `$BROWSER`, sinon par `xdg-open` (Linux) ou `open` (macOS), et attend sa fin. La connexion lui donne `BROWSER` = un script d'Escouade (`open-browser` dans le dossier de données, `pty::open_browser_apart`). Ce script lance le `BROWSER` de l'utilisateur (gardé dans `ESCOUADE_BROWSER`), sinon `xdg-open` ou `open`, dans une session à part (`setsid`, ou `POSIX::setsid` de Perl sous macOS, qui n'a pas `setsid`). Claude Code et ce qu'il a lancé sans fenêtre restent dans le groupe, tué comme avant. Un script que le système refuse de lancer (dossier de données monté `noexec`) n'est pas donné à Claude Code : la connexion ouvre alors le navigateur comme avant.
  - **Tests** (`pty::unix_tests`, et `core_tests::an_accounts_sign_in_runs_its_claude_with_its_folder_in_a_terminal` pour le branchement dans `claude_login`) : la fin de la connexion et la fermeture de son terminal arrêtent un programme lancé en fond et laissent le navigateur (un faux, ouvert par `$BROWSER` comme Claude Code le fait) ; la fin de la vue Claude Code arrête tout ; l'ouvreur garde le `BROWSER` de l'utilisateur et le lui rend, et, sans lui, lance celui du système sans `BROWSER` (sinon `xdg-open` relancerait l'ouvreur). Les tests de la connexion ont échoué sans le correctif, ceux des variables avec un ouvreur qui ne les rend ni ne les retire.
  - **Essai réel sous Linux** (2026-10-15, WSL 2 Ubuntu 24.04 avec WSLg, noyau 5.15) : le vrai `pty.rs` et le vrai `job.rs` compilés pour Linux, le vrai Claude Code 2.1.296 (premier lancement, « Claude account with subscription »), le `xdg-open` d'Ubuntu (1.1.3) et Firefox 158. Sans le correctif, Firefox était dans le groupe et la session de Claude Code, et il s'est fermé avec lui, que Claude Code se termine (SIGTERM, comme `/exit`) ou que le terminal soit fermé (`PtyManager::kill`). Avec le correctif, `xdg-open` puis Firefox étaient dans leur propre session, et Firefox est resté ouvert dans les deux cas. Claude Code s'est arrêté à chaque fois, et rien n'est resté dans son groupe. La branche Perl de l'ouvreur a aussi été essayée sous Linux, avec un PATH sans `setsid`.
  - **Sous macOS**, Claude Code 2.1.296 ouvre la page par `open` quand `BROWSER` est vide. `open` passe la main à LaunchServices, et le navigateur est lancé par launchd, hors du groupe de la connexion : ce cas ne fermait sans doute pas le navigateur, même avant (à confirmer sur un Mac). Le navigateur partait avec la connexion quand `BROWSER` désignait un navigateur, par exemple `/Applications/Firefox.app/Contents/MacOS/firefox`. C'est le cas à essayer sur un Mac, avant et après le correctif.
  - **Essai sous macOS** : pas de Mac sur le poste. Choix de l'utilisateur (2026-10-15) : les tests Unix tournent dans la CI macOS de GitHub, avec un vrai noyau, un vrai pty et la branche Perl de l'ouvreur, mais sans vrai navigateur. Run 38690120958 de la branche `ticket/esc-17` (2026-10-15), lancé à la main : macOS 26 arm64, 1419 tests sur 1419, dont les huit de ce correctif ; Ubuntu 22.04, 1421 sur 1421 ; Windows avec l'e2e, vert.
  - **Reste**, quand un Mac sera là : l'essai avec un vrai navigateur, `BROWSER=/Applications/Firefox.app/Contents/MacOS/firefox` avant et après le correctif, puis `BROWSER` vide (`open`).
- [ ] **Ouvrir un terminal dans le worktree d'un agent** (demandé le 2026-10-13). Ça existe déjà, mais seulement à deux endroits : le clic droit sur la carte de l'agent (« Ouvrir un terminal », `Sidebar.svelte`, `terminalIn` avec son `agentId`) et le clic droit sur un dossier de l'éditeur. Le « + » de la section Terminaux (`shellMenu` → `newTerminal(project.id, …)`) ouvre toujours dans le dossier du projet, et l'en-tête de la conversation n'a pas de bouton pour ça. À ajouter :
  - dans le menu du « + », après les shells, « Dans le worktree de… » avec les agents du projet qui en ont un (pas les archivés) ;
  - un bouton dans l'en-tête de la conversation d'un agent qui a un worktree ;
  - textes fr et en.
- [ ] **Les commandes de lancement dans un worktree** (à étudier, demandé le 2026-10-13). Aujourd'hui, les commandes de la section « Lancement » tournent seulement dans le dossier du projet (`run_start`, `commands.rs` : `pty::run_cwd(&project.path, &run.cwd)`). Il faudrait pouvoir les lancer dans le worktree d'un agent, pour voir son travail tourner sans le fusionner. À voir :
  - **où choisir** : un menu sur ▶ (« Dans le projet », « Dans le worktree de <agent> »), ou « Lancer » dans le clic droit de la carte de l'agent, avec la liste des commandes ;
  - **dossier** : le même `cwd` relatif, pris dans le worktree. S'il n'y existe pas, le lancement est refusé et le message dit pourquoi ;
  - **ports** : la même commande lancée dans le projet et dans un worktree se dispute le port (deux `npm run dev` sur 5173). Les lancements de test réservent déjà un bloc de ports par worktree (`ESCOUADE_PORT_BASE`, `testlaunch.rs`), mais une commande de lancement ne reçoit aucune variable d'Escouade (consigne de `worktrees.rs`). Il faut choisir : exporter le bloc de l'agent, prévenir du conflit, ou n'autoriser qu'une instance à la fois ;
  - **état** : la section montre aujourd'hui un état par commande (prêt, en cours, planté). Il en faudrait un par commande et par endroit, et le nom du terminal dirait où elle tourne (« Front · refacto-auth ») ;
  - **dépendances** : un worktree neuf n'a pas de `node_modules`. C'est le rôle des étapes de préparation du worktree (`WorktreeStep`), à rappeler dans le message d'erreur ou la doc ;
  - **cycle de vie** : ces commandes s'arrêtent quand l'agent est archivé ou supprimé, ou quand son worktree est retiré, comme ses lancements de test aujourd'hui ;
  - **lien avec le « Lancement de test » des tickets** (recette proposée par l'agent, `testlaunch.rs`) : ne pas faire deux fois la même chose. Par exemple, la recette pourrait partir des commandes de lancement du projet quand il en a.
- [ ] **Les onglets des projets ne défilent pas** (bug signalé le 2026-10-14). Avec beaucoup de projets ouverts, ceux qui ne tiennent pas dans la barre du haut sont coupés et inaccessibles, et le « + » qui ajoute un projet disparaît avec eux. Cause : `.tabs` est en `overflow: hidden` et chaque onglet en `flex-shrink: 0` (`TitleBar.svelte`). À faire :
  - la barre des onglets défile à l'horizontale : molette (le défilement vertical devient horizontal), pavé tactile, barre de défilement fine au survol ;
  - le « + » reste visible, hors de la zone qui défile ;
  - l'onglet du projet choisi revient dans la zone visible (`scrollIntoView`), quelle que soit la façon de le choisir : clic sur une notification, raccourci clavier, vue d'ensemble ;
  - un fondu au bord dit qu'il y a des onglets cachés, et prend la couleur d'alerte quand un onglet caché clignote (un agent attend) ;
  - le glisser-déposer qui réordonne les onglets fait défiler la barre près de ses bords ;
  - la zone qui déplace la fenêtre (`data-tauri-drag-region`) garde sa place à droite des onglets ;
  - tests : Vitest pour le défilement et l'onglet actif ramené en vue ; e2e avec une dizaine de projets, à la taille de la fenêtre de la CI (1028×779).
- [ ] **Site : présenter Escouade comme un ADE complet** (demandé le 2026-10-14, ESC-86). Le site, le README et PRODUCT.md présentent Escouade comme « le poste de pilotage de tes agents Claude Code » (« Une escouade de Claude. Une seule fenêtre. », « Toutes tes sessions Claude Code dans une seule fenêtre… »). C'est réducteur : c'est un environnement de développement agentique (ADE) complet.
  - Le récit suit le cycle de développement : planifier (Kanban, tickets, imports, pilote auto), coder (agents en parallèle, éditeur, terminaux), relire et tester (Git, « ▶ Tester », lancements), livrer (commit, merge, PR, puis le suivi de la 1.8), piloter (quotas, coût, stats, notifications, MCP).
  - D'abord deux ou trois directions de récit montrées sur aperçu ; l'utilisateur choisit.
  - Puis meta, hero, sections, FAQ et appel final en français et en anglais, la page « Ce qu'est Escouade » de la doc, le README et la section « Positioning » de PRODUCT.md.
  - Rien d'annoncé qui ne soit livré, et un récit qui ne repose pas sur le nom « Claude », pour accueillir les moteurs de la 1.8 sans être réécrit. La description du dépôt GitHub ne change qu'avec l'accord de l'utilisateur.
- [ ] **Éditeur : « Afficher dans l'Explorateur »** au clic droit sur un fichier ou un dossier de l'arborescence (demandé le 2026-10-14, ESC-87). Le menu (`treeMenu` d'`EditorView.svelte`) n'a aucune entrée vers l'explorateur du système.
  - L'entrée est nommée comme le système : « Afficher dans l'Explorateur » (Windows), « Afficher dans le Finder » (macOS), « Ouvrir le dossier contenant » (Linux).
  - Par `revealItemInDir` du plugin opener, déjà présent : il manque la permission `opener:allow-reveal-item-in-dir`.
  - Sur le vide : le dossier racine de la source (projet ou worktree de l'agent).
  - À essayer avec un projet WSL et un worktree supprimé (refusé avec sa raison).
- [ ] **Fermer un projet garde son historique** (demandé le 2026-10-14, ESC-89) : le rouvrir dans le même dossier le retrouve.
  - Aujourd'hui, « Fermer le projet » (`Core::remove_project`) efface le journal de conversation de chaque agent et son journal de préparation, les tickets du projet (`drop_project_tickets`) et ses réservations de comptes. Les worktrees, les sessions de Claude Code et les stats restent. Un projet recréé dans le même dossier reçoit un nouvel id et repart de zéro.
  - Fermer retire le projet de l'app sans effacer son historique : réglages, agents et conversations, tickets (images comprises) restent sur le disque, rangés comme « projet fermé » avec le chemin de son dossier. Les process, terminaux et lancements de test s'arrêtent comme aujourd'hui.
  - Ajouter un projet dans le dossier d'un projet fermé (chemin normalisé : casse sous Windows, séparateurs, chemins WSL) lui rend son historique et son id, donc ses stats. La fenêtre d'ajout le dit (« Historique retrouvé : 12 agents, 34 tickets… ») et permet de partir d'un projet vierge.
  - Les agents reviennent sans process ; un worktree disparu suit la règle d'aujourd'hui. Les réservations de comptes reviennent pour les comptes qui existent encore.
  - Un ticket « En cours » à la fermeture revient bloqué (« Projet fermé pendant le ticket »), avec « Reprendre » (décidé le 2026-10-14).
  - Effacer vraiment : une case « Effacer aussi son historique » dans la fenêtre de fermeture, décochée par défaut, et la liste des projets fermés dans Réglages › Projets (dossier, taille, dernière activité) avec « Effacer l'historique ».
- [x] **Blocage intermittent de la CI sous macOS** (traité le 2026-10-13) : un test qui crée un agent dans un worktree git ne finit parfois jamais, et le job est coupé au bout de 45 minutes. Ce n'est jamais le même test (`core_tests::the_project_diff_covers_the_checkout_and_each_agents_worktree`, `core_tests::an_agent_made_for_a_ticket_has_its_branch_from_a_base_and_the_projects_env_files`), seulement sous macOS, depuis le 10 octobre au moins (runs de 3ea4859, 24dfda6, 9d07e83, 170e1c3, 140db3e, ba58456). Ce qui a été fait : git ne dépend plus de la fin de ses tubes et s'arrête au bout de 2 minutes (30 minutes pour un commit, une fusion ou un `worktree add` qui lancent des hooks), `claude` et les lancements de test n'héritent plus des descripteurs ouverts de l'app sous macOS (ils tenaient les tubes de git ouverts), et nextest tue un test bloqué au bout de son délai et le nomme. À confirmer sur plusieurs runs macOS (`node scripts/ci-flakes.mjs`) avant de le tenir pour réglé.
- [x] **Intégration Linear** (2026-10-12), à côté de Jira, Trello et GitHub Issues (`src-tauri/src/integrations/`, sur le modèle de `jira.rs` et `github.rs`) :
  - connexion par clé d'API personnelle (en-tête `Authorization`), gardée dans le trousseau comme les autres (`secrets.rs`) ;
  - lien d'un projet vers une équipe Linear, et éventuellement un projet ou un cycle ;
  - import des issues dans le Kanban, avec leurs critères d'acceptation extraits de la description (`text.rs`), import par étiquette compris (« claude-ready ») ;
  - suivi du statut (les états du workflow de l'équipe : « In Progress », « In Review », « Done ») et commentaires au passage « À tester » puis « Terminé » (`sync.rs`) ;
  - API GraphQL (`https://api.linear.app/graphql`) : un faux serveur pour les tests, comme `fake.rs`, et le réglage de proxy appliqué ;
  - interface : Linear dans Réglages › Intégrations et dans « Importer des tickets », textes fr et en ; site, README et journal.

## Ampleur (estimation du 2026-10-11)

Repère : la 1.7 (cinq chantiers) a fait 266 commits et environ 78 000 lignes ajoutées, tests compris.

| Lot | Part de la 1.7 | Ce qui pèse |
|---|---|---|
| Essais réels (point 0) | ~8 % | Il faut l'utilisateur pour les connexions : compte ChatGPT, clé d'un fournisseur pour OpenCode, compte GitHub avec Copilot, compte Cursor. |
| Refonte multi-moteurs, avec Claude seul | ~30 % | Extraction de l'interface « moteur » (refacto de `claude.rs`, `agent.rs` et `core.rs`), données, registre et capacités, interface, comptes et quotas, textes, one-shots, MCP, vue terminal générique, banc de conformité, moteur ACP générique. Elle prend une part de chaque moteur et du reste commun. |
| Codex | ~25 % | Client JSON-RPC, traduction du flux, permissions, modes, comptes `CODEX_HOME`, quota, faux `codex`. Le premier moteur essuie les plâtres de l'interface. |
| Copilot CLI | ~18 % | SDK Rust ou client JSON-RPC, traduction de ses 150 types d'événements, permissions typées, comptes `COPILOT_HOME`, crédits d'IA, faux `copilot`. |
| Cursor CLI | ~10 % | Par le moteur ACP générique de la refonte : extensions `cursor/*` (questions, plan, tâches, sous-agents), comptes par clé d'API, faux `agent`. Ni quota ni coût connus. |
| Vues terminal | ~6 % | La vue générique est dans la refonte : Codex et Copilot sur le modèle de la vue Claude Code (hooks, journal, reprise), OpenCode par `opencode attach`, Cursor en suivi réduit. |
| OpenCode | ~22 % | Serveur HTTP + SSE par agent, traduction, règles de permission, faux `opencode`. Deux lignes d'API (v1, v2) à départager. Moins si OpenCode passe par le moteur ACP générique (voir « Décisions »). |
| Reste commun | ~5 % | Déclaration du serveur MCP dans les configs des moteurs, docs, site. Le reste est dans la refonte. |

Total : environ 1,3 fois la 1.7 avec Cursor et les vues terminal, et plus d'incertitude qu'elle, car les quatre protocoles externes bougent vite :
- le protocole de Codex est marqué « expérimental » ;
- une partie des appels de Copilot l'est aussi, et son CLI sort une version tous les deux ou trois jours ;
- le CLI de Cursor est en bêta, se met à jour seul, et sa version Windows native est récente ;
- OpenCode est à cheval entre une v1 et une v2.

En quota, avec le mode économe (Sonnet pour implémenter et relire), compter une semaine d'abonnement Max environ, Cursor et vues terminal compris. C'est une extrapolation de la 1.7, à revoir après les essais.

**Ordre conseillé et découpe possible** : Codex, puis Copilot, puis Cursor, puis OpenCode. Si la 1.8 doit être plus courte, Codex et Copilot d'abord, Cursor, OpenCode et les vues terminal en 1.9.
- Codex et Copilot ressemblent davantage au modèle d'Escouade : un abonnement avec un quota, un bac à sable du système, un exécutable natif sous Windows.
- Copilot lit déjà `CLAUDE.md` et `.claude/skills` : superpowers a des chances d'y marcher tel quel.
- L'API d'OpenCode se stabilise en v2, et sa doc conseille WSL sous Windows.
- Cursor ne se pilote que par ACP : une fois son client ACP fait, il peut servir à OpenCode (`opencode acp`), d'où Cursor avant OpenCode.

**Hébergeurs de code** (chapitre 2, estimation du 2026-10-14) :

| Lot | Part de la 1.7 | Ce qui pèse |
|---|---|---|
| Essais sur de vrais dépôts | ~3 % | Quatre dépôts de test, chacun avec une CI et un déploiement ; un compte Bitbucket et un Gitea à fournir. |
| Connexions et clients d'API | ~12 % | Quatre API (REST, plus GraphQL chez GitHub), plusieurs comptes par hébergeur, analyse des remotes, faux serveurs. |
| PR des tickets | ~12 % | Colonne « En revue », cycle jusqu'au merge, corrections par l'agent, merge et auto-merge, pilote auto. |
| PR hors tickets, relecture par un agent | ~8 % | Vue « Pull requests », agent sur une PR, relecture à valider avant publication. |
| CI et déploiements | ~8 % | Pastilles, journaux des jobs, vue « Environnements », aperçus. |
| Reste commun | ~5 % | Notifications, vue d'ensemble, serveur MCP, docs, site. |

Total : environ la moitié de la 1.7, avec moins d'incertitude que les moteurs, car les quatre API sont stables et documentées. Avec les moteurs, Cursor et les vues terminal compris, la 1.8 ferait environ 1,8 fois la 1.7.

**Périmètre** (décidé le 2026-10-14) : tout le chapitre 2 en 1.8, pour l'instant. Si la 1.8 s'allonge, le repli est de garder connexions, PR des tickets et CI en 1.8, et de reporter la vue « Pull requests », la relecture par un agent et les déploiements en 1.9.

## 0. À vérifier avant de concevoir (essais jetables, avec les vrais CLI)

Rien de ce qui suit n'a été lancé. Les faits viennent du code source et des docs des projets, lus le 2026-10-11 (le 2026-10-14 pour Cursor) :
- Codex 0.162.1 ;
- OpenCode 1.19.0 et 2.0.26 ;
- Copilot CLI 1.0.95, avec le Copilot SDK 1.0.19 ;
- Cursor CLI (`agent`) : les docs seulement, sans relever de version.

Chaque essai enregistre ses flux réels dans `tests/fixtures/` (comme `real-run-inline.jsonl`), pour les rejouer dans les tests.

### Codex (`codex app-server`, JSON-RPC sur stdio)
- [ ] **Un tour complet enregistré** : texte, raisonnement, commande, modification de fichier, demande d'approbation, interruption, `update_plan`, un sous-agent (`collabToolCall`), une compaction, la fin d'un tour arrêté par la limite.
- [ ] **Méthodes expérimentales** : environ 40 % des méthodes demandent `capabilities.experimentalApi`. Lister celles dont Escouade a besoin (`thread/fork` ? `item/tool/requestUserInput` ? mode Plan ? `approvalsReviewer` ?), et n'en garder que le strict nécessaire.
- [ ] **Bac à sable, worktree et commit** : en `workspace-write`, dans `<projet>/.claude/worktrees/<agent>`, l'agent peut-il commiter ?
  - Un commit écrit dans `<projet>/.git`, hors du worktree, et Codex protégerait `.git` en lecture seule dans ses dossiers inscriptibles (à vérifier).
  - Faut-il `sandbox_workspace_write.writable_roots` ?
  - Mêmes questions avec le bac à sable Windows (`elevated` / `unelevated` : faut-il une installation avec les droits d'admin ?).
- [ ] **MCP par process** : `-c mcp_servers.escouade.url=…` avec `bearer_token_env_var`. Le jeton passe par l'environnement, jamais par la ligne de commande, que la liste des process montre.
  - `-c mcp_servers.escouade.enabled=false` doit écraser une entrée `escouade` de l'utilisateur.
  - Autre piste : la `config` de `thread/start`.
- [ ] **Consignes ajoutées** (protocole du ticket, aujourd'hui `--append-system-prompt`) : `developerInstructions` de `thread/start` ou `-c developer_instructions=…`. Survivent-elles à une reprise ?
- [ ] **Second compte** : un `CODEX_HOME` à part, connexion (`codex login`, ChatGPT ou code d'appareil), entrée du trousseau propre à chaque dossier. Lire le quota par `account/rateLimits/read` sans lancer de tour, et voir ce qu'un app-server lancé pour ça seulement coûte.
- [ ] **Reprise sur un autre compte** : un `rollout-*.jsonl` copié dans un autre `CODEX_HOME` se reprend-il par `thread/resume` ?
- [ ] **Windows** : lancer le binaire natif, pas le `codex.cmd` de npm (comme `resolve_binary` le fait pour `claude`). Le process et ses enfants restent dans le Job de l'agent.
- [ ] **Vue terminal** (voir « Vue terminal de chaque moteur ») :
  - déclarer les hooks d'Escouade pour un seul process : `-c hooks…`, ou un fichier du `CODEX_HOME` ;
  - la vérification de confiance des hooks (`/hooks`) bloque-t-elle ceux qu'Escouade ajoute ?
  - un signal pour une permission en attente ;
  - `codex resume <id>` sur un fil créé par l'app-server, et `thread/resume` sur une session de l'interface ;
  - la touche qui interrompt, et le rollout pendant que la session tourne.

### Copilot CLI (Copilot SDK, JSON-RPC sur stdio)
- [ ] **SDK Rust ou client maison** : la crate `github-copilot-sdk` (1.0.19, « generally available », Rust 1.94 au moins) parle à `copilot --server --stdio` en JSON-RPC 2.0, avec des en-têtes `Content-Length` comme LSP.
  - Ces options du CLI sont internes au SDK : elles ne sont pas dans sa référence publique.
  - Questions pour la crate : laisse-t-elle Escouade lancer le process elle-même (Job de l'agent, environnement du compte, CPU et RAM par agent) ? Et la désactivation de sa fonction `bundled-cli` (qui télécharge le CLI à la compilation), pour pointer vers le `copilot` installé ?
  - Sinon, un client JSON-RPC maison écrit d'après son schéma.
  - Le `rust-version` du `Cargo.toml` passerait de 1.88 à 1.94 (la toolchain épinglée, 1.98, suffit).
- [ ] **Ou ACP** : `copilot --acp --stdio` (en préversion depuis le 2026-01-28) parle le protocole de Cursor. À comparer avec le SDK : ce qu'ACP ne donne pas (crédits et quota, permissions typées, tâches ?). Voir « Décisions », « ACP comme voie commune ».
- [ ] **Pipes sous Windows** : l'issue github/copilot-cli#3188 (sortie en code 1, flux vides quand stdout est un tube, vue en 1.0.44) est-elle réglée en 1.0.95 ? Le SDK passe par des tubes.
- [ ] **Un tour complet enregistré** :
  - texte et raisonnement (`assistant.message_delta`, `assistant.reasoning_delta`) ;
  - outils (`tool.execution_*`, `tool.shell_output`) et diff d'une écriture ;
  - `permission.requested` pour un shell, une écriture, une URL, un outil MCP ;
  - une question (`user_input.requested`, `elicitation.requested`) ;
  - le mode plan et `exit_plan_mode.requested` ;
  - un sous-agent (`subagent.*`), `session.todos_changed` puis la lecture des tâches, une compaction, `abort`, `assistant.usage`.
- [ ] **Noms des outils** de Copilot (shell, écriture, lecture…), relevés dans ces flux, pour leur correspondance avec ceux de Claude Code.
- [ ] **Modes** : ce que font vraiment les modes de permission `manual`, `assisted` (un modèle recommande) et `allow-all`, et les modes d'agent `interactive`, `plan` et `autopilot`. Lequel correspond au mode « Auto » d'Escouade ?
- [ ] **Bac à sable** (`--sandbox`, ouvert à tous depuis la 1.0.93, Windows compris) : même question que pour Codex sur le commit depuis un worktree.
- [ ] **Consignes ajoutées** : un message système à ajouter à la création de session (à trouver dans le SDK). Survit-il à `resume_session` ?
- [ ] **Quota** : depuis le 2026-06-01, Copilot facture en crédits d'IA (1 crédit = 0,01 $, au tarif des API), sauf les abonnements annuels d'avant, qui gardent les requêtes premium.
  - Lire `account.getQuota` (expérimental) et les `quotaSnapshots` d'`assistant.usage` sur un vrai compte : unités, `remainingPercentage`, `resetDate`, drapeau `token_based_billing`.
  - Les limites par session et par semaine (Pro et Pro+, depuis le 2026-04-20) ont-elles une API, ou seulement `/usage` ?
- [ ] **Coût d'un tour** : la conversion des nano-AIU (`copilotUsage.totalNanoAiu`) en dollars, que la doc dit de vérifier avant de l'afficher.
- [ ] **GitHub Enterprise (GHE)**, à gérer : Copilot CLI sur l'instance de l'entreprise, pas seulement github.com.
  - Les deux cas : GHE.com (`<entreprise>.ghe.com`, données hébergées dans une région) et GitHub Enterprise Server (hôte de l'entreprise).
  - Par compte Copilot d'Escouade : l'hôte à donner au CLI (variable d'environnement ou option, à relever dans le CLI et le SDK), la connexion sur cet hôte, et un `COPILOT_HOME` à part si un poste a aussi un compte github.com.
  - Ce qui doit suivre l'hôte : quota et crédits, liste des modèles (souvent restreinte par l'entreprise), liens affichés ; le proxy d'entreprise des réglages appliqué.
  - Essai réel avec un compte GHE (à fournir).
- [ ] **OpenRouter**, si possible : Copilot CLI avec des modèles qui passent par OpenRouter plutôt que par ceux de Copilot (fournisseur de modèles à soi, « bring your own key »).
  - Relever dans le CLI et le SDK si et comment un tel fournisseur se déclare (clé, adresse de l'API, modèles). S'il ne se déclare pas, on s'arrête là.
  - Modèles : la liste vient d'OpenRouter (identifiants `fournisseur/modèle`), pas de `models.list` de Copilot.
  - Quota et coût : facturés par OpenRouter, pas en crédits de Copilot ; solde et usage de la clé par l'API d'OpenRouter, sinon coût affiché comme estimation.
  - La clé, si Escouade doit la connaître, gardée dans le trousseau comme les jetons des intégrations.
  - Essai réel avec une clé OpenRouter (à fournir).
- [ ] **Second compte** : un `COPILOT_HOME` à part et `copilot login`. La connexion va dans le trousseau du système : chaque dossier y a-t-il son entrée ? Autre piste : un jeton par session (`github_token` du SDK, `COPILOT_GITHUB_TOKEN`).
- [ ] **MCP par process** : `--additional-mcp-config @<fichier privé>` (vaut pour la session, remplace une entrée du même nom) et `--disable-mcp-server escouade` ; ou `mcpServers` à la création de session.
- [ ] **Reprise et copie** : `resume_session` ; la copie à un événement donné (`sessions().fork(id, to_event_id)`, expérimental) ; `history.truncate`.
- [ ] **Vue terminal** (voir « Vue terminal de chaque moteur ») :
  - les hooks d'Escouade pour un seul process : le dossier de hooks d'un `COPILOT_HOME`, avec la garde du jeton ;
  - un signal pour une permission en attente ;
  - `copilot --resume <id>` sur une session créée par le SDK (ou par ACP), et l'inverse ;
  - la touche qui interrompt, et `events.jsonl` pendant que la session tourne.

### Cursor CLI (`agent acp`, ACP sur stdio)
- [ ] **Voie** : `agent acp` (Agent Client Protocol : JSON-RPC 2.0, un message par ligne, sur stdio) est la seule qui attende une permission ou une réponse à une question.
  - Le mode headless (`agent -p --output-format stream-json --stream-partial-output`, `--resume <session>`) lance un process par tour, sans permission interactive (`--force` autorise tout). Il servira aux one-shots.
  - ACP est un protocole ouvert (lancé par Zed), parlé aussi par Copilot CLI et OpenCode : voir « Décisions », « ACP comme voie commune ». Il a une v1 et une v2 (cycle d'un prompt revu) : relever celle que parle Cursor.
- [ ] **Un tour complet enregistré** :
  - `session/update` : texte (`agent_message_chunk`), raisonnement, `tool_call` et `tool_call_update` (avec leur `kind` et leur diff), `plan` ;
  - `session/request_permission` ;
  - les extensions de Cursor : `cursor/ask_question` et `cursor/create_plan` attendent une réponse ; `cursor/update_todos`, `cursor/task` et `cursor/generate_image` sont de simples notifications ;
  - `session/cancel`, une compaction, une limite d'usage atteinte.
- [ ] **Modes** : `agent`, `plan` et `ask`.
  - Comment en changer en cours de session : `session/set_mode` d'ACP, ou seulement au lancement ?
  - Ce que valent, pour les modes d'Escouade, les règles `Shell(…)`, `Read(…)`, `Write(…)` et `WebFetch(…)` de `~/.cursor/cli-config.json` et de `.cursor/cli.json`.
  - « Toujours » (`allow-always`) écrit dans la config globale, même quand le projet a la sienne (forum de Cursor).
- [ ] **Modèles** : `agent models` liste ceux du compte. En ACP, en changer au lancement (`--model`) seulement, ou aussi en cours de session ?
- [ ] **Bac à sable** (`--sandbox enabled|disabled`, réseau réglé par `sandbox.json`) : même question que pour Codex sur le commit depuis un worktree, et ce qu'il donne sous Windows.
- [ ] **Consignes ajoutées** : ACP n'a pas de message système. Pistes : une règle passée au process, ou un préambule au premier message (gardé à la reprise, puisqu'il est dans l'historique).
- [ ] **MCP par process** : le serveur d'Escouade dans les `mcpServers` de `session/new` (une entrée HTTP avec l'en-tête `Authorization`).
  - La doc les montre vides, sans dire comment Cursor les traite.
  - Sinon, `.cursor/mcp.json` : un fichier du projet, auquel Escouade ne touche pas.
  - Une entrée `escouade` de l'utilisateur dans `~/.cursor/mcp.json` peut-elle être écartée pour un process ?
  - Les serveurs MCP d'une équipe (tableau de bord de Cursor) ne marchent pas en ACP.
- [ ] **Comptes** : `agent login`, une clé (`CURSOR_API_KEY`, `--api-key`) ou un jeton (`CURSOR_AUTH_TOKEN`). Existe-t-il un dossier de config à part par compte (une variable d'environnement) ? Sinon, un compte est une clé.
- [ ] **Usage et coût** : le flux headless ne donne ni tokens ni coût.
  - Chercher dans ACP et ses extensions l'usage d'un tour.
  - Chercher une source pour l'usage du mois. Les plans de Cursor ont un budget mensuel au tarif des API, plus l'usage d'Auto et de Composer, visibles sur le tableau de bord seulement.
- [ ] **Reprise et copie** : `session/load`. Une copie à un message donné existe-t-elle ?
- [ ] **Windows** : l'installeur PowerShell (`irm 'https://cursor.com/install?win32=true' | iex`) installe un `agent` natif, récent (gels de console signalés sur le forum). L'essayer sous Windows et sous WSL (projets WSL d'Escouade, 1.7.5).
- [ ] **Mises à jour** : le CLI se met à jour seul (`agent update`). Peut-on le figer ? Sinon, comment Escouade vit une version non testée arrivée dans la nuit (voir « Versions »).
- [ ] **Nom de la commande** : `agent` dans les docs, `cursor-agent` (ancien nom) encore cité. Chercher les deux. `agent` est un nom court qu'un autre programme peut avoir pris.
- [ ] **Instructions lues** : `AGENTS.md` et `.cursor/rules`. `CLAUDE.md` et `.claude/skills` aussi ? À vérifier, pour superpowers.
- [ ] **Vue terminal** (voir « Vue terminal de chaque moteur ») :
  - les hooks que le CLI envoie dans sa version du jour (au dernier relevé du forum, seulement `beforeShellExecution` et `afterShellExecution`) ;
  - où il garde ses conversations ;
  - `agent --resume <id>` sur une session ouverte par ACP, et `session/load`, qui rejoue ce qui s'est passé dans l'interface.

### OpenCode (`opencode serve`, HTTP + SSE)
- [ ] **Quelle ligne** : la v1.19 (`opencode-ai`, l'API documentée) ou la v2 (`@opencode/cli`, API refaite sous `/api/*`, « Experimental HttpApi surface »).
  - Les deux prennent la commande `opencode` et ne coexistent pas.
  - Regarder ce que donne aujourd'hui l'installation officielle, puis décider.
  - Escouade supporte une ligne en 1.8 et reconnaît l'autre pour dire qu'elle n'est pas prise en charge.
- [ ] **Ou ACP** : `opencode acp` (JSON-RPC sur stdio), à comparer avec `opencode serve`. Pour : un process par agent sans port ni mot de passe, et le client ACP de Cursor. Contre : ce qu'ACP ne donne pas (coût par message, sessions enfants ?).
- [ ] **Un serveur par agent ou par projet** : un serveur par agent garde le CPU et la RAM par agent, l'arbre de process tué d'un coup et l'environnement propre au compte. Mesurer la mémoire d'un serveur inactif et son temps de démarrage. Vérifier le port libre (`--port 0` ?) et le mot de passe (`OPENCODE_SERVER_PASSWORD`).
- [ ] **Un tour complet enregistré** : deltas, outils, `patch`, permission, question, `todo.updated`, sous-agent (outil `task`, session enfant), `abort`, erreur.
- [ ] **Message pendant un tour** (v1) : mis en file, refusé, ou il interrompt ?
- [ ] **Config par process** :
  - `OPENCODE_CONFIG_CONTENT` se fusionne-t-il avec la config de l'utilisateur ?
  - `{env:ESCOUADE_MCP_TOKEN}` marche-t-il dans un en-tête ?
  - `enabled:false` écrase-t-il une entrée de l'utilisateur ?
- [ ] **Consignes ajoutées** : le champ `system` d'un prompt, ou un agent déclaré dans la config du process.
- [ ] **Windows natif** : dossier des données, shell de l'outil `bash` (Git Bash ?), marche sans WSL (la doc recommande WSL).
- [ ] **Pas de bac à sable** du système (rien trouvé dans la doc) : à confirmer dans le code. Sans lui, seules les règles de permission protègent.
- [ ] **Modèles** : liste des seuls fournisseurs connectés, fenêtre de contexte de chaque modèle, `variant` (effort) quand le modèle en a.
- [ ] **Vue terminal** (voir « Vue terminal de chaque moteur ») :
  - `opencode attach http://127.0.0.1:<port> --session <id> --dir <worktree>`, avec le mot de passe du serveur dans l'environnement (`OPENCODE_SERVER_PASSWORD`) ;
  - une permission répondue dans l'interface est-elle vue par Escouade, et l'inverse ?
  - deux clients à la fois sur une session.

## 1. Codex, Copilot CLI, Cursor CLI et OpenCode, à côté de Claude Code

But : un agent tourne sur Claude Code, Codex, Copilot CLI, Cursor CLI ou OpenCode. Le reste d'Escouade ne change pas pour autant : conversation, Kanban et pilote auto, worktrees, git, lancements de test, stats, serveur MCP.

### Ce qu'on sait (2026-10-11, Cursor le 2026-10-14, sans essai réel)

| | Claude Code (aujourd'hui) | Codex 0.162 `app-server` | Copilot CLI 1.0.95 (SDK) | Cursor CLI `agent acp` | OpenCode 1.19 `serve` |
|---|---|---|---|---|---|
| Transport | JSONL sur stdio | JSON-RPC (JSONL) sur stdio, « expérimental » | JSON-RPC à en-têtes `Content-Length` sur stdio ; SDK Rust « GA », une partie des appels expérimentale | ACP : JSON-RPC 2.0 (JSONL) sur stdio ; aussi `-p` en `stream-json`, un process par tour | HTTP + SSE, OpenAPI 3.1 |
| Deltas | `stream_event` | `item/agentMessage/delta`, `item/reasoning/*Delta`, `outputDelta` | `assistant.message_delta`, `assistant.reasoning_delta`, `tool.shell_output` | `session/update` : `agent_message_chunk` (les autres à relever) | `message.part.delta` |
| Permissions | `can_use_tool` | `item/commandExecution\|fileChange\|permissions/requestApproval` | `permission.requested` typé (shell, write avec diff, read, mcp, url…), 7 réponses | `session/request_permission` → `allow-once` / `allow-always` / `reject-once` | `permission.asked` → `once` / `always` / `reject` |
| Question à l'utilisateur | `AskUserQuestion` | `item/tool/requestUserInput` (mode Plan seulement, sauf drapeau) | `user_input.requested`, `elicitation.requested` (formulaire) | `cursor/ask_question` (extension) | `question.asked` |
| Modes | 5 modes | politique d'approbation × bac à sable, `auto_review`, mode Plan | `manual` / `assisted` / `allow-all`, modes `plan` et `autopilot` | `agent` / `plan` / `ask`, règles `Shell(…)`, `Write(…)`… | agents `plan` / `build`, règles par outil |
| Bac à sable du système | non | oui (3 niveaux, Windows compris) | oui (`--sandbox`, Windows compris) | oui (`--sandbox`), Windows à vérifier | non trouvé |
| Interruption / message en cours de tour | `interrupt` / file | `turn/interrupt` / `turn/steer` | `abort` / `immediate` ou `enqueue` | `session/cancel` / à vérifier | `abort` / file en v2 seulement |
| Modèle, effort à chaud | `set_model` | au `turn/start` suivant | `model.switchTo` (effort compris) ; modèles Claude, GPT, Gemini | `--model`, `agent models` ; à chaud, à vérifier | par prompt (`model`, `variant`) |
| Reprise / copie | `--resume` / `--fork-session` à une entrée | `thread/resume` / `thread/fork` à un tour | `resume_session` / `fork` à un événement (expérimental) | `session/load` / inconnue | session / `fork` à un message |
| Coût | `total_cost_usd` | aucun | tokens et nano-AIU par appel | aucun connu | `cost` par message |
| Quota d'abonnement | `rate_limit_event`, `get_usage` | `account/rateLimits/*` (deux fenêtres en %, durée en minutes) | crédits d'IA du mois (`account.getQuota`, expérimental) ; limites de session et hebdo sans API connue | budget du mois sur le tableau de bord, sans API connue | aucun |
| Comptes | `CLAUDE_CONFIG_DIR` | `CODEX_HOME` | `COPILOT_HOME`, ou un jeton par session | `agent login`, ou une clé (`CURSOR_API_KEY`) | une config (XDG, `OPENCODE_CONFIG_DIR`, non vérifié) |
| MCP par process | `--mcp-config` | `-c mcp_servers.…` | `--additional-mcp-config`, `--disable-mcp-server` | `mcpServers` de `session/new` (à vérifier) ; sinon `.cursor/mcp.json` | `OPENCODE_CONFIG_CONTENT` |
| Tâches / sous-agents | outils de tâches, `Task` | `turn/plan/updated` / `collabToolCall` | `session.todos_changed` puis lecture par appel / `subagent.*` | `cursor/update_todos` (ou `plan` d'ACP) / `cursor/task` | `todo.updated` / outil `task`, sessions enfants |
| Instructions lues | `CLAUDE.md`, `.claude/` | `AGENTS.md` | `AGENTS.md`, `CLAUDE.md`, `.github/copilot-instructions.md`, `.claude/skills` | `AGENTS.md`, `.cursor/rules` | `AGENTS.md` |
| Commandes `/` pour un client | `initialize` | `skills/list` | aucune (réservées à l'interface du terminal) | à relever (`available_commands_update` d'ACP ?) | `/command`, `/skill` |
| Transcripts | `~/.claude/projects/…/*.jsonl` | `$CODEX_HOME/sessions/…/rollout-*.jsonl` (format interne) | `~/.copilot/session-state/<id>/events.jsonl` | non relevés | SQLite, schéma non public |

Escouade tient déjà son propre journal normalisé de chaque conversation (`conv.rs`). La recherche, la relecture, les cartes et les stats lisent ce journal, pas les transcripts des CLI, donc les transcripts des autres moteurs n'ont pas à être lus.

### Refonte multi-moteurs, avant le premier moteur (demandé le 2026-10-14)
But : rendre Escouade multi-moteurs en profondeur avant d'écrire Codex, Copilot, Cursor ou OpenCode. Ajouter un moteur, ceux-ci puis d'autres, se résume ensuite à écrire son adaptateur, déclarer ses capacités et fournir son faux moteur.

Elle regroupe ce qui était « Extraction d'une interface moteur » et « Reste commun », plus trois briques : la vue terminal générique, le banc de conformité et le moteur ACP générique. Les sections suivantes de ce chapitre gardent le détail de chaque partie. Ce qui y est propre à un moteur se fait avec lui.

Lots, dans l'ordre :
1. **Interface « moteur »** : extraction de Claude Code, sans changement de comportement (voir « Un moteur par agent »).
2. **Données** : moteur de l'agent, du ticket et du projet ; colonne `engine` des stats.
3. **Registre des moteurs** : détection, versions testées, capacités déclarées (`EngineCaps`), que l'interface lit au lieu de tester « claude ».
4. **Interface** : Réglages › Moteurs, choix du moteur, marque sur les cartes, menus du composer fournis par le moteur (voir « Choisir le moteur », « Composer »).
5. **Comptes et quotas par moteur** (voir « Comptes et quotas », « Coût, tokens, stats »).
6. **Textes et consignes** indépendants du moteur (voir « Textes »).
7. **Textes rédigés** par moteur (voir « Textes rédigés par un modèle »).
8. **Serveur MCP** par moteur (voir « Serveur MCP d'Escouade »).
9. **Vue terminal générique** et commande `escouade-hook` (voir « Vue terminal de chaque moteur »).
10. **Banc de conformité** des moteurs, et guide `docs/ENGINES.md` « Ajouter un moteur ».
11. **Moteur ACP générique** : client, extensions propres à un moteur, faux agent ACP.
12. **Continuer sur un autre moteur** quand le quota est épuisé (voir la section du même nom, ESC-88).

Critère d'ensemble : à la fin, avec Claude seul, rien ne change pour l'utilisateur, et le faux moteur du banc se branche sans toucher au cœur.

### Un moteur par agent
- [ ] `AgentMeta.engine` vaut `claude`, `codex`, `copilot`, `cursor` ou `opencode`. Par défaut, `claude` (les agents d'avant la 1.8). Un agent a un moteur à la fois. Il en change seulement par une passation (voir « Continuer sur un autre moteur quand le quota est épuisé »), et chaque tour garde le moteur qui l'a fait ; sa copie prend son moteur du moment.
- [ ] Ce qui est propre à Claude passe derrière une interface « moteur ». C'est aujourd'hui le process stdio et ses requêtes de contrôle (`claude.rs`), et la traduction des frames en éléments de conversation (`AgentRt::handle_frame`). L'interface couvre : lancer, reprendre, copier, envoyer un message avec ses pièces jointes, interrompre, changer de modèle, d'effort ou de mode, répondre à une permission ou une question, lire l'usage, arrêter.
- [ ] Chaque moteur traduit son flux dans les mêmes éléments de conversation (`user`, `text`, `thinking`, `tool`, `question`, `permission`, `turn`, `notice`, `event`) et les mêmes changements du bandeau « Plan ».
- [ ] **D'abord une extraction pure**, avec Claude seul et sans aucun changement de comportement : les tests existants (`core_tests`, `agent.rs`, e2e) passent tels quels.
- [ ] **Outils** : le backend donne aux outils des autres moteurs le nom que leur donne Claude Code. `tools.ts`, la ligne d'activité, `convsearch.rs` et les cartes ne changent donc pas. Un outil inconnu s'affiche sous son propre nom.

  | Moteur | Outil d'origine | Nom pris dans Escouade |
  |---|---|---|
  | Codex | `commandExecution` | `Bash` |
  | Codex | `fileChange` | `Edit` ou `Write`, avec ses +N −M et le fichier touché |
  | Codex | `mcpToolCall` | `mcp__<serveur>__<outil>` |
  | Codex | `webSearch` | `WebSearch` |
  | Copilot | shell, écriture, lecture… (noms relevés au point 0) | leur équivalent chez Claude Code ; les diffs viennent de `detailedContent` et des `fileEdits` des permissions |
  | Copilot | outil MCP (`serverName`, `toolName`) | `mcp__<serveur>__<outil>` |
  | Cursor | `tool_call` d'ACP, selon son `kind` : `execute`, `edit`, `read`, `search`, `fetch` | `Bash`, `Edit` (ou `Write`), `Read`, `Grep`, `WebFetch` ; les noms propres à Cursor (`readToolCall`, `writeToolCall`…) relevés au point 0 |
  | Cursor | outil MCP | `mcp__<serveur>__<outil>` |
  | OpenCode | `bash`, `edit`, `write`, `read`, `grep`, `glob`, `list`, `patch`, `webfetch`, `todowrite`, `task` | leur équivalent chez Claude Code |
- [ ] **Process** :
  - Codex : un `codex app-server` par agent.
  - Copilot : un `copilot --server --stdio` par agent (par la crate du SDK si elle laisse Escouade lancer le process, voir point 0).
  - Cursor : un `agent acp` par agent.
  - OpenCode : un `opencode serve` par agent, sur `127.0.0.1`, avec un port libre et un mot de passe tiré au hasard, sauf si l'essai du point 0 le montre trop coûteux. Le backend lit son flux SSE `/event`.
  - Dans tous les cas, le process est dans le Job de l'agent, ce qui garde le CPU et la RAM par agent et tue tout l'arbre d'un coup.
  - L'arrêt pour inactivité et la reprise restent ceux d'aujourd'hui (`thread/resume`, `resume_session`, ou session rouverte par son id).
- [ ] **Versions** :
  - Chaque moteur a une plage de versions testées. Une version plus ancienne est refusée avec sa raison. Une plus récente est acceptée avec un avertissement (« Codex 0.170 n'a pas été testé avec Escouade »).
  - La lecture tolère l'inconnu : champs ignorés, et une notification ou un élément inconnu est écarté sans casser le tour.
  - Les schémas des versions testées sont gardés dans le dépôt, et un test vérifie que les champs lus par Escouade y sont : `codex app-server generate-json-schema` pour Codex, le schéma du protocole du SDK pour Copilot, le schéma d'ACP et la liste des extensions `cursor/*` pour Cursor.
  - Cursor se met à jour seul : une version non testée peut arriver sans que l'utilisateur ait rien fait. L'avertissement le dit, et la lecture tolérante de l'inconnu compte d'autant plus.

### Choisir le moteur
- [ ] **Réglages › « Moteurs »** (aujourd'hui l'onglet « Claude ») : pour chaque moteur, l'exécutable (comme `claude_path`), la version trouvée, son état (absent, non connecté, prêt, version non testée) et « Se connecter… » (un terminal intégré avec la commande de connexion). Un moteur absent n'apparaît nulle part ailleurs.
- [ ] **Premier lancement** : Escouade marche dès qu'un moteur est installé. « Claude Code introuvable » devient « Aucun moteur trouvé », avec la commande d'installation de chacun.
- [ ] **Projet** : « Moteur par défaut ». Par défaut, Claude Code, ou le seul moteur installé.
- [ ] **Nouvel agent** : un menu « Moteur », réglé d'abord sur celui du projet.
- [ ] **Ticket** : un champ « Moteur » (par défaut, celui du projet), que le pilote auto suit.
- [ ] **Marque du moteur** sur la carte de l'agent, dans la barre latérale, la vue d'ensemble et l'onglet, dès que plus d'un moteur sert.

### Composer : modèles, effort, modes
- [ ] **Modèles** : le menu liste ceux du moteur de l'agent.
  - Claude : les alias, comme aujourd'hui.
  - Codex : `model/list`, avec `supportedReasoningEfforts` et `inputModalities`.
  - Copilot : `models.list`, avec ses efforts, ses capacités et son multiplicateur de facturation (écrit à côté du modèle : un modèle qui coûte trois fois plus le dit).
  - Cursor : `agent models`, les modèles du compte (Auto compris). Le changement en cours de session dépend du point 0.
  - OpenCode : les modèles des seuls fournisseurs connectés, avec une recherche et des favoris dans les réglages, car la liste peut être longue.
- [ ] **Effort** : celui du modèle choisi (efforts de raisonnement de Codex et de Copilot, `variant` d'OpenCode). Le menu est masqué si le modèle n'en a pas.
- [ ] **Modes** : Escouade garde ses cinq modes. Chaque moteur dit lesquels il prend et comment. Le titre du mode dans le menu explique ce qu'il veut dire pour ce moteur, bac à sable compris. À confirmer par les essais :

  | Escouade | Codex | Copilot | Cursor | OpenCode |
  |---|---|---|---|---|
  | Plan | mode Plan, bac à sable `read-only` | mode `plan` | mode `plan` | agent `plan` |
  | Demander | `on-request`, `workspace-write` | `manual` | mode `agent`, chaque outil demandé | agent `build`, `edit` et `bash` en `ask` |
  | Accepter les modifications | `workspace-write` : les écritures du bac à sable passent, une commande qui en sort demande | `manual`, les demandes `write` du dossier de l'agent acceptées d'office (ou `--allow-tool write`) | mode `agent`, les écritures du dossier de l'agent acceptées d'office par Escouade | `edit` en `allow`, `bash` en `ask` |
  | Auto | `on-request` + `approvalsReviewer: auto_review` | `assisted` ou `autopilot` (voir point 0) | absent (masqué) | absent (masqué) |
  | Sans permissions | `never`, `danger-full-access` | `allow-all` | `--force` (en ACP, à vérifier) | tout en `allow` |

  Le mode `ask` de Cursor (questions, lecture seule) n'a pas d'équivalent dans Escouade : il n'est pas proposé.
- [ ] **Changement à chaud** : avec Codex, il vaut à partir du tour suivant (`turn/start`). Avec Copilot, tout de suite (`model.switchTo`, `mode.set`). Avec OpenCode, à chaque prompt. Avec Cursor, selon le point 0. Le menu le dit quand un tour est en cours.
- [ ] **Commandes `/`** :
  - Claude : comme aujourd'hui.
  - Codex : ses skills (`skills/list`), envoyés comme élément `skill`.
  - Copilot : aucune n'est offerte à un client (celles de son terminal restent dans son terminal). Celles qui ont un appel à elles (compacter, par exemple) deviennent des actions d'Escouade.
  - Cursor : celles qu'annonce ACP (`available_commands_update`), si Cursor en annonce (point 0).
  - OpenCode : `/command` et `/skill`.
- [ ] **Suggestions `@`** : l'index de fichiers d'Escouade (celui de « Ouvrir un fichier ») pour les autres moteurs.
- [ ] **Pièces jointes** :
  - Images : quand les modalités du modèle les acceptent (pièce jointe `file` ou `blob` chez Copilot, bloc `image` d'ACP chez Cursor s'il annonce `promptCapabilities.image`).
  - PDF : Codex ne les lit pas. Le bouton le dit et les refuse. Avec Copilot, Cursor et OpenCode, ça dépend du modèle (à vérifier).
  - Documents texte : collés comme texte.
- [ ] **Message pendant un tour** :
  - Codex : « maintenant » passe par `turn/steer`.
  - Copilot : « maintenant » passe par `immediate`, « ensuite » par `enqueue`.
  - Ailleurs, Escouade met le message en file jusqu'à la fin du tour, et c'est le cas pour OpenCode v1 si l'essai le demande.

### Permissions et questions
- [ ] **Codex** :
  - Une commande donne une carte « Codex veut lancer : <commande> », avec son dossier et sa raison.
  - Une modification de fichier donne une carte avec son diff.
  - `item/permissions/requestApproval` donne une carte qui dit ce qui est demandé.
  - Boutons : « Autoriser » (`accept`), « Pour cette session » (`acceptForSession`), « Refuser » (`decline`).
- [ ] **Copilot** : `permission.requested` donne une carte selon son `kind`.
  - Shell : la commande entière et les chemins qu'elle touche.
  - Écriture : le diff.
  - Lecture et URL : le chemin ou l'adresse.
  - MCP : le serveur, l'outil, et s'il ne fait que lire.
  - Boutons : « Autoriser » (`approve-once`), « Pour cette session » (`approve-for-session`), « Refuser » (`reject`).
  - `approve-permanently` et `approve-for-location` (écrits dans la config de Copilot) : à proposer ou non, voir « Décisions ».
- [ ] **Cursor** : `session/request_permission` donne une carte selon le `kind` de l'outil : la commande, la modification avec son diff, le fichier lu, l'adresse, l'outil MCP. Boutons : « Autoriser » (`allow-once`), « Toujours » (`allow-always`, qui écrit dans `~/.cursor/cli-config.json` : voir « Décisions »), « Refuser » (`reject-once`).
- [ ] **OpenCode** : `permission.asked` donne une carte. Boutons : « Autoriser » (`once`), « Toujours » (`always`, qui écrit le motif retenu, `patterns`), « Refuser » (`reject`, avec le message saisi).
- [ ] **Questions** : la carte de question existante reçoit :
  - Codex : `item/tool/requestUserInput` (options, « Autre », champ secret masqué). Ces questions n'arrivent qu'en mode Plan : le dire dans la doc.
  - Copilot : `user_input.requested` (choix et réponse libre) et `elicitation.requested` (un formulaire de plusieurs champs, qui demande une carte un peu plus riche).
  - Cursor : `cursor/ask_question` (une extension, qui attend la réponse).
  - OpenCode : `question.asked`.
- [ ] **Fin d'un plan** : chez Copilot, `exit_plan_mode.requested` (résumé, plan, actions proposées) devient la carte « Plan à approuver » qu'Escouade montre déjà pour Claude. Chez Cursor, `cursor/create_plan`, qui attend la réponse, de même.
- [ ] Les notifications, la liste « À répondre » de la vue d'ensemble et le pilote auto ne changent pas : ils lisent les demandes normalisées.

### Plan, tâches, sous-agents (bandeau « Plan » de la 1.7)
- [ ] **Codex** : `turn/plan/updated` (les étapes d'`update_plan` et leur statut) donne les tâches, `collabToolCall` les sous-agents.
- [ ] **Copilot** : `session.todos_changed` dit que les tâches ont changé, puis Escouade les lit (`plan.readSqlTodosWithDependencies`, dépendances comprises). Les sous-agents viennent de `subagent.*` et de l'`agentId` de chaque événement.
- [ ] **Cursor** : `cursor/update_todos` (ou le `plan` d'ACP) donne les tâches, `cursor/task` les sous-agents (à confirmer au point 0).
- [ ] **OpenCode** : `todo.updated` (`pending`, `in_progress`, `completed`, `cancelled`) donne les tâches. L'outil `task` et les sessions enfants donnent les sous-agents, avec leurs tokens et leurs outils.
- [ ] Les fichiers de plan (superpowers) se lisent sur le disque comme aujourd'hui, quel que soit le moteur. Faire un essai réel avec superpowers sous Codex, sous Copilot (qui lit `.claude/skills`) et sous Cursor.

### Comptes et quotas
- [ ] « Comptes Claude » devient « Comptes », groupés par moteur.
- [ ] **Comptes Codex** : un compte est un `CODEX_HOME`.
  - Principal est `~/.codex`, lancé sans toucher à la variable, comme pour Claude.
  - Les autres sont dans `~/.escouade/codex/<nom>`. « Ajouter un compte » ouvre un terminal intégré avec `codex login`.
  - Ce qui se partage (`config.toml`, `AGENTS.md`, skills) est proposé par lien ou par copie. `auth.json` n'est jamais copié.
- [ ] **Comptes Copilot** : un compte est un `COPILOT_HOME`.
  - Principal est `~/.copilot`, lancé sans la variable.
  - Les autres sont dans `~/.escouade/copilot/<nom>`, avec `copilot login` dans un terminal intégré.
  - Ce qui se partage : `mcp-config.json`, `copilot-instructions.md`, agents, skills. La connexion n'est jamais copiée.
  - Si le trousseau ne sépare pas les dossiers (point 0) : un jeton par compte, gardé dans le trousseau d'Escouade et passé à la session.
- [ ] **Comptes Cursor** :
  - Principal est la connexion de `agent login`, lancé sans rien changer.
  - Les autres sont une clé d'API par compte, gardée dans le trousseau et passée par `CURSOR_API_KEY` dans l'environnement du process, jamais sur la ligne de commande. Sauf si le point 0 trouve un dossier de config à part par compte, à traiter alors comme `CODEX_HOME`.
- [ ] **Quota de Codex** :
  - Source : `account/rateLimits/updated` sur un agent vivant. Sinon, `account/rateLimits/read` toutes les 5 min, par un app-server du compte lancé pour ça (voir point 0).
  - Les fenêtres prennent un libellé selon leur durée (`windowDurationMins` 300 → « 5h », 10080 → « 7j » / « W »), avec `usedPercent` et `resetsAt`.
  - Un tour arrêté par la limite (`rateLimitReachedType`) attend la remise à zéro, puis reprend seul, comme chez Claude.
- [ ] **Quota de Copilot** :
  - Une barre « crédits du mois » (`remainingPercentage`, remise à zéro à `resetDate`), lue dans les `quotaSnapshots` de chaque appel, sinon par `account.getQuota`.
  - Les limites par session et par semaine en plus, si le point 0 leur trouve une source.
  - Un tour arrêté parce que les crédits sont épuisés attend la remise à zéro ou le dit (dépassement payant possible selon le plan). Si la session a un plafond (`--max-ai-credits`), son événement `session_limits_exhausted.requested` devient une carte.
- [ ] **Quota de Cursor** : aucune source connue. Les plans ont un budget mensuel au tarif des API, plus l'usage d'Auto et de Composer, visibles sur le tableau de bord de Cursor.
  - Si le point 0 trouve une source : une barre « usage du mois », comme pour Copilot.
  - Sinon : pas de barre, un lien vers le tableau de bord dans le panneau des comptes, et pas de pause au quota pour le pilote auto, comme pour OpenCode.
  - Un tour arrêté par la limite devient une carte qui le dit.
- [ ] **Comptes réservés aux projets, pour tout moteur** : chaque compte, de tout moteur, peut être « Réservé aux projets » cochés, comme les comptes Claude aujourd'hui (Principal compris), avec les mêmes règles. Les moteurs ont un ordre, réglable : il donne les moteurs de secours de chaque projet (voir la section suivante).
- [ ] **Bascule automatique** : entre comptes du même moteur seulement (passer à un autre moteur : voir la section suivante). Les nouveaux agents et tickets partent sur le compte suivant sous le seuil. Reprendre un agent Codex, Copilot ou Cursor sur un autre compte ne se fait que si l'essai du point 0 l'a permis.
- [ ] **OpenCode** : il n'a pas de quota d'abonnement (il passe par les API des fournisseurs).
  - Un seul compte en 1.8 : sa config par défaut.
  - Pas de barre de quota : la barre d'état montre seulement le coût du jour.
  - Pas de pause au quota pour le pilote auto (voir « Décisions »).
- [ ] **Barre d'état** : le groupe des quotas montre les comptes du moteur de l'agent affiché (sinon, du moteur par défaut du projet). Le panneau des comptes les liste tous, groupés par moteur.

### Continuer sur un autre moteur quand le quota est épuisé (demandé le 2026-10-14, ESC-88)
But : « je n'ai plus de quota Claude, je passe sur Codex ». Aujourd'hui, un agent arrêté par la limite attend la remise à zéro, ou reprend sur un autre compte Claude (docs/SPEC.md, « Reprise sur un autre compte »). Quand tous les comptes du moteur d'un agent sont épuisés, il doit pouvoir continuer sur un autre moteur.

- [ ] **Une passation, pas une copie de session** : une session ne passe pas d'un moteur à l'autre.
  - Même agent, même conversation dans Escouade, même worktree, même branche. Un nouveau process du moteur cible, sur le compte que choisit sa propre règle.
  - Le premier message est un dossier de passation construit par Escouade depuis son journal (`conv.rs`), sans le moteur épuisé : demandes de l'utilisateur ; travail fait (derniers messages de l'agent, avancement, tâches du plan) ; fichiers modifiés (`git status`, `diff --stat`) ; tour coupé, et où ; question ou permission en attente.
  - Pour un ticket, son protocole (critères, boucle, bilan) repasse par les consignes ajoutées.
  - Le dossier est borné à la fenêtre de contexte du modèle cible ; les tours anciens peuvent être résumés par le modèle des textes rédigés du moteur cible.
- [ ] **Mode et modèle** : le même mode si le moteur cible l'a, sinon le plus proche qui n'autorise jamais plus. Un modèle de passation par moteur, réglable (par défaut, son meilleur).
- [ ] **Dans la conversation** : une séparation « Suite sur Codex · quota Claude épuisé jusqu'à 18:00 », puis la marque du moteur sur les tours suivants. La carte montre le moteur du moment. Chaque tour garde son moteur (stats, coût).
- [ ] **Moteurs de secours, par projet** (décidé le 2026-10-14) : comme les comptes Claude aujourd'hui, sans réglage à part.
  - Chaque compte, de tout moteur, peut être « Réservé aux projets » cochés, et les moteurs ont un ordre, réglable (voir « Comptes et quotas »).
  - Pour un projet, la suite est : son moteur par défaut, puis les autres moteurs dans l'ordre réglé, seulement ceux qui ont un compte actif servant ce projet.
  - Un projet qu'aucun autre moteur ne sert attend la remise à zéro, comme aujourd'hui.
- [ ] **Quand** (décidé le 2026-10-14 : l'automatique est une option) :
  - par défaut, rien ne change : l'agent attend la remise à zéro et reprend seul ;
  - « Continuer sur <moteur> » sur la carte de fin d'un tour arrêté par la limite, pour le moteur suivant du projet (un menu s'il y en a plusieurs), à côté de « Reprendre sur <compte> » ;
  - « Continuer sur un autre moteur… » dans le menu de l'agent, à tout moment ;
  - une option, désactivée par défaut, « Continuer sur le moteur suivant quand tous les comptes d'un moteur sont épuisés » : l'agent y passe seul, et le pilote auto la suit pour les tickets.
- [ ] **Retour** (décidé le 2026-10-14 : jamais automatique) : le moteur quitté est mis de côté jusqu'à sa remise à zéro (pas d'allers-retours, comme le compte quitté aujourd'hui). Ensuite, l'agent reste sur son moteur de secours et « Revenir sur Claude » est proposé : une passation dans l'autre sens, ou l'ancienne session de ce moteur reprise et complétée par ce qui s'est passé depuis.
- [ ] **Vue terminal** : l'agent continue dans la vue du moteur cible s'il en a une, sinon en vue Escouade.
- [ ] **Tests** : deux faux moteurs du banc de conformité (limite sur le premier, suite sur le second, retour) ; essai réel Claude → Codex avant la release (ESC-52).

### Coût, tokens, stats
- [ ] **Codex** : les tokens d'un tour sont le `last` de `thread/tokenUsage/updated` (entrée, cache lu, cache écrit, sortie, raisonnement). Codex ne donne aucun coût en dollars : il est estimé avec une grille des modèles OpenAI dans `pricing.rs` et marqué « estimé ». Avec un abonnement ChatGPT, c'est un coût théorique, comme celui de Claude avec Max.
- [ ] **Copilot** : les tokens de chaque appel (`assistant.usage`) additionnés par tour. Le coût vient des nano-AIU convertis en dollars (conversion à vérifier au point 0), sinon des prix par token de `models.list`.
- [ ] **Cursor** : ni tokens ni coût connus dans son flux, à chercher au point 0. Sans source, le coût d'un tour n'est pas affiché (« — ») et les stats de Cursor comptent les tours et leur durée.
- [ ] **OpenCode** : `cost` et `tokens` de chaque message, additionnés par tour.
- [ ] **Fenêtre de contexte** : `modelContextWindow` chez Codex, `session.usage_info` chez Copilot, la limite du modèle chez OpenCode ; chez Cursor, à relever au point 0.
- [ ] **Stats** :
  - Une colonne `engine` dans la table des tours (les tours d'avant valent `claude`).
  - Un filtre « Tous les moteurs / Claude Code / Codex / Copilot / Cursor / OpenCode », qui se combine avec celui des comptes.
  - Une vue « Par moteur ».

### Serveur MCP d'Escouade
- [ ] **Agents d'Escouade** (réglage « Les agents peuvent utiliser Escouade ») : même jeton par lancement et même oubli qu'aujourd'hui.
  - Codex : `-c mcp_servers.escouade.url=…` et `bearer_token_env_var`, avec le jeton dans l'environnement du process. Réglage désactivé : `-c mcp_servers.escouade.enabled=false`.
  - Copilot : `--additional-mcp-config @<fichier privé>` (comme le `--mcp-config` de Claude : un fichier 0600, oublié à la fin du process), ou `mcpServers` à la création de session. Réglage désactivé : `--disable-mcp-server escouade`.
  - Cursor : une entrée HTTP dans les `mcpServers` de `session/new`, avec l'en-tête `Authorization`, si le point 0 montre que Cursor les prend. Réglage désactivé : rien de passé (pour une entrée `escouade` de l'utilisateur, voir point 0).
  - OpenCode : `OPENCODE_CONFIG_CONTENT`, avec une entrée `remote` et l'en-tête `Authorization: Bearer {env:ESCOUADE_MCP_TOKEN}`. Réglage désactivé : `enabled:false`.
- [ ] Leurs appels prennent le nom `mcp__escouade__<outil>` dans la conversation, donc la carte de permission et les libellés des outils d'Escouade marchent tels quels.
- [ ] **« <Moteur> peut piloter Escouade »** (déclaration pour l'utilisateur, après tout le reste) :
  - Codex : une entrée dans le `config.toml` de chaque compte, par `codex mcp add` s'il prend une URL et un en-tête, sinon par une édition qui garde les commentaires (`toml_edit`).
  - Copilot : une entrée dans le `mcp-config.json` de chaque `COPILOT_HOME`.
  - Cursor : une entrée dans `~/.cursor/mcp.json`, la config qu'il partage avec l'éditeur Cursor. L'éditeur pourrait alors piloter Escouade lui aussi.
  - OpenCode : une entrée dans sa config globale (JSONC, commentaires gardés).
  - Le jeton reste en clair dans ces fichiers : le dire, comme pour Claude.

### Textes rédigés par un modèle (one-shots)
- [ ] Aujourd'hui, ces textes passent par Haiku via `claude -p` : nom d'un agent, message de commit proposé, et les autres questions de `ask_claude`. Un utilisateur sans Claude Code doit aussi les avoir.
- [ ] Réglage « Modèle des textes rédigés », avec cinq choix. Mêmes limites qu'aujourd'hui : 90 s, lecture bornée.
  - Claude Haiku : par défaut quand Claude Code est installé.
  - Codex : `codex exec`, petit modèle, bac à sable `read-only`, éphémère.
  - Copilot : `copilot -p … --output-format json`, petit modèle, sans outil (`--available-tools` vide, à vérifier). Le schéma de sa sortie n'est pas documenté : lire la seule réponse finale.
  - Cursor : `agent -p --output-format json`, avec un petit modèle, en lecture seule (mode `ask` ou sans `--force` : à vérifier).
  - OpenCode : `opencode run`, avec le modèle choisi.

### Vue terminal de chaque moteur (demandé le 2026-10-14)
But : comme en vue Claude Code (`docs/PROTOCOL.md`, « Vue Claude Code »), un agent tourne dans l'interface en terminal de son moteur, à la place de la conversation d'Escouade. Escouade continue de le suivre : état sur sa carte, notifications, permission en attente, fin de tour, coût estimé, recherche dans sa conversation.

- [ ] **Le modèle de la vue Claude Code**, repris pour chaque moteur :
  - l'interface tourne dans un terminal intégré, dans le worktree de l'agent, avec l'environnement de son compte et le serveur MCP d'Escouade ;
  - une session n'a qu'un process à la fois : changer de vue arrête l'un et reprend la session dans l'autre ;
  - des hooks disent l'état en direct (début et fin de tour, permission ou question en attente) ;
  - le journal de session du moteur donne la conversation, rejouée dans le traducteur du moteur ;
  - un message de la fenêtre ou du serveur MCP est collé dans l'interface (collage entre crochets, puis Entrée), et une interruption envoie la touche du moteur (Échap chez Claude Code, à relever ailleurs) ;
  - un agent de ticket reste sur le protocole ;
  - quand l'interface demande la confiance du dossier, Escouade ne répond jamais à la place de l'utilisateur.
- [ ] **Hooks par commande** : Claude Code appelle des hooks HTTP, alors que Codex et Copilot lancent des commandes.
  - Escouade fournit une petite commande (`escouade-hook`, livrée avec l'app) qui lit l'événement sur son entrée et le poste au serveur de hooks d'Escouade, avec le jeton de l'interface lu dans l'environnement (`ESCOUADE_HOOK_TOKEN`).
  - Sans ce jeton, elle sort aussitôt sans rien faire. Un hook déclaré dans un dossier que l'utilisateur partage avec ses propres sessions ne les touche donc pas.
- [ ] **Par moteur** (à confirmer au point 0) :

  | Moteur | Interface | État en direct | Conversation | Changer de vue |
  |---|---|---|---|---|
  | Codex | `codex` | hooks, stables depuis la 0.124 (`SessionStart`, `PreToolUse`, `PostToolUse`, `Stop`…), par `hooks.json` ou `[hooks]` de `config.toml` | le `rollout-*.jsonl` de la session (format interne) | `codex resume <id>` ↔ `thread/resume`, dans le même `CODEX_HOME` |
  | Copilot | `copilot` | hooks de `~/.copilot/hooks/*.json` (ou du `COPILOT_HOME` du compte) : `preToolUse`, `postToolUse`, `agentStop`, `sessionEnd`… | `session-state/<id>/events.jsonl` | `copilot --resume <id>` ↔ `resume_session` |
  | Cursor | `agent` | hooks du CLI partiels : seuls `beforeShellExecution` et `afterShellExecution` partent (forum de Cursor), pas de `stop` | journal non relevé ; au retour en vue Escouade, `session/load` d'ACP rejoue l'historique | `agent --resume <id>` ↔ `session/load` |
  | OpenCode | `opencode attach <adresse> --session <id>` | le flux SSE du serveur de l'agent, comme en vue Escouade | le même flux | rien à arrêter : l'interface se branche sur le serveur de l'agent, et les deux vues marchent en même temps |

- [ ] **OpenCode d'abord** : c'est le plus simple, sans hook ni journal à lire. Seulement si OpenCode passe par `opencode serve` (voir « Décisions », « ACP comme voie commune »).
- [ ] **Cursor en suivi réduit**, tant que son CLI n'envoie pas plus de hooks :
  - pas de fin de tour ni de permission en attente sur la carte, ni de notification ;
  - la vue le dit (« Suivi limité : Cursor ne prévient pas Escouade de la fin d'un tour ») ;
  - l'historique revient au retour en vue Escouade ;
  - à revoir à chaque version du CLI.
- [ ] **Permission en attente** : Claude Code a `PermissionRequest`. Chez Codex et Copilot, trouver le signal : un hook dédié, ou le `PreToolUse` d'un outil que la politique fera demander. Sans lui, la carte ne dit pas « attend ta réponse ».
- [ ] **Bouton et réglage** : « Vue Claude Code » devient « Vue <moteur> » (« Vue Codex »…) dans l'en-tête de la conversation et le clic droit sur la carte. Le réglage des nouveaux agents propose « Escouade » ou « Terminal du moteur ». Textes fr et en.
- [ ] **Coût** : estimé depuis l'usage lu dans le journal, comme en vue Claude Code. Rien pour Cursor.
- [ ] **Tests** : chaque faux moteur a aussi une interface factice, lancée dans un pseudo-terminal, qui écrit son journal et appelle ses hooks, comme pour la vue Claude Code (`tests/fixtures/tui-transcript.jsonl`, `tui-hooks.jsonl`).

### Ce qui reste propre à Claude Code
- [ ] Ces fonctions sont masquées pour les autres moteurs, pas grisées :
  - Remote Control (Copilot envoie à son agent dans le cloud par `/delegate`, Cursor a ses agents dans le cloud : d'autres fonctions, hors de ce chantier).
  - L'avertissement « Claude Code refuserait par défaut ».
  - Les tâches de fond et les workflows.
  - `CLAUDE_CODE_ENABLE_TODO_TOOLS`.
- [ ] La copie d'un agent se fait au dernier tour (Codex), au dernier événement (Copilot) ou au dernier message (OpenCode), qui est le même point que celui de Claude. Pour Cursor, selon le point 0 ; s'il ne sait pas copier, « Dupliquer la conversation » est masqué pour ses agents.

### Textes
- [ ] Environ 100 « Claude » par catalogue (fr et en).
  - Ceux qui parlent de l'agent prennent le nom du moteur : « {engine} attend ta réponse ».
  - Ceux qui parlent de Claude lui-même restent (comptes Claude, Remote Control).
  - « Langue des textes rédigés par Claude » devient « … par les agents ».
- [ ] Consignes ajoutées et invites du pilote auto :
  - Codex : `developerInstructions`.
  - Copilot : le message système de la session (point 0).
  - Cursor : une règle passée au process, ou un préambule au premier message (point 0).
  - OpenCode : le `system` du prompt.
  - Leur texte ne nomme plus d'outil propre à Claude Code.

### Tests
- [ ] **Faux moteurs** : `fake-codex.mjs` (app-server JSON-RPC), `fake-copilot.mjs` (JSON-RPC à en-têtes `Content-Length`), `fake-cursor.mjs` (ACP, JSON-RPC sur stdio, avec les extensions `cursor/*`) et `fake-opencode.mjs` (serveur HTTP + SSE), avec leurs `.cmd`. Mêmes scénarios que le faux `claude` : texte, raisonnement, commande avec approbation, modification de fichier, question, interruption, plan, limite, reprise, copie, quota.
- [ ] **Tests unitaires** : chaque traduction (flux réel enregistré → éléments de conversation), la correspondance des modes, l'estimation du coût, le libellé des fenêtres de quota.
- [ ] **e2e** :
  - Un agent sur Codex : message, approbation d'une commande, diff, fin de tour.
  - Le même sur Copilot, sur Cursor, puis sur OpenCode.
  - Un ticket lancé sur Codex par le pilote auto.
  - Les stats filtrées par moteur.
- [ ] **Avant la release, un essai réel par moteur** : un vrai compte, un ticket en worktree jusqu'au commit et au merge, un plan avec sous-agents.

### Docs
- [ ] PROTOCOL.md : sections « Codex app-server », « Copilot SDK », « Cursor (ACP) » et « OpenCode serve » (versions testées, méthodes et événements utilisés, correspondance des modes et des outils).
- [ ] SPEC : section « Moteurs ».
- [ ] README (installation de chaque moteur) et CHANGELOG.
- [ ] Site, en français et en anglais : Escouade pilote Claude Code, Codex, Copilot CLI, Cursor CLI et OpenCode, avec des captures.

### Décisions à prendre
- [ ] **Ligne d'OpenCode** : v1 ou v2, après l'essai du point 0.
- [ ] **Copilot par la crate du SDK ou par un client maison** : après l'essai du point 0 (lancement du process par Escouade, version épinglée).
- [ ] **« Toujours » chez Copilot** : proposer `approve-permanently` et `approve-for-location`, qui écrivent dans la config de Copilot, ou seulement l'accord pour la session ?
- [ ] **« Toujours » chez Cursor** : proposer `allow-always`, qui écrit dans `~/.cursor/cli-config.json` (la config globale, même quand le projet a la sienne), ou seulement un accord pour la session, tenu par Escouade ?
- [ ] **ACP comme voie commune** : Cursor ne se pilote bien que par ACP, que parlent aussi Copilot CLI (`copilot --acp`, en préversion) et OpenCode (`opencode acp`).
  - Pour : un seul client ACP d'Escouade, plus les extensions de chaque moteur, servirait aux trois, au lieu du SDK de Copilot et du serveur HTTP d'OpenCode. Moins de code, et d'autres moteurs plus tard au même prix.
  - Contre : ACP est le plus petit dénominateur. Quota, crédits, coût et permissions typées passent peut-être seulement par les protocoles propres (à vérifier). Et sa v2 arrive.
  - Pour OpenCode, garder `opencode serve` garde aussi `opencode attach` : sa vue terminal, la plus simple de toutes, se branche sur le serveur de l'agent.
  - Proposé : ACP pour Cursor. Pour Copilot et OpenCode, décider après le point 0, en comparant ce que donne chaque voie.
- [ ] **Mot de l'interface** : « Moteur » (proposé) ? « Agent » est déjà pris.
- [x] **Repli sur un autre moteur** (décidé le 2026-10-14) : oui, et pour un agent en cours, pas seulement pour les nouveaux tickets : voir « Continuer sur un autre moteur quand le quota est épuisé ». Décidé le même jour : l'automatique est une option, désactivée par défaut, et les moteurs de secours se règlent par projet, par les comptes réservés aux projets. Le retour n'est jamais automatique : « Revenir sur Claude » est proposé.
- [ ] **Garde-fou d'OpenCode**, qui n'a pas de quota : un budget par jour en dollars, au-delà duquel le pilote auto se met en pause ?

## 2. Hébergeurs de code : GitHub, GitLab, Bitbucket, Gitea (demandé le 2026-10-14)

But : Escouade suit le travail des agents après le push, jusqu'à la production. Elle suit la pull request (merge request chez GitLab), sa CI, ses relectures et son merge, puis les déploiements. Les fonctionnalités ci-dessous sont des propositions. Trois décisions ont été prises le 2026-10-14 (colonne « En revue », corrections automatiques, tout en 1.8) ; les autres restent dans « Décisions à prendre ».

**Aujourd'hui** :
- « Valider + PR » pousse la branche du ticket et ouvre la PR par `gh pr create` (`tickets.rs`, `open_pr`).
- Sans `gh`, la page de comparaison de GitHub s'ouvre. Sur un autre hébergeur, la branche est seulement poussée.
- Le ticket passe aussitôt « Terminé », et plus rien ne suit la PR.
- Conséquence : un ticket qui l'attend (« Après ») part avant que son code soit dans la branche cible.

### Ce qui sert déjà
- Les comptes des intégrations (`integrations/mod.rs`, `Accounts`, un par service), leurs secrets dans le trousseau (`secrets.rs`), le réglage de proxy, le faux serveur HTTP des tests (`fake.rs`).
- Le compte GitHub d'Issues, par jeton ou par celui de `gh` (`sync.rs`, `gh_token`).
- L'adresse d'un remote GitHub reconnue (`board::github_repo`), la description de PR (`board::pr_body`), la page de comparaison (`board::compare_url`).
- Un agent archivé dont le worktree a disparu se restaure depuis sa branche : de quoi rendre un ticket à son agent après la PR.
- Les tests en échec à la validation renvoient le ticket à son agent avec les 80 dernières lignes : même mécanique pour la CI.
- « Branches mergées » et la base du projet comptent déjà une branche mergée par une PR, une fois récupérée par fetch.

### Connexions
- [ ] **Un compte par instance**, et plusieurs par hébergeur (pro et perso, github.com et l'instance de l'entreprise) :
  - GitHub : github.com, GHE.com et GitHub Enterprise Server (les mêmes hôtes que les comptes Copilot GHE du chapitre 1) ;
  - GitLab : gitlab.com et les instances auto-hébergées ;
  - Bitbucket Cloud. Bitbucket Data Center, dont l'API diffère, attend une autre version ;
  - Gitea et Forgejo, Codeberg compris.
- [ ] **Le projet trouve son compte par son remote** (celui de la branche cible, sinon `origin`) : l'hôte donne le compte.
  - `board::github_repo` devient un analyseur par hébergeur : https, ssh, forme scp, port ssh, sous-groupes de GitLab (`groupe/sous-groupe/projet`), `workspace/dépôt` de Bitbucket.
  - Un hôte inconnu propose « Connecter <hôte> ». Le type d'une instance auto-hébergée se devine en sondant `/api/v4/version` (GitLab) et `/api/v1/version` (Gitea, Forgejo).
- [ ] **Jetons**, gardés dans le trousseau comme aujourd'hui :
  - GitHub : celui de `gh` (`gh auth token --hostname <hôte>`), sinon un jeton à droits fins. Droits à documenter : pull requests et contenu en écriture ; checks, statuts, Actions et déploiements en lecture.
  - GitLab : un jeton personnel (`api`), ou celui de `glab`. Aucune commande `glab auth token` n'est connue : il faudrait lire la sortie de `glab auth status --show-token --hostname <hôte>`.
  - Bitbucket Cloud : un jeton d'API Atlassian à scopes (e-mail du compte + jeton). D'après Atlassian, les mots de passe d'application ne se créent plus depuis le 2025-09-09, et leur retrait était prévu le 2026-07-28.
  - Gitea et Forgejo : un jeton d'accès à scopes.
- [ ] **Une connexion, deux usages** : le compte GitHub d'Issues et celui des PR ne font qu'un (`Accounts.github` devient le compte de github.com).
- [ ] **Réglages › Intégrations** : une section « Hébergeurs de code », avec l'état de chaque compte (connecté, jeton expiré, droits manquants listés) et les projets qui s'en servent.

### Les PR des tickets
- [ ] **Ouvrir la PR par l'API** des quatre hébergeurs : `gh` n'est plus nécessaire, il reste une source de jeton.
  - Brouillon : le champ `draft` de GitHub, de Gitea et de Bitbucket (à vérifier) ; chez GitLab, le titre préfixé `Draft:`.
  - Lien vers l'issue d'origine : `Closes #12` pour GitHub, GitLab et Gitea ; la clé Jira ou Linear dans le titre, comme aujourd'hui.
  - Relecteurs et étiquettes par défaut dans les réglages du Kanban.
  - Sans compte : comme aujourd'hui, mais avec la page de création de chaque hébergeur, remplie (`/compare/` de GitHub et de Gitea, `merge_requests/new?merge_request[source_branch]=…` de GitLab, `pull-requests/new?source=…` de Bitbucket).
- [ ] **Colonne « En revue »**, entre « À tester » et « Terminé » (décidé le 2026-10-14) : le ticket y passe à l'ouverture de sa PR et y reste jusqu'au merge. Elle n'apparaît que si le Kanban ouvre des PR ou qu'un ticket y est.
- [ ] **Carte en revue** : numéro et lien, état (brouillon, ouverte, approuvée, changements demandés), CI (✓, ✗ ou ⏳, et le nombre de jobs), approbations (« 1/2 »), fils de discussion ouverts, conflit avec la cible.
- [ ] **Mergée sur l'hébergeur** :
  - le ticket passe « Terminé » (« ⤵ PR #12 mergée dans main ») ;
  - l'issue externe suit et reçoit son commentaire (`sync.rs`) ;
  - la branche locale part si le réglage le dit, et un fetch met la base à jour ;
  - les tickets qui l'attendent partent alors, sur une cible qui a son code.
- [ ] **Fermée sans merge** : le ticket est bloqué (« PR #12 fermée sans merge »), avec « Remettre à faire » et « Terminer quand même ».
- [ ] **CI en échec** : « L'agent corrige ».
  - Le ticket repart chez son agent, restauré depuis sa branche si son worktree n'est plus là.
  - L'agent reçoit le nom des jobs en échec et la fin de leurs journaux (80 lignes par job, comme les tests).
  - Il pousse, la PR se met à jour, et le ticket revient « En revue ».
  - Avec le pilote auto, ça part seul, dans la limite des boucles du ticket (décidé le 2026-10-14). Sans lui, un bouton sur la carte.
- [ ] **Changements demandés** : « L'agent traite les retours ».
  - Escouade le propose dès que des retours arrivent (décidé le 2026-10-14) : une notification, et sur la carte, le nombre de retours avec « L'agent corrige les retours ». L'agent ne part qu'après ce clic, même avec le pilote auto.
  - Il reçoit les commentaires non résolus arrivés depuis le dernier push : fichier, ligne, extrait du diff, auteur, texte.
  - Il pousse ses corrections.
  - Répondre sous chaque fil (« Corrigé dans abc1234 ») est un réglage, désactivé par défaut. Escouade ne résout jamais un fil seule.
- [ ] **Conflit avec la cible** : « L'agent met à jour la branche », par un merge de la cible dans la branche, avec le traitement des conflits d'aujourd'hui (`on_conflict`). Le rebase, et son push forcé (`--force-with-lease`), seulement en réglage.
- [ ] **Merger depuis Escouade** : « Merger » quand la PR est approuvée et verte, avec la stratégie du Kanban (parmi celles que le dépôt autorise). « Merger quand c'est vert » passe par l'auto-merge de l'hébergeur :
  - GitHub : `enablePullRequestAutoMerge` (GraphQL), si le dépôt l'autorise ;
  - GitLab : `merge_when_pipeline_succeeds` (ou `auto_merge`, selon la version) ;
  - Gitea : `merge_when_checks_succeed`, seulement sur une branche protégée ;
  - Bitbucket : aucun appel d'API connu. Son API merge tout de suite (demandes ouvertes BCLOUD-22055 et BCLOUD-22062). Escouade attend la CI puis merge elle-même tant qu'elle tourne, et le dit.
- [ ] **Pilote auto** : un ticket en revue ne compte dans les agents en parallèle que quand son agent travaille.

### Les PR hors tickets
- [ ] **Agent ordinaire** : « Ouvrir une PR » sur sa branche (en-tête de la conversation, clic droit sur sa carte, sélecteur de branches).
  - Push, puis titre et description proposés par Haiku, comme le message de commit ; brouillon au choix.
  - Sa carte montre ensuite l'état de la PR et de sa CI.
  - « L'agent corrige » et « L'agent traite les retours » s'y font en un clic, jamais seuls.
- [ ] **Vue « Pull requests » du projet**, à côté du Kanban : les PR ouvertes du dépôt (les miennes, celles où on me demande une relecture, toutes), avec CI, relectures, conflit et âge. Actions :
  - ouvrir dans le navigateur ;
  - « Lancer un agent sur cette PR » : un worktree sur sa branche, comme « Reprendre une branche existante… ». Une PR venue d'un fork se récupère par `refs/pull/<n>/head` (GitHub, Gitea) ou `refs/merge-requests/<n>/head` (GitLab) ; pour Bitbucket, à voir ;
  - « Faire relire par un agent » (ci-dessous).
- [ ] **Relecture par un agent** : un agent lit le diff d'une PR et rédige une relecture, avec des commentaires par fichier et par ligne.
  - L'utilisateur les garde, les corrige ou les écarte un par un dans Escouade, puis publie : review de GitHub et de Gitea, discussions de GitLab, commentaires de ligne de Bitbucket.
  - Rien ne part sans lui.

### La CI
- [ ] **La branche cible** : une pastille dans l'en-tête du projet (« main ✗ depuis 2 h »). Réglage du pilote auto « Pause quand la CI de la cible est rouge », désactivé par défaut.
- [ ] **Les branches poussées sans PR** (« Valider et pousser », agents) : la même pastille sur la carte.
- [ ] **Les sources** : checks et statuts de GitHub, pipelines de GitLab, statuts de commit de Bitbucket, statuts combinés de Gitea. Une CI tierce (Jenkins, CircleCI, Buildkite…) ne donne qu'un statut et une adresse, pas de journal : la carte le dit.
- [ ] **Le journal d'un job**, en lecture seule dans Escouade comme un terminal, avec « Envoyer à un agent ».
- [ ] **« Relancer les jobs en échec »**, quand l'hébergeur le permet : GitHub Actions et GitLab ; Bitbucket Pipelines et Gitea Actions, à vérifier.

### Les déploiements
- [ ] **Après le merge**, Escouade suit les déploiements par environnement : déploiements et statuts de GitHub, environnements de GitLab, déploiements de Bitbucket Pipelines. Gitea n'a pas d'API de déploiements connue : seulement la CI de la cible.
- [ ] **Un ticket est dans un environnement** dès qu'un déploiement réussi y porte un commit qui contient son merge (`git merge-base --is-ancestor` après un fetch, sans appel d'API de plus). Sa carte « Terminé » le dit (« staging ✓ · production ⏳ »), avec l'adresse de l'environnement.
- [ ] **Vue « Environnements »** du projet : ce qui tourne où (commit, date, auteur), les tickets livrés dans chacun, et ceux mergés qui attendent la production (« 3 tickets pas encore en production »).
- [ ] **Aperçus** : Vercel, Netlify, Cloudflare Pages… posent sur la PR un déploiement ou un statut avec une adresse. « Ouvrir l'aperçu » sur la carte en revue, à côté de « ▶ Tester ».
- [ ] **Échec d'un déploiement** : une notification et une pastille sur l'environnement, avec le lien du job.

### Notifications, vue d'ensemble, serveur MCP
- [ ] **Notifications**, réglables une à une comme les autres : CI en échec, changements demandés, PR approuvée, PR mergée, conflit, relecture demandée à l'utilisateur, déploiement en production réussi ou en échec.
- [ ] **Vue d'ensemble** : un groupe « Pull requests » (à merger, à corriger, relectures demandées).
- [ ] **Serveur MCP** : `list_pull_requests` et `get_pull_request` (état, CI, relectures, fils ouverts, déploiements), et la PR du ticket dans `get_ticket`. L'agent d'un ticket peut y relire les retours lui-même. Commenter ou merger par le MCP : pas en 1.8.

### Rafraîchissement et limites des API
- [ ] **Pas de webhook** : l'app n'a pas d'adresse publique. Escouade interroge les hébergeurs, et seulement sur ce qu'elle suit : les PR des tickets en revue et des agents, la CI de la cible, les déploiements récents.
  - Toutes les minutes quand la fenêtre est au premier plan, toutes les 5 minutes sinon ;
  - tout de suite après un push d'Escouade et quand la fenêtre revient ;
  - un bouton « Rafraîchir ».
- [ ] **Économiser les quotas d'API** :
  - GitHub : toutes les PR suivies d'un dépôt en une requête GraphQL, ou des requêtes REST conditionnelles (`If-None-Match` : une réponse 304 ne compte pas dans la limite) ;
  - relever les limites de chaque hébergeur dans les essais (Bitbucket Cloud aurait la plus basse, à vérifier) ;
  - ralentir à l'approche de la limite (`X-RateLimit-*`).
- [ ] **Hébergeur injoignable ou jeton expiré** : la carte le dit (« GitLab injoignable depuis 10 min »). Une erreur ne déplace jamais un ticket.

### À vérifier d'abord (essais jetables, sur de vrais dépôts)
- [ ] **Un parcours enregistré par hébergeur**, rejoué dans les tests (`tests/fixtures/forges/`) : créer une PR ; lire son état, ses checks, ses relectures et ses commentaires de ligne ; merger ; auto-merge ; lire le journal d'un job. Routes des journaux : `/actions/jobs/{id}/logs` chez GitHub, `/jobs/:id/trace` chez GitLab, journal d'une étape de Bitbucket Pipelines, Actions de Gitea (routes à relever, versions récentes seulement).
- [ ] **Dépôts de test** : github.com, gitlab.com, Bitbucket Cloud, et Codeberg (Forgejo) ou un Gitea en local par Docker. Chacun a une CI qui échoue puis passe, et un déploiement.
- [ ] **`glab`** : lire son jeton par `glab auth status --show-token`, ou dans son fichier de config.
- [ ] **Bitbucket** : le jeton d'API Atlassian (e-mail + jeton, scopes), les PR en brouillon, la relance d'un pipeline, les limites horaires.
- [ ] **Gitea et Forgejo** : les écarts entre les deux API sur ce dont Escouade a besoin (Forgejo diverge depuis 2024), et la plus ancienne version de chacun à prendre en charge.
- [ ] **Forks** : un projet dont `origin` est un fork et `upstream` le dépôt d'origine. Sur quel dépôt ouvrir la PR ? Aujourd'hui, `gh pr create --repo` (`tickets.rs`).

### Tests
- [ ] **Faux hébergeurs** sur le modèle de `fake.rs`, nourris des réponses enregistrées pendant les essais.
- [ ] **Tests unitaires** :
  - l'analyse des adresses de remote (https, ssh, scp, sous-groupes, ports) ;
  - l'état d'une PR lu dans chaque API (brouillon, approuvée, changements demandés, CI rouge, en cours ou verte, conflit, mergée, fermée) ;
  - les commentaires arrivés depuis le dernier push ;
  - l'appartenance d'un ticket à un déploiement.
- [ ] **Cycle d'un ticket** : PR ouverte, CI rouge, l'agent corrige, CI verte, PR approuvée puis mergée, ticket « Terminé », puis déployé. Le pilote auto, et « Après » qui attend le merge.
- [ ] **e2e** : la carte en revue et ses actions, la vue « Pull requests ».
- [ ] **Avant la release**, un essai réel par hébergeur, sur les dépôts de test.

### Docs
- [ ] SPEC (section « Hébergeurs de code »), README, CHANGELOG.
- [ ] Site, en français et en anglais : pages de doc des intégrations et du Kanban, captures.
- [ ] Statistiques d'usage : les nouveaux événements (PR ouverte, PR mergée, correction de CI, relecture par un agent), sans nom ni adresse. Les ajouter à la page Confidentialité du site, qui liste chaque événement.
- [ ] Rien ne part vers les serveurs d'Escouade : les jetons restent sur le poste, et les appels vont de l'app à l'hébergeur. Le dire dans la page Confidentialité.

### Décisions à prendre
- [x] **Colonne « En revue »** (décidé le 2026-10-14) : une colonne entre « À tester » et « Terminé », plutôt qu'un ticket « Terminé » avec l'état de sa PR. Elle répare au passage « Après » : aujourd'hui, un ticket part avant que le code qu'il attend soit mergé.
- [x] **Correction automatique par le pilote auto** (décidé le 2026-10-14) :
  - CI en échec : corrigée seule, dans la limite des boucles ;
  - retours de relecture : Escouade propose de les faire corriger par l'agent dès qu'ils arrivent, et l'agent ne part qu'avec l'accord de l'utilisateur (un relecteur humain peut attendre une discussion plutôt qu'un correctif).
- [ ] **Ce qu'Escouade publie au nom de l'utilisateur** (réponses sous les fils, relecture rédigée par un agent) : proposé, rien sans un réglage ou une validation à la main.
- [ ] **Mot de l'interface** : « PR » partout, ou « MR » sur GitLab ? Proposé : le mot de l'hébergeur.
- [ ] **Worktree pendant la revue** : le garder jusqu'au merge (corrections plus rapides, mais disque et `node_modules`), ou le supprimer comme aujourd'hui et le recréer au besoin depuis la branche ? Proposé : comme aujourd'hui.
- [x] **Ce qui entre dans la 1.8** (décidé le 2026-10-14) : tout le chapitre 2, pour l'instant. La découpe de « Ampleur » reste le repli si la 1.8 s'allonge.

### Plus tard
- Import des issues de GitLab et de Gitea dans le Kanban, comme GitHub Issues. Peu coûteux une fois les comptes faits : à remonter en 1.8 si le temps le permet.
- Approuver depuis Escouade un déploiement en attente (environnements protégés de GitHub et de GitLab).
- « Inclus dans v1.8.0 » sur un ticket, d'après les releases de l'hébergeur.
- Bitbucket Data Center, Azure DevOps.

## Ordre proposé

1. Essais du point 0 sur les quatre moteurs. Ils décident de la ligne d'OpenCode, de la voie de Copilot (crate, client maison ou ACP), d'ACP comme voie commune, du commit sous les bacs à sable de Codex, de Copilot et de Cursor, et de la façon de passer le serveur MCP.
2. Refonte multi-moteurs (ses 12 lots), avec Claude seul, sans changement visible.
3. Codex, en deux temps :
   - Lancement, conversation, permissions, questions, modèles et modes, plan.
   - Comptes, quota, stats, serveur MCP.
4. Copilot, dans le même ordre que Codex, puis GitHub Enterprise et OpenRouter.
5. Cursor, par le moteur ACP générique, dans le même ordre.
6. OpenCode, dans le même ordre (par le moteur ACP si c'est décidé).
7. Vues terminal : OpenCode (par `opencode attach`), Codex, Copilot, puis Cursor en suivi réduit.
8. Déclaration du serveur MCP dans les configs des quatre moteurs, puis docs et site, puis un essai réel de chaque moteur.

Le chapitre 2 ne dépend pas du chapitre 1. Il peut passer avant les moteurs ou se faire en parallèle. Son ordre :
1. Essais sur les dépôts de test.
2. Connexions et clients d'API des quatre hébergeurs.
3. PR des tickets : ouverture par l'API, colonne « En revue », merge sur l'hébergeur.
4. CI : pastilles, journaux, « L'agent corrige », puis « L'agent traite les retours ».
5. PR hors tickets, vue « Pull requests », relecture par un agent.
6. Déploiements et vue « Environnements ».
7. Notifications, vue d'ensemble, serveur MCP, puis docs et site.
