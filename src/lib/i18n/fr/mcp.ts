import type { Tree } from '../types';

// The MCP server: its switch, what it was asked, and how its tools read in the conversation.
export default {
  // The « Claude Code » tab of the settings: the server declared in Claude Code, and what it was asked.
  section: {
    title: 'Escouade dans Claude',
    /** The one switch (ESC-91): Claude Code outside Escouade and every agent of Escouade, each with a token of its own. */
    enable: 'Claude et les agents peuvent piloter Escouade',
    enableDesc:
      'Claude Code hors d’Escouade (un terminal, un autre outil) et chaque agent d’Escouade lisent tes projets, agents et tickets, et peuvent créer des tickets ou lancer des agents, ce qui dépense du quota, valider des tickets (tests, fusion, push, pull request) ou en supprimer. Un agent déjà lancé reçoit les outils au prochain démarrage de son process.',
    /** `{accounts}`: the accounts it is declared in, each as « compte … » (below), joined by commas. */
    declared: 'Déclaré dans Claude · {accounts}',
    account: 'compte {name}',
    working: 'Déclaration dans Claude…',
    notDeclared: 'Pas déclaré pour le compte {name} : {error}',
    serverFailed: 'Le serveur n’a pas démarré : {error}',
    /** The agents whose process started before the switch was on (« Outils d’Escouade au prochain démarrage » on their conversation). */
    nextStart: {
      one: '{count} agent déjà lancé aura les outils au prochain démarrage de son process : « Relancer » sur sa conversation.',
      other: '{count} agents déjà lancés auront les outils au prochain démarrage de leur process : « Relancer » sur leur conversation.',
    },
    /** `{shell}`: the shell the command is written for, PowerShell or sh. */
    command: 'À lancer à la main dans {shell} :',
    copy: 'Copier la commande',
    copied: 'Commande copiée',
    copyFailed: 'Copie impossible : {error}',
    note: 'Le jeton est écrit en clair dans la config de Claude Code (~/.claude.json pour le compte Principal). Désactiver change le jeton.',
  },
  activity: {
    title: 'Activité MCP',
    list: 'Appels au serveur MCP, les plus récents en tête',
    empty: 'Aucun appel pour l’instant.',
    /** The result of a call (a request refused before it reached a tool has none). */
    outcome: { ok: 'Fait', refused: 'Refusé', error: 'Erreur' },
  },
  tools: {
    /** The badge of one of its tools: `tool` is its name in words (below), or as Claude names it. */
    named: 'Escouade · {tool}',
    listProjects: 'Lister les projets',
    listAgents: 'Lister les agents',
    listTickets: 'Lister les tickets',
    getTicket: 'Lire un ticket',
    getTicketImage: 'Lire une image d’un ticket',
    getUsage: 'Lire le quota',
    getBoard: 'Lire les réglages du Kanban',
    setBoard: 'Régler le Kanban',
    getAgentSummary: 'Résumer un agent',
    listRequests: 'Lister les demandes en attente',
    createTicket: 'Créer un ticket',
    updateTicket: 'Modifier un ticket',
    moveTicket: 'Déplacer un ticket',
    startTicket: 'Lancer un ticket',
    createAgent: 'Lancer un agent',
    sendMessage: 'Envoyer un message',
    stopAgent: 'Arrêter un agent',
    answerPermission: 'Répondre à une demande d’autorisation',
    answerQuestion: 'Répondre à une question',
    reportProgress: 'Donner son avancement',
    splitTicket: 'Découper son ticket',
    showReview: 'Afficher une revue',
    approveTicket: 'Valider un ticket',
    rejectTicket: 'Renvoyer un ticket',
    resumeTicket: 'Reprendre un ticket',
    resolveConflict: 'Résoudre un conflit',
    dismissBlock: 'Écarter un blocage',
    deleteTicket: 'Supprimer un ticket',
    waitTicket: 'Attendre des tickets',
  },
  progress: {
    /** What a status line an agent reported (`report_progress`) is, for assistive technologies; the line is the agent's own words. */
    label: 'Ce que l’agent dit faire',
  },
  permission: {
    /** What a permission card lists: each argument the tool is given, by its name. */
    args: 'Ce que Claude donne à l’outil',
    argName: '{name} :',
    /** A value shown cut: how many characters are not, and where the whole request is. */
    cut: {
      one: '{n} caractère de plus n’est pas montré : lis la demande entière dans la conversation.',
      other: '{n} caractères de plus ne sont pas montrés : lis la demande entière dans la conversation.',
    },
  },
} as const satisfies Tree;
