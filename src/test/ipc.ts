// Test utilities: a fake Tauri backend recording every command.

import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { buffers } from '../lib/editor/buffers.svelte';
import { navHistory } from '../lib/editor/history';
import { recentFiles } from '../lib/editor/quick-open';
import { fileSearches } from '../lib/editor/search.svelte';
import { gitSync } from '../lib/git-sync.svelte';
import { mcp } from '../lib/mcp.svelte';
import { trees } from '../lib/editor/trees.svelte';
import { app } from '../lib/state.svelte';
import type { Agent, BoardSettings, BranchInfo, GitInfo, Project, ReviewItem, Settings, Ticket } from '../lib/types';

export interface Call {
  cmd: string;
  args: Record<string, any>;
}

/** What a command answers when a test gives it no handler: nothing (null), except the lists the window reads on its own. */
const ANSWERS: Record<string, unknown> = { closed_projects: [] };

export function fakeBackend(handlers: Record<string, (args: any) => unknown> = {}) {
  const calls: Call[] = [];
  mockWindows('main');
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args: (args ?? {}) as Record<string, any> });
      const h = handlers[cmd];
      return h ? h(args) : (ANSWERS[cmd] ?? null);
    },
    { shouldMockEvents: true },
  );
  return {
    calls,
    called: (cmd: string) => calls.filter((c) => c.cmd === cmd),
  };
}

export const SETTINGS: Settings = {
  claudePath: '',
  defaultModel: 'sonnet',
  defaultEffort: 'medium',
  defaultMode: 'auto',
  sound: true,
  osNotifications: true,
  notifyFor: { questions: true, done: true, errors: true, tickets: true },
  idleStopMinutes: 30,
  attachMaxMb: 0,
  attachWarnMb: 200,
  attachOrigin: true,
  attachBigText: 'path',
  attachKeepDays: 0,
  pwshPath: '',
  bashPath: '',
  wslDistro: '',
  proxyUrl: '',
  noProxy: 'localhost',
  proxyTerminals: false,
  insecureTls: false,
  autoResume: true,
  switchOnLimit: false,
  quotaPause: 100,
  autoUpdate: true,
  telemetry: false,
  integrations: {
    syncStates: true,
    loopComments: false,
    extractCriteria: true,
    autoImport: false,
    importLabel: 'claude-ready',
    importEvery: 15,
  },
  language: 'system',
  claudeLanguage: 'ui',
  accounts: [{ id: 'principal', name: 'Principal', configDir: '', claudePath: '', active: true }],
  mcpEnabled: false,
  mcpPort: 0,
  todoTools: true,
  terminalView: false,
  remoteControlAll: false,
};

export function project(over: Partial<Project> = {}): Project {
  return {
    id: 'p1',
    name: 'demo-api',
    path: 'C:\\code\\demo-api',
    color: 'oklch(0.72 0.12 48)',
    worktreePerAgent: false,
    createdAt: 1,
    runCommands: [],
    board: board(),
    worktreeCopy: ['.env*'],
    worktreeSetup: [],
    worktreeTeardown: [],
    integrations: { links: [], comments: ['review', 'done'] },
    commitMode: 'agent',
    account: '',
    worktreeDir: '',
    agentBranchPrefix: 'escouade/',
    ticketBranchPrefix: 'ticket/',
    remoteControl: false,
    ...over,
  };
}

export function agent(over: Partial<Agent> = {}): Agent {
  return {
    id: 'a1',
    projectId: 'p1',
    name: 'refacto-auth',
    named: true,
    model: 'opus',
    effort: 'high',
    mode: 'auto',
    sessionId: null,
    lastEntry: null,
    forkOf: null,
    forkAt: null,
    cwd: 'C:\\code\\demo-api',
    worktree: null,
    createdAt: 1,
    archived: false,
    status: 'done',
    tokens: 0,
    cost: 0,
    activeMs: 0,
    touchedFiles: [],
    lastActivity: 1,
    prompts: 0,
    activeSince: null,
    alive: true,
    escouadeNext: false,
    pending: [],
    requests: [],
    contextTokens: 0,
    contextWindow: 0,
    liveTokens: 0,
    liveCost: 0,
    remoteControl: false,
    remoteSource: null,
    remoteOff: false,
    interfaceRemote: null,
    remoteSession: null,
    remoteUrl: null,
    remoteState: null,
    resumeAt: null,
    ticketId: null,
    progressLine: null,
    reviews: [],
    appendPrompt: null,
    portBase: null,
    recipe: null,
    approvedRecipe: null,
    approvedIsola: null,
    account: 'principal',
    movedFrom: null,
    activity: null,
    setup: null,
    terminal: false,
    terminalId: null,
    tuiExit: null,
    isola: false,
    ...over,
  };
}

export function reviewItem(over: Partial<ReviewItem> = {}): ReviewItem {
  return {
    kind: 'review',
    id: 'rv1',
    ts: 1,
    statut: 'attente',
    reponses: null,
    contenu: {
      titre: 'Plan du parseur',
      sur: 'Plan',
      chapo: 'Trois lots.',
      puces: [
        { texte: 'Migration risquée', ton: 'alerte' },
        { texte: 'Tests verts', ton: null },
      ],
      sections: [{ titre: 'Lots', ton: null, items: ['Lot 1'], paragraphes: ['Le lot 1 crée le lexer.'] }],
      titreQuestions: null,
      questions: [
        {
          id: 'q1',
          titre: 'Exécution ?',
          contexte: 'Deux façons de faire.',
          placeholder: null,
          choix: [
            { valeur: 'sub', libelle: 'Sous-agents', detail: null, reco: true },
            { valeur: 'inline', libelle: 'Ici', detail: 'Plus lent', reco: false },
          ],
        },
        { id: 'q2', titre: 'Nom du module ?', contexte: null, placeholder: 'parser', choix: [] },
      ],
    },
    ...over,
  };
}

export function board(over: Partial<BoardSettings> = {}): BoardSettings {
  return {
    action: 'merge',
    target: '',
    strategy: 'squash',
    draft: false,
    testsFirst: false,
    testCommand: '',
    cleanup: true,
    conventional: true,
    conflict: 'ask',
    maxParallel: 2,
    model: '',
    effort: '',
    mode: '',
    autopilot: true,
    prefix: '',
    nextNumber: 1,
    ...over,
  };
}

export function ticket(over: Partial<Ticket> = {}): Ticket {
  return {
    id: 't1',
    projectId: 'p1',
    key: 'DEM-1',
    title: 'Ajouter le fichier',
    description: '',
    criteria: [
      { text: 'Le fichier existe', ok: false, note: '' },
      { text: 'Tests verts', ok: false, note: '' },
    ],
    progress: [],
    maxLoops: 5,
    column: 'todo',
    rank: 1,
    after: [],
    agentId: null,
    iteration: 0,
    loops: 0,
    partial: false,
    blocked: null,
    conflict: false,
    step: null,
    outcome: null,
    outcomeClaude: null,
    outcomeUrl: null,
    outcomeCommit: null,
    forced: false,
    reminded: false,
    cost: 0,
    createdAt: 1,
    startedAt: null,
    reviewAt: null,
    doneAt: null,
    external: null,
    branch: '',
    model: '',
    effort: '',
    images: [],
    files: [],
    ...over,
  };
}

export function gitInfo(over: Partial<GitInfo> = {}): GitInfo {
  return {
    isRepo: true,
    branch: 'main',
    upstream: null,
    upstreamGone: false,
    ahead: 0,
    behind: 0,
    hasRemote: false,
    lastFetch: null,
    modified: 0,
    added: 0,
    deleted: 0,
    total: 0,
    agents: {},
    ...over,
  };
}

/** A local branch of the project's repository, as `branchList` gives it. */
export function branchInfo(over: Partial<BranchInfo> = {}): BranchInfo {
  return {
    name: 'main',
    remote: false,
    current: false,
    upstream: null,
    upstreamGone: false,
    ahead: 0,
    behind: 0,
    worktree: null,
    trackedBy: null,
    agent: null,
    lastCommitAt: 1,
    ...over,
  };
}

/** Puts the app singleton back into a known state. */
export function resetApp(over: { projects?: Project[]; agents?: Agent[]; tickets?: Ticket[] } = {}) {
  const projects = over.projects ?? [project()];
  app.projects = projects;
  app.agents = Object.fromEntries((over.agents ?? []).map((a) => [a.id, a]));
  app.attention = {};
  app.tickets = Object.fromEntries((over.tickets ?? []).map((t) => [t.id, t]));
  app.board = {};
  app.boardIssues = {};
  app.setupOutput = {};
  app.relaunched = {};
  app.autopilotPause = null;
  app.claudePathFound = true;
  app.projectPauses = {};
  app.editor = {};
  app.ui = { activeProject: projects[0]?.id ?? null, view: 'project', selectedAgent: {} };
  app.settings = { ...SETTINGS };
  app.usage = { fiveHour: null, sevenDay: null, todayCost: 0, updatedAt: 0, accounts: [], current: 'principal' };
  app.resources = { instances: 0, memory: 0, cpu: 0, agents: [] };
  app.git = {};
  gitSync.running = {};
  app.shells = [];
  app.terminals = [];
  app.exitedTerms = {};
  app.launches = {};
  app.selectedLaunch = {};
  app.selectedTerm = {};
  app.filesOpen = false;
  app.planOpen = {};
  app.subView = {};
  app.subDrafts = {};
  app.subFocus = { from: null, back: null };
  app.subsOpen = {};
  app.taskView = {};
  app.bgOpen = {};
  app.filesScope = 'agent';
  app.panelTab = 'files';
  app.reviewShown = {};
  app.diffSplit = false;
  app.modal = null;
  app.toasts = [];
  app.update = null;
  app.restartAt = null;
  app.failedUpdate = null;
  app.models = [];
  app.accounts = [];
  // French all along, as the tests (the language itself is put back by the setup).
  app.lang = { ui: 'fr', system: 'fr', claude: 'fr' };
  app.ready = true;
  buffers.reset();
  mcp.reset();
  trees.reset();
  navHistory.reset();
  recentFiles.reset();
  fileSearches.reset();
}
