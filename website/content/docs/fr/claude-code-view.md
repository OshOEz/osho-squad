# La vue Claude Code

La vue Claude Code fait tourner un agent dans l’interface de Claude Code elle-même, dans un terminal, et le ramène dans la conversation d’Escouade sans perdre la session. Cette page explique comment changer de vue, ce qu’Escouade continue de suivre et ce qui change.

## À quoi ça sert

La conversation d’Escouade montre l’essentiel du travail d’un agent, mais pas tout ce que propose Claude Code. Dans sa propre interface, tu retrouves tout : les commandes `/`, les raccourcis, les dialogues, `/model`, `Maj+Tab` pour changer de mode… C’est utile quand tu veux travailler comme tu le ferais dans un terminal, tout en gardant l’agent dans la liste d’Escouade avec ses statuts, ses notifications et son coût.

## Passer d’une vue à l’autre

Dans l’en-tête de la conversation d’un agent, le bouton « Vue Claude Code » (`>_`) lance l’interface de Claude Code dans un terminal qui prend la place de la conversation. Le bouton devient « Vue Escouade » (`◧`) : un clic te ramène à la conversation. La session est la même dans les deux sens : Escouade arrête le processus de l’une des vues et reprend la session dans l’autre (`--resume`), donc l’agent n’oublie rien. Le choix est gardé pour chaque agent. L’interface démarre quand tu affiches l’agent (le terminal dit alors « Démarrage de Claude Code… »), y compris après un redémarrage d’Escouade, et reprend sa session.

Le passage à la vue Claude Code est refusé dans trois cas, avec un message ; le retour à la vue Escouade, lui, n’est refusé dans aucun :

- **L’agent travaille encore** : change de vue quand son tour est fini.
- **Le remote control est activé** : coupe-le d’abord. Dans l’interface de Claude Code, c’est sa commande `/remote-control` qui le fait (voir [le remote control](remote-control)).
- **L’agent travaille sur un ticket** : un agent de ticket reste dans la vue d’Escouade, que son tableau pilote (voir [le Kanban](board)). Le bouton n’existe pas pour lui, ni pour un agent archivé.

Si l’interface s’arrête (tu as tapé `/exit`, ou le processus a été tué), le terminal affiche « Claude Code s’est arrêté. » et un bouton « Relancer » ; si elle ne démarre pas, « Claude Code n’a pas démarré : » suivi de l’erreur, avec le même bouton. Archiver ou supprimer l’agent ferme aussi son terminal.

## Ce qu’Escouade continue de suivre

L’interface de Claude Code est un terminal, mais Escouade sait toujours ce que fait l’agent. Il le lit de deux façons : les hooks de Claude Code, que l’interface appelle sur ta machine (`127.0.0.1`, avec un jeton propre à chaque interface), et le transcript de la session, que l’app relit en continu. Tes propres hooks restent actifs.

Grâce à cela, un agent en vue Claude Code garde :

- **sa pastille et ses statuts** : « En cours », « Question » quand il attend une autorisation ou une réponse, « Terminé » à la fin d’un tour ;
- **ses notifications** : le clignotement, le carillon et la notification système quand il attend une autorisation ou une réponse, puis quand il termine (voir [les notifications](notifications)) ;
- **sa conversation** : ce qui se passe dans l’interface s’écrit aussi dans la conversation d’Escouade, que tu retrouves en revenant à la « Vue Escouade » ;
- **la recherche** (`Ctrl+K`, voir [suivre ses agents](overview#rechercher-dans-les-conversations)) ;
- **le bandeau « Plan »** (voir [l’avancée d’un plan](plan)) ;
- **ses statistiques et son coût** (voir [les statistiques](stats)). L’interface ne donne pas le coût de chaque tour : Escouade l’estime d’après les tarifs publics.

Quand l’agent attend une réponse, c’est dans le terminal que tu la donnes : la [vue d’ensemble](overview) le rappelle (« Attend ta réponse dans l’interface de Claude Code ») et son bouton « Répondre » ouvre l’agent.

## Écrire à un agent dans cette vue

Le champ de saisie d’Escouade n’est pas affiché : tu écris dans le terminal, comme dans Claude Code. Pour joindre une image, par exemple une capture d’écran copiée, colle-la dans le terminal (`Ctrl+V`, `Cmd+V` sous macOS) : Escouade l’enregistre dans un fichier et tape son chemin, que Claude Code prend pour l’image (« [Image #1] »). Du texte collé est tapé tel quel.

Un message envoyé à l’agent d’ailleurs, par exemple par le [serveur MCP](mcp), est tapé dans son interface. Il est refusé tant que l’agent attend une réponse dans son terminal (« L’agent attend une réponse dans son terminal : réponds-y d’abord. »). Une interruption demandée depuis Escouade devient la touche `Échap` dans l’interface.

## Ouvrir les nouveaux agents dans cette vue

Dans Réglages › Claude Code, groupe « Nouveaux agents », l’interrupteur « Ouvrir les nouveaux agents dans l’interface de Claude Code » (désactivé par défaut) fait démarrer chaque nouvel agent dans cette vue. Chaque agent peut ensuite changer de vue depuis son en-tête. Les agents de ticket restent toujours dans la vue d’Escouade. Voir [les réglages de Claude Code](settings-claude-code).

## La confiance du dossier

La première fois que l’interface de Claude Code se lance dans un dossier, elle demande si tu lui fais confiance. Escouade ne répond jamais à ta place. Les worktrees créés dans le dossier des worktrees par défaut du projet (`.claude/worktrees`) héritent de la réponse donnée pour le projet. Un « Dossier des worktrees » hors du projet n’en hérite pas : l’interface pose la question au premier lancement de chaque worktree (voir [les worktrees](worktrees)).

## Sous Windows avec WSL

Quand Claude Code est installé dans WSL, Escouade le lance par `wsl.exe` (voir [les réglages de Claude Code](settings-claude-code)). Le suivi en direct de la vue Claude Code demande alors le réseau miroir de WSL : sans lui, les hooks de l’interface n’atteignent pas l’app. Les réglages le signalent sous le chemin de `claude` (« Le réseau de WSL est en NAT… »). Pour passer en mode miroir, ajoute ceci à `%UserProfile%\.wslconfig`, puis lance `wsl --shutdown` :

```ini
[wsl2]
networkingMode=mirrored
```
