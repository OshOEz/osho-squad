<script lang="ts" module>
  const commandCache = new Map<string, { name: string; description: string; argumentHint?: string }[]>();
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import {
    MAX_TOTAL,
    attachByPath,
    clipboardImage,
    inMessageBytes,
    planAttachment,
    planByPath,
    readForMessage,
    type DraftAttachment,
  } from '../lib/attachments';
  import { accountName, newAgentAccount, serves } from '../lib/accounts';
  import { applyCompletion, detectTrigger, filterCommands, type Trigger } from '../lib/complete';
  import { conversationOf } from '../lib/conversations.svelte';
  import { getDraft, setDraft } from '../lib/drafts';
  import { injectedSource } from '../lib/events';
  import { fileOrigins } from '../lib/file-origin';
  import { basename, dirname } from '../lib/format';
  import { t } from '../lib/i18n';
  import { api } from '../lib/ipc';
  import { EFFORTS, MODES, modelOptions, supportsAuto, supportsEffort } from '../lib/models';
  import { keyLabel } from '../lib/platform';
  import { observeWidth } from '../lib/resize';
  import { app } from '../lib/state.svelte';
  import type { Agent, QuestionItem } from '../lib/types';
  import AttachmentChip from './AttachmentChip.svelte';
  import Dropdown from './Dropdown.svelte';
  import { tip } from '../lib/tip';

  let { agent }: { agent: Agent } = $props();

  // The agent object is replaced on every backend update: only its id may drive the draft.
  const agentId = $derived(agent.id);
  let draftFor = untrack(() => agent.id);
  const initial = getDraft(draftFor);
  let text = $state(initial.text);
  let files = $state<DraftAttachment[]>(initial.files);
  let ta = $state<HTMLTextAreaElement>();
  let trigger = $state<Trigger | null>(null);
  let suggestions = $state<{ label: string; detail: string; value: string }[]>([]);
  let sel = $state(0);
  let sending = $state(false);
  /** The agent whose message being sent the backend holds until its worktree is prepared. */
  let heldFor: string | null = null;
  /** Files being read, not attached yet. */
  let reading = $state(0);
  /** A file is being copied: its chip is there, but its path is not known yet. */
  const copying = $derived(files.some((f) => f.copying));
  let dragOver = $state(false);
  let menu = $state<'model' | 'effort' | 'mode' | 'account' | null>(null);
  let width = $state(0);
  let fileInput = $state<HTMLInputElement>();
  let reqSeq = 0;

  const conv = $derived(conversationOf(agent.id));
  const pendingItem = $derived(agent.pending.length ? conv.items.find((i) => i.id === agent.pending[0]) : undefined);
  const busy = $derived(agent.status === 'running' || agent.status === 'waiting');
  const effortOk = $derived(supportsEffort(agent.model));
  const modeOptions = $derived(
    MODES.map((md) => {
      const unavailable = md.value === 'auto' && !supportsAuto(agent.model);
      return {
        value: md.value,
        label: md.label,
        detail: unavailable ? t('composer.mode.autoUnavailableDetail') : md.title,
        title: unavailable ? t('composer.mode.autoUnavailableTitle') : md.title,
        disabled: unavailable,
      };
    }),
  );
  // The account chip: with several accounts, beside the model. It changes the account only while the agent has no session
  // (nothing said to it yet, no copy's original behind it): then its conversation is filed in that account.
  const project = $derived(app.projects.find((p) => p.id === agent.projectId));
  const accounts = $derived(app.settings.accounts);
  const accountLocked = $derived(!!agent.sessionId || !!agent.forkOf || agent.prompts > 0 || busy);
  // « Automatique » names the account a new agent of the project would go to: none when every one is reserved to others.
  const autoAccount = $derived(newAgentAccount(project));
  const accountOptions = $derived(
    accounts.length < 2
      ? []
      : [
          {
            value: '',
            label: autoAccount ? t('accounts.composer.auto', { name: accountName(autoAccount) }) : t('accounts.composer.autoNone'),
            detail: t('accounts.composer.autoDetail'),
          },
          // The agent's own, even switched off since: it is where it is.
          ...accounts
            .filter((a) => (a.active && serves(a, agent.projectId)) || a.id === agent.account)
            .map((a) => ({ value: a.id, label: a.active ? accountName(a) : t('accounts.composer.inactive', { name: accountName(a) }) })),
        ],
  );
  // Keeps the send button on the same line in a narrow column (split layout): drop the captions.
  const tight = $derived(width > 0 && width < (busy ? 640 : 540));
  const placeholder = $derived(
    pendingItem
      ? pendingItem.kind === 'permission'
        ? t('composer.placeholder.denyPermission')
        : t('composer.placeholder.answerQuestion')
      : conv.items.length
        ? t('composer.placeholder.sendTo', { name: agent.name })
        : t('composer.placeholder.describe'),
  );

  // Reused for another agent (the id itself changed): switch drafts.
  $effect(() => {
    const id = agentId;
    if (id === draftFor) return;
    untrack(() => {
      draftFor = id;
      const d = getDraft(id);
      text = d.text;
      files = d.files;
      trigger = null;
      suggestions = [];
      queueMicrotask(autosize);
    });
  });

  // Drafts are saved as they are typed, so they survive agent switches (and, for their text, a restart).
  $effect(() => {
    const draft = { text, files: files.slice() };
    untrack(() => setDraft(draftFor, draft));
  });

  $effect(() => {
    queueMicrotask(autosize);
  });

  $effect(() => {
    void app.focusComposer;
    queueMicrotask(() => ta?.focus());
  });

  function autosize() {
    if (!ta) return;
    ta.style.height = 'auto';
    ta.style.height = Math.min(ta.scrollHeight, window.innerHeight * 0.4) + 'px';
  }

  // Also when the text changes without typing: cleared once sent, another agent's draft, recall.
  $effect(() => {
    void text;
    autosize();
  });

  async function refreshSuggestions() {
    if (!ta) return;
    trigger = detectTrigger(text, ta.selectionStart);
    const trig = trigger;
    if (!trig) {
      suggestions = [];
      return;
    }
    const seq = ++reqSeq;
    if (trig.kind === 'file') {
      const files = await api.fileSuggestions(agent.id, trig.query).catch(() => [] as string[]);
      if (seq !== reqSeq) return;
      suggestions = files.slice(0, 12).map((f) => ({ label: basename(f), detail: dirname(f), value: f }));
    } else {
      let cmds = commandCache.get(agent.id);
      if (!cmds) {
        cmds = await api.getCommands(agent.id).catch(() => []);
        if (cmds.length) commandCache.set(agent.id, cmds);
      }
      if (seq !== reqSeq) return;
      suggestions = filterCommands(cmds, trig.query).map((c) => ({
        label: '/' + c.name + (c.argumentHint ? ' ' + c.argumentHint : ''),
        detail: c.description,
        value: c.name,
      }));
    }
    sel = 0;
  }

  function accept(i: number) {
    const s = suggestions[i];
    if (!s || !trigger || !ta) return;
    const r = applyCompletion(text, trigger, s.value);
    text = r.text;
    suggestions = [];
    trigger = null;
    queueMicrotask(() => {
      ta?.setSelectionRange(r.caret, r.caret);
      ta?.focus();
      autosize();
    });
  }

  function lastUserMessage(): string | null {
    for (let i = conv.items.length - 1; i >= 0; i--) {
      const it = conv.items[i];
      if (it.kind === 'user' && !injectedSource(it)) return it.text;
    }
    return null;
  }

  function onKeydown(e: KeyboardEvent) {
    // A dialog is open above the composer: its keys (Escape…) are not for the agent.
    if (app.modal) return;
    if (suggestions.length) {
      if (e.key === 'ArrowDown') {
        sel = (sel + 1) % suggestions.length;
        e.preventDefault();
        return;
      }
      if (e.key === 'ArrowUp') {
        sel = (sel - 1 + suggestions.length) % suggestions.length;
        e.preventDefault();
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        accept(sel);
        e.preventDefault();
        return;
      }
      if (e.key === 'Escape') {
        suggestions = [];
        trigger = null;
        e.preventDefault();
        e.stopPropagation();
        return;
      }
    }
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
      return;
    }
    if (e.key === 'Escape' && busy) {
      e.preventDefault();
      e.stopPropagation();
      stop();
      return;
    }
    if (e.key === 'ArrowUp' && !text) {
      const last = lastUserMessage();
      if (last) {
        text = last;
        e.preventDefault();
        queueMicrotask(autosize);
      }
    }
  }

  // A file that cannot be attached is named in a toast, never silently dropped. Sending waits for
  // the files being read or copied, and one read while switching agents goes to the draft it was added to.
  async function addFiles(source: File[] | Promise<File[]>) {
    const owner = draftFor;
    // Asked at once, while the files are read: what the host holds for each of them is an id, never a path the page could give.
    const origins = app.settings.attachOrigin && Array.isArray(source) ? fileOrigins(source) : null;
    reading++;
    try {
      const list = await source;
      for (const [i, f] of list.entries()) {
        try {
          await attach(f, owner, origins?.then((o) => o[i] ?? null) ?? null);
        } catch (e) {
          failed(e);
        }
      }
    } catch (e) {
      failed(e);
    } finally {
      reading--;
    }
  }

  const draftFiles = (owner: string) => (owner === draftFor ? files : getDraft(owner).files);

  function add(owner: string, a: DraftAttachment) {
    if (owner === draftFor) files.push(a);
    else setDraft(owner, { text: getDraft(owner).text, files: [...getDraft(owner).files, a] });
  }

  /** Takes an attachment out of the draft of `owner`, if it is still there (a copy that failed). */
  function leave(owner: string, a: DraftAttachment) {
    const list = draftFiles(owner);
    const i = list.indexOf(a);
    if (i >= 0) list.splice(i, 1);
  }

  /**
   * Attaches a file to the draft of `owner`: in the message when Claude reads it and the message has room, else by its path
   * (the chip is there at once, while the file is copied or its path checked). `origin`: the id the host holds for the file
   * when it lies on the disk, if it knows it.
   */
  async function attach(f: File, owner: string, origin: Promise<string | null> | null) {
    const settings = app.settings;
    let plan = planAttachment(f, { settings, messageBytes: inMessageBytes(draftFiles(owner)) });
    if ('refusal' in plan) throw new Error(plan.refusal);
    if (plan.kind !== 'file') {
      const a = await readForMessage(f, plan.kind);
      // Another file may have taken the room while this one was read.
      if (a && inMessageBytes(draftFiles(owner)) + a.size <= MAX_TOTAL) return add(owner, a);
      // Not text after all, or no room left: its path carries it.
      plan = planByPath(f, settings);
      if ('refusal' in plan) throw new Error(plan.refusal);
    }
    const abort = new AbortController();
    const chip = $state<DraftAttachment>({
      kind: 'file',
      name: f.name,
      mediaType: '',
      data: '',
      size: f.size,
      warn: plan.warn,
      copying: true,
      progress: 0,
      cancel: () => abort.abort(),
    });
    add(owner, chip);
    // Not waited for: the next files of a drop do not wait for the copy of a big one, and sending waits for the chip itself.
    void attachChip(f, owner, chip, abort.signal, origin);
  }

  /** The copy of a file (or the check of its path) that its chip stands for, until its path is known. */
  async function attachChip(f: File, owner: string, chip: DraftAttachment, signal: AbortSignal, origin: Promise<string | null> | null) {
    try {
      chip.file = await attachByPath(f, owner, {
        origin: await origin,
        signal,
        onProgress: (done) => (chip.progress = done / f.size),
      });
      chip.copying = false;
      chip.cancel = undefined;
    } catch (e) {
      leave(owner, chip);
      // Taken away by the user: nothing to say.
      if (!signal.aborted) failed(e);
    }
  }

  /** Takes an attachment away: a copy still going stops, one finished is erased (never a file attached where it is). */
  function remove(i: number) {
    const [a] = files.splice(i, 1);
    if (a.copying) a.cancel?.();
    else if (a.file && !a.file.origin) api.attachDiscard(draftFor, a.file.path).catch(() => {});
  }

  function failed(e: unknown) {
    app.toast(e instanceof Error ? e.message : String(e), 'error');
  }

  function onPaste(e: ClipboardEvent) {
    const list = [...(e.clipboardData?.files ?? [])];
    if (list.length) {
      e.preventDefault();
      addFiles(list);
    } else if (!e.clipboardData?.types.includes('text/plain')) {
      addFiles(clipboardImage().then((f) => (f ? [f] : [])));
    }
  }

  function onDrop(e: DragEvent) {
    dragOver = false;
    const list = [...(e.dataTransfer?.files ?? [])];
    if (list.length) {
      // Left to the WebView, a dropped file would replace the app.
      e.preventDefault();
      addFiles(list);
    }
  }

  async function send() {
    const body = text.trim();
    if ((!body && !files.length) || reading || copying) return;
    if (sending) {
      // The one before waits for the worktree, which can take minutes: this one is not dropped without a word.
      if (heldFor === agent.id) app.toast(t('composer.send.alreadyWaiting'));
      return;
    }
    // Text typed while Claude waits answers the question / refuses the permission.
    const answering = body ? pendingItem : undefined;
    // `!command`, as in Claude Code: run in the agent's folder, its output given to Claude with the next message.
    const bang = !answering && !agent.terminal && body.startsWith('!') ? body.slice(1).trim() : '';
    sending = true;
    const owner = draftFor;
    const prevText = text;
    const prevFiles = files;
    // A new worktree is prepared first: the backend holds the message until then, and it shows meanwhile.
    const unhold =
      !answering && !bang && agent.setup
        ? conv.holdMessage({
            text: body,
            images: prevFiles.filter((f) => f.kind === 'image').length,
            files: prevFiles.filter((f) => f.kind !== 'image').map((f) => f.name),
          })
        : undefined;
    if (unhold) heldFor = agent.id;
    text = '';
    if (!answering && !bang) files = [];
    try {
      if (answering?.kind === 'question') {
        const q = answering as QuestionItem;
        await api.answerQuestion(agent.id, q.id, Object.fromEntries(q.questions.map((x) => [x.question, body])));
      } else if (answering?.kind === 'permission') {
        await api.answerPermission(agent.id, answering.id, 'deny', body);
      } else if (bang) {
        await api.runBang(agent.id, bang);
      } else {
        await api.sendMessage(
          agent.id,
          body,
          prevFiles.filter((f) => f.kind !== 'file').map(({ name, mediaType, data }) => ({ name, mediaType, data })),
          prevFiles.filter((f) => f.file).map((f) => $state.snapshot(f.file!)),
        );
      }
      if (answering && prevFiles.length) {
        app.toast(t('composer.send.filesKept'));
      }
    } catch (e) {
      // The files of an answer never left the field.
      giveBack(owner, prevText, answering || bang ? [] : prevFiles);
      app.toast(String(e), 'error');
    }
    // Recorded by now, or given back to the field above.
    unhold?.();
    heldFor = null;
    sending = false;
  }

  /**
   * A message refused given back to the draft it was sent from (another agent's may be on screen by now), before what
   * was typed and attached there since: a send held for minutes by a worktree's setup leaves time to write the next one.
   */
  function giveBack(owner: string, back: string, backFiles: DraftAttachment[]) {
    const here = owner === draftFor;
    const since = here ? { text, files } : getDraft(owner);
    const draft = { text: since.text.trim() ? `${back}\n\n${since.text}` : back, files: [...backFiles, ...since.files] };
    if (!here) return setDraft(owner, draft);
    text = draft.text;
    files = draft.files;
  }

  function stop() {
    app.run(api.interrupt(agent.id));
  }

  function toggleMenu(m: 'model' | 'effort' | 'mode' | 'account') {
    menu = menu === m ? null : m;
  }

  function pick(o: { model?: string; effort?: string; mode?: string }) {
    menu = null;
    setOption(o);
    ta?.focus();
  }

  // A dialog opening over an open menu would leave it hidden, still catching Escape.
  $effect(() => {
    if (app.modal) menu = null;
  });

  /** Empty: « Automatique », which account that is is the backend's to say (it tells the agent back). */
  async function pickAccount(id: string) {
    menu = null;
    const a = app.agents[agent.id];
    const before = a?.account;
    if (a && id) a.account = id;
    ta?.focus();
    try {
      await api.setAgentAccount(agent.id, id);
    } catch (e) {
      // Refused: the agent is where it was, as the chip must say.
      if (a && before !== undefined) a.account = before;
      app.toast(String(e), 'error');
    }
  }

  function setOption(o: { model?: string; effort?: string; mode?: string }) {
    const a = app.agents[agent.id];
    if (a) Object.assign(a, o);
    if (o.mode === 'bypassPermissions') app.toast(t('composer.mode.bypassToast'), 'info');
    app.run(api.setAgentOptions(agent.id, o));
  }
</script>

<div class="composer-wrap">
  <div
    class="composer"
    class:pending={!!pendingItem}
    class:drag={dragOver}
    role="group"
    ondragover={(e) => {
      if (e.dataTransfer?.types.includes('Files')) {
        e.preventDefault();
        dragOver = true;
      }
    }}
    ondragleave={() => (dragOver = false)}
    ondrop={onDrop}
  >
    {#if suggestions.length}
      <div class="suggest" role="listbox">
        {#each suggestions as s, i (s.value)}
          <button
            class="sug"
            class:on={i === sel}
            role="option"
            aria-selected={i === sel}
            onmousedown={(e) => {
              e.preventDefault();
              accept(i);
            }}
            onmouseenter={() => (sel = i)}
          >
            <span class="sl mono">{s.label}</span>
            <span class="sd">{s.detail}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#if files.length}
      <div class="atts">
        {#each files as f, i (i)}
          {#if f.url}
            <div class="img">
              <img src={f.url} alt={f.name} />
              <button class="rm" aria-label={t('composer.attachments.remove', { name: f.name })} onclick={() => remove(i)}>×</button>
            </div>
          {:else}
            <AttachmentChip attachment={f} onremove={() => remove(i)} />
          {/if}
        {/each}
      </div>
    {/if}
    <textarea
      bind:this={ta}
      bind:value={text}
      {placeholder}
      rows="2"
      spellcheck="false"
      oninput={() => {
        autosize();
        refreshSuggestions();
      }}
      onkeydown={onKeydown}
      onclick={refreshSuggestions}
      onpaste={onPaste}
      onblur={() => setTimeout(() => (suggestions = []), 120)}
    ></textarea>
    <div class="bar" use:observeWidth={(w) => (width = w)}>
      <Dropdown
        caption={t('common.model')}
        value={agent.model}
        options={modelOptions(app.models)}
        open={menu === 'model'}
        showCaption={!tight}
        onToggle={() => toggleMenu('model')}
        onPick={(v) => pick({ model: v })}
      />
      {#if accounts.length > 1}
        <Dropdown
          caption={t('accounts.composer.caption')}
          value={agent.account}
          options={accountOptions}
          open={menu === 'account'}
          showCaption={!tight}
          readonly={accountLocked}
          title={accountLocked ? t('accounts.composer.locked') : undefined}
          onToggle={() => toggleMenu('account')}
          onPick={pickAccount}
        />
      {/if}
      <Dropdown
        caption={t('composer.bar.effort')}
        value={effortOk ? agent.effort : ''}
        options={EFFORTS.map((ef) => ({ value: ef.value, label: ef.label, detail: ef.title }))}
        open={menu === 'effort'}
        showCaption={!tight}
        disabled={!effortOk}
        title={effortOk ? t('composer.bar.effortTitle') : t('composer.bar.effortUnsupported')}
        onToggle={() => toggleMenu('effort')}
        onPick={(v) => pick({ effort: v })}
      />
      <Dropdown
        caption={t('composer.bar.mode')}
        value={agent.mode}
        options={modeOptions}
        open={menu === 'mode'}
        showCaption={!tight}
        danger={agent.mode === 'bypassPermissions'}
        onToggle={() => toggleMenu('mode')}
        onPick={(v) => pick({ mode: v })}
      />
      <div style="flex:1"></div>
      {#if busy}
        <button
          class="btn ghost stop"
          class:icon={tight}
          onclick={stop}
          use:tip={t('composer.bar.interrupt', { key: keyLabel('Esc') })}
          aria-label={t('composer.bar.stop')}>{tight ? '■' : `■ ${t('composer.bar.stop')}`}</button
        >
      {/if}
      <input
        bind:this={fileInput}
        type="file"
        multiple
        hidden
        aria-label={t('composer.attach.label')}
        onchange={(e) => {
          // Copied first: the list is the input's own, emptied with its value below, before `addFiles` reads it.
          addFiles([...(e.currentTarget.files ?? [])]);
          e.currentTarget.value = '';
        }}
      />
      <button
        class="icon-btn attach"
        use:tip={t('composer.attach.title')}
        aria-label={t('composer.attach.label')}
        onclick={() => fileInput?.click()}
      >
        <svg
          width="15"
          height="15"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          ><path d="M21.4 11.1l-9.2 9.2a6 6 0 0 1-8.5-8.5l9.2-9.2a4 4 0 0 1 5.7 5.7l-9.2 9.2a2 2 0 0 1-2.8-2.8l8.5-8.5" /></svg
        >
      </button>
      <button
        class="btn primary"
        use:tip={busy && !pendingItem
          ? t('composer.send.titleQueued', { enter: keyLabel('Enter') })
          : t('composer.send.title', { enter: keyLabel('Enter'), newline: keyLabel('Shift+Enter') })}
        disabled={sending || reading > 0 || copying || (!text.trim() && !files.length)}
        onclick={send}
      >
        {t('composer.send.label')}
      </button>
    </div>
  </div>
</div>

<style>
  .composer-wrap {
    flex: none;
    padding: 0 28px 20px;
  }
  .composer {
    position: relative;
    max-width: 780px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    border-radius: var(--r);
    border: 1px solid var(--line2);
    background: var(--elev);
    transition: border-color 0.15s;
  }
  .composer:focus-within {
    border-color: color-mix(in oklch, var(--accent) 55%, var(--line2));
  }
  .composer.pending {
    border-color: var(--wait);
  }
  .composer.drag {
    border-style: dashed;
    border-color: var(--accent);
  }
  textarea {
    resize: none;
    border: none;
    outline: none;
    background: transparent;
    font-size: 14px;
    line-height: 1.5;
    padding: 14px 16px 6px;
    min-height: 58px;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 10px 14px;
    min-width: 0;
  }
  .bar > :global(button) {
    flex: none;
  }
  .stop {
    height: 30px;
    color: var(--del);
  }
  .stop.icon {
    padding: 0 10px;
  }
  .atts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 12px 14px 0;
  }
  .img {
    position: relative;
    width: 64px;
    height: 64px;
    border-radius: var(--r-sm);
    overflow: hidden;
    border: 1px solid var(--line2);
  }
  .img img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .rm {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 18px;
    height: 18px;
    border: none;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }
  .suggest {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + 6px);
    z-index: 30;
    display: flex;
    flex-direction: column;
    padding: 5px;
    max-height: 320px;
    overflow: auto;
    border-radius: var(--r);
    border: 1px solid var(--line2);
    background: var(--elev);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  }
  .sug {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 7px 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }
  .sug.on {
    background: var(--elev2);
  }
  .sl {
    font-size: 12.5px;
    flex: none;
  }
  .sd {
    font-size: 12px;
    color: var(--dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
</style>
