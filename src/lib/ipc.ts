import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  Account,
  AccountForm,
  AccountStatus,
  AccountView,
  Agent,
  AttachedFile,
  Attachment,
  BoardSettings,
  BranchInfo,
  CheckedUpdate,
  ClaudeLocated,
  ClosedProject,
  CommitScope,
  Container,
  ConvItem,
  ConvSearchResult,
  DownloadedUpdate,
  ExternalIssue,
  FileBase,
  FileChange,
  FileText,
  FileTree,
  FolderInfo,
  GitLog,
  InitialState,
  Integration,
  McpActivityEntry,
  McpDeclaration,
  McpStatus,
  OwnedDiff,
  IsolaService,
  IssuePage,
  Project,
  RunSuggestion,
  SearchQuery,
  SearchResult,
  Service,
  Settings,
  ShareMode,
  ShellInfo,
  SlashCommand,
  StatesView,
  StatsView,
  SubagentRead,
  TaskOutput,
  ReviewAnswers,
  TermInfo,
  TestRecipe,
  Ticket,
  TicketDraft,
  TicketImageData,
  TranscriptCursor,
  UiEvent,
  UiState,
  WorktreeSuggestion,
} from './types';

/**
 * What a terminal tells the app's log (see pty.rs): which renderer draws it, or that an echo was slow. Durations only, in
 * ms (fractional) from `performance.now()`: the window's clock is never compared with the backend's.
 */
export type TermReport =
  /** `detail`: the GPU's name for WebGL (a software one, SwiftShader, would say so), or why WebGL is not used. */
  | { kind: 'renderer'; renderer: 'webgl' | 'dom'; detail?: string }
  /**
   * The write numbered `seq` by `term_write` (`bytes` of UTF-8) took `sentToReceived` from leaving the window to the output
   * that answers it, and `receivedToPainted` from that output to its paint. `sentToAcked`: from the call of `term_write` to
   * the resolution of its promise (the way to the backend and back). `outputEnd`: the bytes of output the terminal had
   * received, the echo's chunk included, when that chunk came in: every read of the backend is one message of the same
   * bytes, in order, so this tells which read was the echo.
   */
  | {
      kind: 'slowEcho';
      seq: number;
      bytes: number;
      sentToReceived: number;
      receivedToPainted: number;
      sentToAcked?: number;
      outputEnd?: number;
    };

export const api = {
  subscribe: (onEvent: (e: UiEvent) => void) => {
    const channel = new Channel<UiEvent>();
    channel.onmessage = onEvent;
    return invoke<InitialState>('subscribe', { channel });
  },
  setUi: (ui: UiState) => invoke<void>('set_ui', { ui }),
  mcpStatus: () => invoke<McpStatus>('mcp_status'),
  /** The MCP server's activity log, oldest first (the last 200). */
  mcpActivity: () => invoke<McpActivityEntry[]>('mcp_activity'),
  mcpClearActivity: () => invoke<void>('mcp_clear_activity'),
  /** « Claude et les agents peuvent piloter Escouade »: saved at once; the declaration in each account follows (`mcpDeclared`). */
  mcpSetEnabled: (enabled: boolean) => invoke<void>('mcp_set_enabled', { enabled }),
  /** Where the server stands in each active account's Claude Code (the command to run by hand has its token hidden). */
  mcpDeclareStatus: () => invoke<McpDeclaration[]>('mcp_declare_status'),
  /** The command that declares the server by hand in the account, with the real token: asked for when it is copied. */
  mcpManualCommand: (account: string) => invoke<string>('mcp_manual_command', { account }),
  /** The answer to « Statistiques d’usage », saved at once. */
  telemetrySet: (consent: boolean) => invoke<void>('telemetry_set', { consent }),
  /** The settings, but for the Claude accounts (their tab changes them with the `account*` calls below). */
  saveSettings: (settings: Settings) => invoke<ShellInfo[]>('save_settings', { settings }),
  /** Where the `claude` of `path` is (blank: the one found by itself), `wslDistro` the distribution for one of WSL. */
  locateClaude: (path: string, wslDistro: string) => invoke<ClaudeLocated>('claude_locate', { path, wslDistro }),
  /** Whether Claude Code is found for the account new agents go to, and by the settings' own path (as at the start). */
  claudeFound: () => invoke<{ claudeFound: boolean; claudePathFound: boolean }>('claude_found'),
  /** The items of Principal's folder a new account may share (those it has), files first. */
  accountShareable: () => invoke<string[]>('account_shareable'),
  /** A new Claude account, its folder made with the items `share` of Principal's linked or copied, saved last. */
  accountCreate: (name: string, share: string[], mode: ShareMode) => invoke<Account>('account_create', { name, share, mode }),
  /** The account's name, `claude` and « Actif » (its folder stays its own); the accounts as they then are. */
  accountUpdate: (account: Account) => invoke<Account[]>('account_update', { account }),
  /** The accounts in the order of `ids`, as they then are. */
  accountReorder: (ids: string[]) => invoke<Account[]>('account_reorder', { ids }),
  /** The account removed, its folder left on the disk; refused while an agent not archived runs on it. */
  accountRemove: (id: string) => invoke<Account[]>('account_remove', { id }),
  accountStatus: (id: string) => invoke<AccountStatus>('account_status', { id }),
  /** The account's `claude` in an interactive terminal, to sign in with (written to and killed as a terminal). */
  accountLogin: (id: string, cols: number, rows: number, onData: (d: ArrayBuffer) => void) => {
    const output = new Channel<ArrayBuffer>();
    output.onmessage = onData;
    return invoke<TermInfo>('account_login', { id, cols, rows, output });
  },
  /** `historyId`: the history of the folder to say (a closed project's, from « Rouvrir »), else its latest closed project's. */
  inspectFolder: (path: string, historyId?: string) => invoke<FolderInfo>('inspect_folder', historyId ? { path, historyId } : { path }),
  /**
   * `restore`: the folder of a closed project gets its history (same id, agents, tickets) back; else a project of its
   * own. `historyId`: which of the folder's histories comes back (the latest closed by default).
   */
  createProject: (a: {
    path: string;
    name: string;
    color: string;
    worktreePerAgent: boolean;
    firstAgent: string | null;
    restore: boolean;
    historyId?: string;
  }) => invoke<Project>('create_project', a),
  updateProject: (project: Project) => invoke<void>('update_project', { project }),
  reorderProjects: (ids: string[]) => invoke<void>('reorder_projects', { ids }),
  /** `erase`: its history goes too (conversations, tickets, settings); else it is kept as a closed project. */
  removeProject: (id: string, erase: boolean) => invoke<void>('remove_project', { id, erase }),
  /** The projects closed with their history kept, the latest closed first. */
  closedProjects: () => invoke<ClosedProject[]>('closed_projects'),
  /** The history of a closed project, erased for good (an entry the app cannot read: its one file). */
  eraseClosedProject: (id: string) => invoke<void>('erase_closed_project', { id }),
  createAgent: (projectId: string, model: string | null = null) => invoke<Agent>('create_agent', { projectId, model }),
  createAgentOnBranch: (projectId: string, branch: string, model: string | null = null) =>
    invoke<Agent>('create_agent_on_branch', { projectId, branch, model }),
  warmAgent: (id: string) => invoke<void>('warm_agent', { id }),
  /** « Relancer »: the agent's process, at rest, started again on the same session (« Outils d'Escouade au prochain démarrage »). */
  restartAgentProcess: (id: string) => invoke<void>('restart_agent_process', { id }),
  getConversation: (id: string) => invoke<ConvItem[]>('get_conversation', { id }),
  /** Searches the conversations of a project's agents (all projects' with null), archived agents included or not. */
  searchConversations: (query: string, projectId: string | null, archived: boolean) =>
    invoke<ConvSearchResult>('search_conversations', { query, projectId, archived }),
  /** `files`: those attached by their path (copies the app holds, or the files themselves): their paths go to Claude after the text. */
  sendMessage: (id: string, text: string, attachments: Attachment[] = [], files: AttachedFile[] = []) =>
    invoke<void>('send_message', { id, text, attachments, files }),
  /** Opens a copy to write, for `owner` (an agent's id, or « draft »): the handle of the write. Refused over the maximum size. */
  attachOpen: (owner: string, name: string, size: number) => invoke<string>('attach_open', { owner, name, size }),
  /** The next bytes of a copy, raw (not base64 in a JSON): the handle goes in a header. */
  attachChunk: (handle: string, bytes: Uint8Array) => invoke<void>('attach_chunk', bytes, { headers: { 'x-handle': handle } }),
  attachFinish: (handle: string) => invoke<AttachedFile>('attach_finish', { handle }),
  /** Erases a copy half written. */
  attachCancel: (handle: string) => invoke<void>('attach_cancel', { handle }),
  /** Erases a copy finished (its chip removed); the backend refuses any file that is not one of `owner`'s copies. */
  attachDiscard: (owner: string, path: string) => invoke<void>('attach_discard', { owner, path }),
  /**
   * A file attached where it is: checked, not copied. `id` is the one `fileOrigin` was asked for: the backend finds the path
   * itself, from what the host read, and refuses an id it does not know or has given out already. The page never gives a path.
   */
  attachOrigin: (owner: string, id: string) => invoke<AttachedFile>('attach_origin', { owner, id }),
  /**
   * The path of the file the window gave the WebView under `id` (see `file-origin.ts`); null when it is no file of the disk. The
   * backend keeps the path for `attachOrigin`: the page has no use for it but to know the file is one of the disk.
   */
  fileOrigin: (id: string) => invoke<string | null>('file_origin', { id }),
  /** `!command`: run in the agent's folder, its output handed to Claude with the next message (no turn started). */
  runBang: (id: string, command: string) => invoke<void>('run_bang', { id, command }),
  /** The image on the system's clipboard as PNG, empty without one: read for WebKitGTK (Linux), which leaves it out of a paste. */
  clipboardImage: () => invoke<ArrayBuffer>('clipboard_image'),
  interrupt: (id: string) => invoke<void>('interrupt', { id }),
  /** « Arrêter » on a subagent of the agent `id` (`call`: the call that launched it), by Claude Code's `stop_task`. */
  stopSubagent: (id: string, call: string) => invoke<void>('stop_subagent', { id, call }),
  /** « Arrêter » on a background task of the agent `id` (`task`: a `BgTask` id or a `WorkflowRun` id), by Claude Code's `stop_task`. Refused when it does not run, in the Claude Code view, or with no process. */
  stopBgTask: (id: string, task: string) => invoke<void>('stop_bg_task', { id, task }),
  /** The end of the output of a background task of the agent `id` (8 KiB at most while it runs, 256 KiB once it ended): the running process's, else its output file's. */
  bgTaskOutput: (id: string, task: string) => invoke<TaskOutput>('task_output', { id, task }),
  /** The conversation of a subagent in the Claude Code view, read from the transcript Claude Code keeps of it (read-only; no items while there is none). `cursor` is the one the read before gave: null back when its files did not change since. */
  subagentTranscript: (id: string, call: string, cursor: TranscriptCursor | null = null) =>
    invoke<SubagentRead | null>('subagent_transcript', { id, call, cursor }),
  /** A message to a subagent of the agent `id`: the agent is asked to pass it on (Claude Code lets no host write to a subagent). */
  messageSubagent: (id: string, call: string, text: string) => invoke<void>('message_subagent', { id, call, text }),
  answerQuestion: (id: string, requestId: string, answers: Record<string, string>) =>
    invoke<void>('answer_question', { id, requestId, answers }),
  answerPermission: (id: string, requestId: string, decision: 'allow' | 'always' | 'deny', message: string | null = null) =>
    invoke<void>('answer_permission', { id, requestId, decision, message }),
  /** « Envoyer » of a review's page: the answers go to the agent as the user's message (refused once answered). */
  answerReview: (agentId: string, reviewId: string, answers: ReviewAnswers) =>
    invoke<void>('answer_review', { agentId, reviewId, answers }),
  setAgentOptions: (id: string, o: { model?: string; effort?: string; mode?: string }) =>
    invoke<void>('set_agent_options', { id, model: o.model ?? null, effort: o.effort ?? null, mode: o.mode ?? null }),
  /** The Claude account of an agent that has not started (empty: « Automatique », the backend chooses). */
  setAgentAccount: (id: string, account: string) => invoke<void>('set_agent_account', { id, account }),
  /** « Reprendre sur <compte> »: the agent stopped by the usage limit goes on, with its session, on another account. */
  resumeOnAccount: (agentId: string, account: string) => invoke<void>('resume_on_account', { agentId, account }),
  /** « Revenir sur <compte> »: a resume on another account failed; the agent goes back to the account it came from. */
  backToPreviousAccount: (agentId: string) => invoke<void>('back_to_previous_account', { agentId }),
  renameAgent: (id: string, name: string) => invoke<void>('rename_agent', { id, name }),
  /** A copy of the agent, « <nom> (copie) », whose Claude Code session forks the original’s (refused during its turn). */
  duplicateAgent: (id: string) => invoke<Agent>('duplicate_agent', { id }),
  archiveAgent: (id: string, archived: boolean) => invoke<void>('archive_agent', { id, archived }),
  setAgentView: (id: string, terminal: boolean) => invoke<void>('set_agent_view', { id, terminal }),
  /**
   * Starts the agent's Claude Code interface. `manual`: the user asked for it (« Relancer »); the backend refuses one that is
   * not, after two stops in a row as soon as it started.
   */
  agentTerminal: (id: string, cols: number, rows: number, onData: (d: ArrayBuffer) => void, manual = false) => {
    const output = new Channel<ArrayBuffer>();
    output.onmessage = onData;
    return invoke<TermInfo>('agent_terminal', { id, cols, rows, output, manual });
  },
  /** An image pasted in the agent's Claude Code view (base64), written to a file: the path to type there, as its `claude` reads it. */
  pasteImage: (id: string, mediaType: string, data: string) => invoke<string>('paste_image', { id, mediaType, data }),
  deleteAgent: (id: string, removeWorktree: boolean) => invoke<string | null>('delete_agent', { id, removeWorktree }),
  /** Merges into the agent's base branch; `switchToBase`: the project's folder is switched to it first. */
  mergeAgent: (id: string, squash: boolean, switchToBase = false) => invoke<string>('merge_agent', { id, squash, switchToBase }),
  integrateBase: (id: string) => invoke<Integration>('integrate_base', { id }),
  getCommands: (id: string) => invoke<SlashCommand[]>('get_commands', { id }),
  fileSuggestions: (id: string, query: string) => invoke<string[]>('file_suggestions', { id, query }),
  gitFiles: (projectId: string, agentId: string | null) => invoke<FileChange[]>('git_files', { projectId, agentId }),
  gitDiff: (projectId: string, agentId: string | null, paths: string[]) => invoke<string>('git_diff', { projectId, agentId, paths }),
  gitProjectDiff: (projectId: string) => invoke<OwnedDiff[]>('git_project_diff', { projectId }),
  gitLog: (projectId: string, agentId: string | null) => invoke<GitLog>('git_log', { projectId, agentId }),
  gitShow: (projectId: string, hash: string) => invoke<string>('git_show', { projectId, hash }),
  /** Fetch, pull (fast-forward only) and push of the project's checkout; each returns a summary. */
  gitFetch: (projectId: string) => invoke<string>('git_fetch', { projectId }),
  gitPull: (projectId: string) => invoke<string>('git_pull', { projectId }),
  gitPush: (projectId: string) => invoke<string>('git_push', { projectId }),
  /** The project's branches: the local ones (the folder's first), then the remote ones. */
  branchList: (projectId: string) => invoke<BranchInfo[]>('branch_list', { projectId }),
  /** Rejected with why `name` can't be a new branch (git's rules, a branch of that name already there). */
  branchCheck: (projectId: string, name: string) => invoke<void>('branch_check', { projectId, name }),
  /**
   * Switches the project's folder to `name` (a remote branch "origin/x": the local one that tracks it, made if need be); the stash's name when
   * `stash` put the uncommitted changes aside. Rejected with `DIRTY`, `IN_WORKTREE:<agent id>:<agent name>`, `AGENT_WORKING:<agent id>:<agent name>`.
   */
  branchSwitch: (projectId: string, name: string, stash = false) => invoke<string | null>('branch_switch', { projectId, name, stash }),
  /** Creates `name` at `start` (a branch, a commit; HEAD when empty); `switchTo`: the folder goes there, rejected as `branchSwitch` is. */
  branchCreate: (projectId: string, name: string, start: string, switchTo: boolean, stash = false) =>
    invoke<string | null>('branch_create', { projectId, name, start, switch: switchTo, stash }),
  /**
   * Deletes a branch, its remote one too with `remote` (or a remote branch alone, "origin/x"). Rejected with `UNMERGED:<n>` (`n` commits in no
   * other branch, maybe 0) unless `force`, `IN_WORKTREE:<agent id>:<agent name>`, or in words for the folder's own branch. Resolves with a
   * sentence to show when `remote` was asked but only the local branch went (its remote copy is the default branch, the base's, named
   * otherwise, or tracked by another branch), else null.
   */
  branchDelete: (projectId: string, name: string, remote: boolean, force = false) =>
    invoke<string | null>('branch_delete', { projectId, name, remote, force }),
  /** The local branches already in the project's base, but the base, the folder's branch and the worktrees' ones. */
  branchesMerged: (projectId: string) => invoke<string[]>('branches_merged', { projectId }),
  /** The diff from `a` to `b` (`git diff a b`), two branches or commits. */
  gitDiffRefs: (projectId: string, a: string, b: string) => invoke<string>('git_diff_refs', { projectId, a, b }),
  /**
   * Turns an agent's remote control on or off by hand (off sets it apart from the settings that cover it). Resolves with
   * whether the change waits for the next launch of the Claude Code interface that is open: Escouade can't change it there.
   */
  setRemoteControl: (id: string, enabled: boolean) => invoke<boolean>('set_remote_control', { id, enabled }),
  /** The statistics of a period: of every account (null) or of one. */
  stats: (range: string, account: string | null = null) => invoke<StatsView>('stats', { range, account }),
  refreshUsage: () => invoke<void>('refresh_usage'),
  gitDiscard: (projectId: string, agentId: string | null, path: string) => invoke<void>('git_discard', { projectId, agentId, path }),
  /** What a direct commit of the agent's changes takes (null: the project's own checkout, its agents' worktrees apart). */
  commitPreview: (projectId: string, agentId: string | null) => invoke<CommitScope>('commit_preview', { projectId, agentId }),
  /** Haiku's message for a direct commit of `paths`, in the style of the repository's latest commits: only proposed. */
  commitPropose: (projectId: string, agentId: string | null, paths: string[]) =>
    invoke<string>('commit_propose', { projectId, agentId, paths }),
  /** Commits `paths` with the user's message; the commit's short hash. */
  commitDirect: (projectId: string, agentId: string | null, paths: string[], message: string) =>
    invoke<string>('commit_direct', { projectId, agentId, paths, message }),
  fsTree: (projectId: string, agentId: string | null) => invoke<FileTree>('fs_tree', { projectId, agentId }),
  fsRead: (projectId: string, agentId: string | null, path: string) => invoke<FileText>('fs_read', { projectId, agentId, path }),
  /** Refused with "changed" / "deleted" when the file is no longer the one read (`expectedHash`); null forces. */
  fsWrite: (a: {
    projectId: string;
    agentId: string | null;
    path: string;
    text: string;
    eol: 'lf' | 'crlf';
    bom: boolean;
    expectedHash: string | null;
  }) => invoke<string>('fs_write', a),
  /** Creates an empty file, its folders with it; refused when something is already there. */
  fsCreate: (projectId: string, agentId: string | null, path: string) => invoke<void>('fs_create', { projectId, agentId, path }),
  /** Renames or moves a file or a folder (folders of `to` created); refused when something else is at `to`. */
  fsRename: (projectId: string, agentId: string | null, from: string, to: string) =>
    invoke<void>('fs_rename', { projectId, agentId, from, to }),
  /** Sends a file or a folder to the system's trash; never the root nor the agents' worktrees. */
  fsDelete: (projectId: string, agentId: string | null, path: string) => invoke<void>('fs_delete', { projectId, agentId, path }),
  /** Shows a file or a folder in the system's file manager, selected; null opens the source's folder. */
  fsReveal: (projectId: string, agentId: string | null, path: string | null) => invoke<void>('fs_reveal', { projectId, agentId, path }),
  /** Creates an empty folder, its parents with it; refused when something is already there. */
  fsMkdir: (projectId: string, agentId: string | null, path: string) => invoke<void>('fs_mkdir', { projectId, agentId, path }),
  fsBase: (projectId: string, agentId: string | null, path: string) => invoke<FileBase | null>('fs_base', { projectId, agentId, path }),
  /** The lines of the source's files (the ignored ones and the agents' worktrees left out) that match `query`. */
  codeSearch: (projectId: string, agentId: string | null, query: SearchQuery) =>
    invoke<SearchResult>('code_search', { projectId, agentId, query }),
  setUnsaved: (count: number) => invoke<void>('set_unsaved', { count }),
  cancelResume: (id: string) => invoke<void>('cancel_resume', { id }),
  ticketCreate: (projectId: string, draft: TicketDraft) => invoke<Ticket>('ticket_create', { projectId, draft }),
  ticketUpdate: (id: string, draft: TicketDraft) => invoke<Ticket>('ticket_update', { id, draft }),
  ticketDelete: (id: string) => invoke<void>('ticket_delete', { id }),
  /** The images of a ticket with their content, for its form; those whose file is gone are left out. */
  ticketImages: (id: string) => invoke<TicketImageData[]>('ticket_images', { id }),
  ticketPrioritize: (id: string) => invoke<void>('ticket_prioritize', { id }),
  /** "Lancer": starts even with the autopilot off. */
  ticketStart: (id: string) => invoke<void>('ticket_start', { id }),
  ticketResume: (id: string) => invoke<void>('ticket_resume', { id }),
  ticketApprove: (id: string) => invoke<void>('ticket_approve', { id }),
  ticketReject: (id: string, comment: string) => invoke<void>('ticket_reject', { id, comment }),
  ticketResolveConflict: (id: string) => invoke<void>('ticket_resolve_conflict', { id }),
  ticketDismiss: (id: string) => invoke<void>('ticket_dismiss', { id }),
  boardSet: (projectId: string, settings: BoardSettings) => invoke<Project>('board_set', { projectId, settings }),
  /** « Reprendre maintenant »: the autopilot's pause (a quota, a usage limit) is lifted. */
  autopilotResume: () => invoke<void>('autopilot_resume'),
  gitBranches: (projectId: string) => invoke<string[]>('git_branches', { projectId }),
  /** Checks the credentials with the service, then saves them (apart, never sent back). */
  integrationConnect: (service: Service, account: AccountForm) => invoke<AccountView>('integration_connect', { service, account }),
  integrationDisconnect: (service: Service) => invoke<AccountView[]>('integration_disconnect', { service }),
  /** Jira projects, Trello boards, GitHub repositories (the project's own first). */
  integrationContainers: (service: Service, projectId: string | null) =>
    invoke<Container[]>('integration_containers', { service, projectId }),
  integrationStates: (service: Service, container: string) => invoke<StatesView>('integration_states', { service, container }),
  /** `page`: the `next` of the page before (« Afficher plus »); none for the first one. */
  integrationIssues: (projectId: string, service: Service, text: string, filters: string[], page?: string) =>
    invoke<IssuePage>('integration_issues', { projectId, service, text, filters, page }),
  integrationImport: (projectId: string, issues: ExternalIssue[], maxLoops: number) =>
    invoke<Ticket[]>('integration_import', { projectId, issues, maxLoops }),
  /** « Resynchroniser »: the ticket's failed syncs go again at once; rejected with why they still fail. */
  integrationResync: (ticketId: string) => invoke<void>('integration_resync', { ticketId }),
  /** `agentId` and `subdir` say where it opens: the agent's worktree, a folder of it (see `TermPlace`). */
  termSpawn: (
    a: { projectId: string; agentId?: string | null; subdir?: string; shell: string; name: string; cols: number; rows: number },
    onData: (d: ArrayBuffer) => void,
  ) => {
    const output = new Channel<ArrayBuffer>();
    output.onmessage = onData;
    return invoke<TermInfo>('term_spawn', { ...a, output });
  },
  runStart: (
    a: { projectId: string; commandId: string; cols: number; rows: number; cursorRow: number },
    onData: (d: ArrayBuffer) => void,
  ) => {
    const output = new Channel<ArrayBuffer>();
    output.onmessage = onData;
    return invoke<TermInfo>('run_start', { ...a, output });
  },
  /** "Préparer le lancement": reserves the agent's ports and asks it for its recipe. */
  agentPrepareLaunch: (id: string) => invoke<void>('agent_prepare_launch', { id }),
  /** « Lancer » in the test modal: the user read `recipe` (the agent's, as shown) and lets it run. */
  testRecipeApprove: (agentId: string, recipe: TestRecipe) => invoke<void>('test_recipe_approve', { agentId, recipe }),
  testRunStart: (
    a: { agentId: string; kind: 'prep' | 'run' | 'isola'; index: number; cols: number; rows: number; cursorRow: number },
    onData: (d: ArrayBuffer) => void,
  ) => {
    const output = new Channel<ArrayBuffer>();
    output.onmessage = onData;
    return invoke<TermInfo>('test_run_start', { ...a, output });
  },
  /** The worktree's .isola.toml, whole, as `isola up` will read it: what is shown before the services start. */
  isolaConfig: (agentId: string) => invoke<string>('isola_config', { agentId }),
  /** « Lancer » for isola: the user read `config` (the .isola.toml) and `open` (the address to open) and lets `isola up` run. */
  isolaApprove: (agentId: string, config: string, open: string) => invoke<void>('isola_approve', { agentId, config, open }),
  /** The services isola runs for the agent's worktree, with their addresses. */
  isolaServices: (agentId: string) => invoke<IsolaService[]>('isola_services', { agentId }),
  /** Stops the services isola runs for the agent's worktree. */
  isolaDown: (agentId: string) => invoke<void>('isola_down', { agentId }),
  /** The setup and teardown Claude suggests for the project's worktrees, from what it reads of it. */
  suggestWorktreeSteps: (projectId: string) => invoke<WorktreeSuggestion>('suggest_worktree_steps', { projectId }),
  /** The launch commands Claude suggests for the project, from what it reads of it, with how many it gave that were refused; nothing is saved. */
  suggestRunCommands: (projectId: string) => invoke<RunSuggestion>('suggest_run_commands', { projectId }),
  /**
   * A newer release, through the network settings (proxy, TLS verification), kept by the backend for its download.
   * `manual`: the user asked, so a failure is always to be shown.
   */
  updateCheck: (manual: boolean) => invoke<CheckedUpdate>('update_check', { manual }),
  /**
   * Downloads the release found, its signature checked, kept by the backend until it installs: ready once it is the
   * one, superseded when a newer one took its place meanwhile.
   */
  updateDownload: (id: number, manual: boolean) => invoke<DownloadedUpdate>('update_download', { id, manual }),
  updateClose: (id: number) => invoke<void>('update_close', { id }),
  /** « Redémarrer maintenant »: the update downloaded installs, the app stopped cleanly first, and the app starts again. */
  updateRestart: () => invoke<void>('update_restart'),
  /** « Plus tard »: the automatic restart planned is called off. */
  updatePostpone: () => invoke<void>('update_postpone'),
  /** What an automatic restart waits for, as the window sees it. */
  updatePresence: (presence: { modal: boolean; testing: boolean; activeAt: number }) => invoke<void>('update_presence', { presence }),
  /** One HTTP request without proxy (2 s): true for any answer. */
  httpReady: (url: string) => invoke<boolean>('http_ready', { url }),
  /** The write's number, which the backend's log tells it by. */
  termWrite: (id: string, data: string) => invoke<number>('term_write', { id, data }),
  termReport: (id: string, report: TermReport) => invoke<void>('term_report', { id, report }),
  termResize: (id: string, cols: number, rows: number) => invoke<void>('term_resize', { id, cols, rows }),
  termKill: (id: string) => invoke<void>('term_kill', { id }),
  playChime: () => invoke<void>('play_chime'),
  quit: () => invoke<void>('quit_app'),
};
