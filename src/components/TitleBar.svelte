<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import { api } from '../lib/ipc';
  import { menu } from '../lib/menu.svelte';
  import { askCloseProject } from '../lib/project-actions';
  import { shownStatus } from '../lib/review';
  import { app } from '../lib/state.svelte';
  import { IS_MAC, keyLabel } from '../lib/platform';
  import {
    alertColor,
    edgeSpeed,
    hiddenSides,
    mostPressing,
    revealScroll,
    thumb,
    thumbDrag,
    wheelScroll,
    type Metrics,
  } from '../lib/tab-strip';
  import { PROJECT_COLORS } from '../lib/theme';
  import type { Project } from '../lib/types';
  import { tip } from '../lib/tip';

  /** Width of the fades at the edges of the tabs, and the room left around a tab brought into view. */
  const FADE = 28;

  const win = getCurrentWindow();
  let maximized = $state(false);
  let dragId = $state<string | null>(null);
  let dropIndex = $state<number | null>(null);

  // The tabs scroll sideways in their strip when they are more than its room holds.
  let scroller = $state<HTMLElement>();
  let track = $state<HTMLElement>();
  const tabEls: Record<string, HTMLElement | null> = $state({});
  let metrics = $state.raw<Metrics>({ scroll: 0, view: 0, content: 0 });
  let boxes = $state.raw<Record<string, { left: number; width: number }>>({});
  /** Where the pointer was when the thumb of the scroll bar was taken, and the scroll then. */
  let held = $state<{ x: number; from: number } | null>(null);
  /** Where a tab being dragged is, for the edges of the strip to scroll. */
  let dragAt: { x: number; y: number } | null = null;
  let edgeFrame = 0;
  /** When the edges scrolled last: their speed is per 60th of a second, whatever the screen's rate. */
  let edgeTime = 0;
  /** The first tab brought into view (at start) is there at once; the next ones glide. */
  let glide = false;

  const tabs = $derived(app.projects.map((p) => ({ p, s: stats(p), active: app.ui.view === 'project' && app.ui.activeProject === p.id })));
  const sides = $derived(
    hiddenSides(
      tabs.flatMap(({ p, s, active }) => (boxes[p.id] ? [{ ...boxes[p.id], alert: active ? null : s.alert }] : [])),
      metrics,
    ),
  );
  const bar = $derived(thumb(metrics));

  function read(): Metrics {
    return { scroll: scroller!.scrollLeft, view: scroller!.clientWidth, content: scroller!.scrollWidth };
  }

  function measure() {
    if (!scroller) return;
    metrics = read();
    boxes = Object.fromEntries(
      Object.entries(tabEls).flatMap(([id, el]) => (el ? [[id, { left: el.offsetLeft, width: el.offsetWidth }]] : [])),
    );
  }

  /** The tab of the project on screen (none behind the statistics or « Vue d’ensemble »). */
  function activeTab(): HTMLElement | null {
    return app.ui.view === 'project' && app.ui.activeProject ? (tabEls[app.ui.activeProject] ?? null) : null;
  }

  // Measured again when the room or the tabs change size, and when the tabs change order. The tab on screen stays in
  // view when the room narrows or a badge widens it, unless it was scrolled out of view.
  $effect(() => {
    if (!scroller || !track) return;
    const ro = new ResizeObserver(() => {
      const box = app.ui.activeProject ? boxes[app.ui.activeProject] : undefined;
      const seen = !!box && box.left >= metrics.scroll - 0.5 && box.left + box.width <= metrics.scroll + metrics.view + 0.5;
      measure();
      const el = activeTab();
      if (seen && el) reveal(el, false);
    });
    ro.observe(scroller);
    ro.observe(track);
    return () => ro.disconnect();
  });
  $effect(() => {
    void app.projects.map((p) => p.id).join();
    untrack(measure);
  });

  // The tab of the project on screen comes into view, however it was chosen: a click, a notification, a shortcut,
  // an agent picked in « Vue d’ensemble », the project on screen chosen again.
  $effect(() => {
    void app.chosen;
    const el = activeTab();
    if (el && scroller) {
      untrack(() => reveal(el, glide && !window.matchMedia?.('(prefers-reduced-motion: reduce)').matches));
      glide = true;
    }
  });

  function reveal(el: HTMLElement, smooth: boolean) {
    const m = read();
    const to = revealScroll(m, el.offsetLeft, el.offsetWidth, FADE);
    if (to !== m.scroll) scroller!.scrollTo({ left: to, behavior: smooth ? 'smooth' : 'auto' });
  }

  // The bar is one row: the wheel scrolls it sideways.
  function onWheel(e: WheelEvent) {
    if (!scroller || scroller.scrollWidth <= scroller.clientWidth) return;
    const by = wheelScroll(e, scroller.clientWidth);
    if (by === null) return;
    e.preventDefault();
    scroller.scrollLeft += by;
  }

  // A tab dragged near an edge of the strip scrolls it, to be dropped among the tabs out of view.
  function onDragOver(e: DragEvent) {
    if (dragId === null) return;
    dragAt = { x: e.clientX, y: e.clientY };
    if (!edgeFrame) {
      edgeTime = 0;
      edgeFrame = requestAnimationFrame(scrollAtEdge);
    }
  }

  function scrollAtEdge(now: number) {
    edgeFrame = 0;
    if (dragId === null || !dragAt || !scroller) return;
    const frames = edgeTime ? Math.min(3, (now - edgeTime) / (1000 / 60)) : 1;
    edgeTime = now;
    const r = scroller.getBoundingClientRect();
    if (dragAt.y >= r.top && dragAt.y <= r.bottom) scroller.scrollLeft += edgeSpeed(dragAt.x, r.left, r.right) * frames;
    edgeFrame = requestAnimationFrame(scrollAtEdge);
  }

  $effect(() => () => cancelAnimationFrame(edgeFrame));

  function onThumbDown(e: PointerEvent) {
    if (e.button !== 0 || !scroller) return;
    e.preventDefault();
    held = { x: e.clientX, from: scroller.scrollLeft };
  }

  function onPointerMove(e: PointerEvent) {
    if (!held || !scroller) return;
    // Released out of the window, where its pointerup never came.
    if (!(e.buttons & 1)) return void (held = null);
    scroller.scrollLeft = thumbDrag(read(), held.from, e.clientX - held.x);
  }

  $effect(() => {
    win.isMaximized().then((m) => (maximized = m));
    const un = win.onResized(() => win.isMaximized().then((m) => (maximized = m)));
    return () => {
      un.then((f) => f()).catch(() => {});
    };
  });

  // The project on screen (none behind the statistics).
  const activeProject = $derived(app.ui.view === 'project' ? (app.projects.find((p) => p.id === app.ui.activeProject) ?? null) : null);

  function stats(p: Project) {
    const agents = Object.values(app.agents).filter((a) => a.projectId === p.id && !a.archived);
    const seen = app.attentionIn(p.id);
    // The most pressing kind sets the blink's color: an error, then a question, then an end.
    const alert = mostPressing(seen.map((a) => alertColor(shownStatus(a))));
    return {
      waiting: agents.filter((a) => shownStatus(a) === 'waiting').length,
      running: agents.some((a) => a.status === 'running'),
      changes: app.git[p.id]?.total ?? 0,
      alert,
      alertTitle: seen.length ? t('nav.titleBar.attention', { names: seen.map((a) => a.name).join(', ') }) : null,
    };
  }

  function tabMenu(e: MouseEvent, p: Project) {
    menu.show(e, [
      {
        label: t('nav.titleBar.menu.rename'),
        onClick: () => {
          app.modal = {
            kind: 'rename',
            title: t('nav.titleBar.menu.renameTitle'),
            value: p.name,
            onSubmit: (name) => save({ ...p, name }),
          };
        },
      },
      { label: t('common.color'), colors: { values: PROJECT_COLORS, selected: p.color, onPick: (color) => save({ ...p, color }) } },
      { label: '', separator: true },
      {
        label: p.worktreePerAgent ? t('nav.titleBar.menu.worktreeOff') : t('nav.titleBar.menu.worktreeOn'),
        hint: t('nav.titleBar.menu.worktreeHint'),
        onClick: () => save({ ...p, worktreePerAgent: !p.worktreePerAgent }),
      },
      {
        label: t('nav.titleBar.menu.settings'),
        onClick: () => {
          app.modal = { kind: 'settings', tab: 'projects', projectId: p.id };
        },
      },
      { label: t('nav.titleBar.menu.openFolder'), onClick: () => openPath(p.path).catch((err) => app.toast(String(err), 'error')) },
      { label: '', separator: true },
      { label: t('nav.titleBar.menu.close'), danger: true, onClick: () => askCloseProject(p) },
    ]);
  }

  async function save(p: Project) {
    const i = app.projects.findIndex((x) => x.id === p.id);
    if (i >= 0) app.projects[i] = p;
    await app.run(api.updateProject(p));
  }

  function onDrop() {
    if (dragId === null || dropIndex === null) return;
    const list = [...app.projects];
    const from = list.findIndex((p) => p.id === dragId);
    const [moved] = list.splice(from, 1);
    list.splice(dropIndex > from ? dropIndex - 1 : dropIndex, 0, moved);
    app.projects = list;
    app.run(api.reorderProjects(list.map((p) => p.id)));
    ((dragId = null), (dropIndex = null));
  }
</script>

<svelte:window
  ondragover={onDragOver}
  onpointermove={onPointerMove}
  onpointerup={() => (held = null)}
  onpointercancel={() => (held = null)}
/>

<header class="bar" class:mac={IS_MAC} data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img class="mark" src="/logo.svg" alt="Escouade" draggable="false" />
  </div>
  <!-- Every project's agents: before the tabs of the projects, apart from them. -->
  <div class="all" data-tauri-drag-region>
    <button
      class="tab stats"
      class:active={app.ui.view === 'overview'}
      aria-pressed={app.ui.view === 'overview'}
      use:tip={t('nav.titleBar.overviewTitle', { shortcut: keyLabel('Ctrl+Shift+A') })}
      onclick={() => app.toggleOverview()}
    >
      <svg
        width="14"
        height="12"
        viewBox="0 0 14 12"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        aria-hidden="true"
        ><path d="M5.5 2h7.5M5.5 6h7.5M5.5 10h7.5" /><circle cx="1.75" cy="2" r="1.1" fill="currentColor" stroke="none" /><circle
          cx="1.75"
          cy="6"
          r="1.1"
          fill="currentColor"
          stroke="none"
        /><circle cx="1.75" cy="10" r="1.1" fill="currentColor" stroke="none" /></svg
      >
      {t('common.overview')}
    </button>
  </div>
  <nav class="tabs" data-tauri-drag-region>
    <!-- The tabs scroll in their strip; « + » stays out of it, always in view. -->
    <div class="strip" style:--fade="{FADE}px" onwheel={onWheel}>
      <!-- The middle button's press would start the browser's autoscroll in the strip (Chromium): its click opens the
           menu of a tab. Not an interaction of its own: the tabs in it are buttons. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="scroller"
        bind:this={scroller}
        onscroll={measure}
        onmousedown={(e) => e.button === 1 && e.preventDefault()}
        data-tauri-drag-region
      >
        <div class="track" bind:this={track} data-tauri-drag-region>
          {#each tabs as { p, s, active }, i (p.id)}
            <button
              class="tab"
              class:active
              class:alert={!!s.alert && !active}
              class:drop-before={dropIndex === i && dragId !== p.id}
              style:--tab-color={p.color}
              style:--alert={s.alert}
              draggable="true"
              bind:this={tabEls[p.id]}
              use:tip={s.alertTitle && !active ? `${s.alertTitle}\n${p.path}` : p.path}
              onclick={() => app.selectProject(p.id)}
              oncontextmenu={(e) => tabMenu(e, p)}
              onauxclick={(e) => e.button === 1 && tabMenu(e, p)}
              ondragstart={(e) => {
                dragId = p.id;
                e.dataTransfer?.setData('text/plain', p.id);
              }}
              ondragover={(e) => {
                e.preventDefault();
                const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                dropIndex = e.clientX < r.left + r.width / 2 ? i : i + 1;
              }}
              ondrop={(e) => {
                e.preventDefault();
                onDrop();
              }}
              ondragend={() => ((dragId = null), (dropIndex = null))}
            >
              <span class="swatch" style:background={p.color}></span>
              <span class="name">{p.name}</span>
              {#if s.running && !s.waiting}<span class="dot" style="width:6px;height:6px;background:var(--ok)"></span>{/if}
              {#if s.changes > 0}
                <span class="delta" use:tip={t('nav.titleBar.delta')}>Δ {s.changes}</span>
              {/if}
              {#if s.waiting > 0}<span class="pill" use:tip={t('nav.titleBar.waiting')}>{s.waiting}</span>{/if}
            </button>
          {/each}
        </div>
      </div>
      <!-- Tabs out of view at an edge: a fade, in the color of a blink among them. -->
      {#if sides.left.hidden}
        <div class="fade left" class:alert={!!sides.left.alert} style:--alert={sides.left.alert} aria-hidden="true"></div>
      {/if}
      {#if sides.right.hidden}
        <div class="fade right" class:alert={!!sides.right.alert} style:--alert={sides.right.alert} aria-hidden="true"></div>
      {/if}
      {#if bar}
        <div
          class="thumb"
          class:held={!!held}
          style:left="{bar.left}px"
          style:width="{bar.width}px"
          onpointerdown={onThumbDown}
          aria-hidden="true"
        ></div>
      {/if}
    </div>
    <button
      class="add"
      use:tip={t('nav.titleBar.addProject')}
      aria-label={t('nav.titleBar.addProject')}
      onclick={() => {
        app.modal = { kind: 'newProject' };
      }}>+</button
    >
  </nav>
  <div class="spacer" data-tauri-drag-region></div>
  <div class="right">
    {#if activeProject}
      <button
        class="tab stats"
        aria-label={t('nav.titleBar.openEditor')}
        use:tip={t('nav.titleBar.openEditorTitle')}
        onclick={() => app.openEditor({ projectId: activeProject.id, source: 'project' })}
      >
        <svg width="14" height="12" viewBox="0 0 14 12" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"
          ><path d="M4.5 2.5 1 6l3.5 3.5M9.5 2.5 13 6l-3.5 3.5" /></svg
        >
        {t('common.editor')}
      </button>
    {/if}
    <button class="tab stats" class:active={app.ui.view === 'stats'} onclick={() => app.openStats()}>
      <span class="bars"><span style="height:6px"></span><span style="height:12px"></span><span style="height:9px"></span></span>
      {t('nav.titleBar.stats')}
    </button>
  </div>
  <!-- macOS draws its own buttons (traffic lights) at the left of the bar. -->
  {#if !IS_MAC}
    <div class="controls">
      <button class="ctl" use:tip={t('nav.titleBar.minimize')} aria-label={t('nav.titleBar.minimize')} onclick={() => win.minimize()}>
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1" /></svg>
      </button>
      <button
        class="ctl"
        use:tip={maximized ? t('nav.titleBar.restore') : t('nav.titleBar.maximize')}
        aria-label={t('nav.titleBar.maximize')}
        onclick={() => win.toggleMaximize()}
      >
        {#if maximized}
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1"
            ><path d="M2.5 2.5V.5h7v7h-2" /><rect x=".5" y="2.5" width="7" height="7" /></svg
          >
        {:else}
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1"
            ><rect x=".5" y=".5" width="9" height="9" /></svg
          >
        {/if}
      </button>
      <button class="ctl close" use:tip={t('nav.titleBar.closeWindow')} aria-label={t('common.close')} onclick={() => win.hide()}>
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" /></svg>
      </button>
    </div>
  {/if}
</header>

<style>
  .bar {
    height: 46px;
    flex: none;
    display: flex;
    align-items: stretch;
    padding: 0 0 0 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--line2);
    user-select: none;
  }
  /* Room for the traffic lights (window buttons drawn by macOS over the bar). */
  .bar.mac {
    padding-left: 84px;
    padding-right: 8px;
  }
  .brand {
    display: flex;
    align-items: center;
    padding-right: 14px;
  }
  .mark {
    width: 24px;
    height: 24px;
    display: block;
    pointer-events: none;
  }
  .all {
    flex: none;
    display: flex;
    align-items: flex-end;
  }
  /* A rule between the view of all the projects and the tabs of each. */
  .all::after {
    content: '';
    width: 1px;
    height: 18px;
    margin: 0 8px 10px;
    background: var(--line2);
  }
  .tabs {
    display: flex;
    align-items: flex-end;
    min-width: 0;
  }
  /* Over the bar's bottom border, which the tab on screen covers to join the view under it. */
  .strip {
    position: relative;
    display: flex;
    align-self: stretch;
    min-width: 0;
    margin-bottom: -1px;
  }
  /* Its own scroll bar is the thin one drawn over it (.thumb): the browser's would take room under the tabs. */
  .scroller {
    display: flex;
    align-items: flex-end;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    /* A tab reached by the keyboard is left clear of the fades. */
    scroll-padding-inline: var(--fade);
  }
  .scroller::-webkit-scrollbar {
    display: none;
  }
  .track {
    position: relative;
    flex: none;
    display: flex;
    align-items: flex-end;
    gap: 2px;
  }
  .track .tab {
    margin-bottom: 0;
  }
  /* Over the tabs, not over the bar's bottom border. */
  .fade {
    position: absolute;
    bottom: 1px;
    width: var(--fade);
    height: 37px;
    pointer-events: none;
  }
  .fade.left {
    left: 0;
    background: linear-gradient(to right, var(--panel) 15%, transparent);
  }
  .fade.right {
    right: 0;
    background: linear-gradient(to left, var(--panel) 15%, transparent);
  }
  /* A tab out of view blinks: its color at the edge, blinking with it. */
  .fade.alert::after {
    content: '';
    position: absolute;
    inset: 0;
    animation: fadeAlert 1.1s ease-in-out infinite alternate;
  }
  .fade.left.alert::after {
    background: linear-gradient(to right, color-mix(in oklch, var(--alert) 60%, transparent), transparent);
    box-shadow: inset 2px 0 0 var(--alert);
  }
  .fade.right.alert::after {
    background: linear-gradient(to left, color-mix(in oklch, var(--alert) 60%, transparent), transparent);
    box-shadow: inset -2px 0 0 var(--alert);
  }
  @keyframes fadeAlert {
    from {
      opacity: 0.35;
    }
    to {
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fade.alert::after {
      animation: none;
      opacity: 0.8;
    }
  }
  /* The thin scroll bar, shown while the pointer is over the tabs: on the bar's bottom border and the very bottom of the
     tabs, which it takes from them only there. */
  .thumb {
    position: absolute;
    bottom: 0;
    height: 4px;
    display: flex;
    align-items: flex-end;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .thumb::after {
    content: '';
    flex: 1;
    height: 3px;
    border-radius: 3px;
    background: color-mix(in oklch, var(--muted) 45%, transparent);
    transition: height 0.1s;
  }
  .strip:hover .thumb,
  .thumb.held {
    opacity: 1;
  }
  .thumb:hover::after,
  .thumb.held::after {
    height: 4px;
    background: color-mix(in oklch, var(--muted) 70%, transparent);
  }
  .tab {
    height: 37px;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 0 14px;
    border-width: 1px 1px 0 1px;
    border-style: solid;
    border-color: transparent;
    border-top: 2px solid transparent;
    border-radius: var(--r) var(--r) 0 0;
    background: transparent;
    color: var(--muted);
    font-size: 13px;
    cursor: pointer;
    margin-bottom: -1px;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .tab:hover:not(.active) {
    color: var(--text);
    background: color-mix(in oklch, var(--elev) 50%, transparent);
  }
  .tab.active {
    background: var(--bg);
    border-color: var(--line2);
    border-top-color: var(--tab-color, transparent);
    color: var(--text);
  }
  /* A project with an agent to look at blinks until that agent is seen. */
  .tab.alert {
    color: var(--text);
    border-top-color: var(--alert);
    animation: tabAlert 1.1s ease-in-out infinite alternate;
  }
  @keyframes tabAlert {
    from {
      background: transparent;
    }
    to {
      background: color-mix(in oklch, var(--alert) 26%, transparent);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .tab.alert {
      animation: none;
      background: color-mix(in oklch, var(--alert) 20%, transparent);
    }
  }
  .tab.stats.active {
    border-top-color: var(--line2);
  }
  .drop-before {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 3px;
    flex: none;
  }
  .name {
    font-weight: 600;
  }
  .delta {
    font-family: var(--mono);
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 99px;
    background: var(--elev2);
    color: var(--muted);
  }
  .add {
    height: 28px;
    width: 28px;
    margin: 0 0 5px 6px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--muted);
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
    flex: none;
  }
  .add:hover {
    background: var(--elev2);
    color: var(--text);
  }
  .spacer {
    flex: 1;
    min-width: 24px;
  }
  .right {
    display: flex;
    align-items: flex-end;
    padding-right: 10px;
  }
  .stats {
    font-weight: 600;
    gap: 8px;
    padding: 0 16px;
  }
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 12px;
  }
  .bars span {
    width: 3px;
    background: currentColor;
    border-radius: 1px;
  }
  .controls {
    display: flex;
    align-items: stretch;
  }
  .ctl {
    width: 46px;
    border: none;
    background: transparent;
    color: var(--muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .ctl:hover {
    background: var(--elev2);
    color: var(--text);
  }
  .ctl.close:hover {
    background: #c42b1c;
    color: #fff;
  }
</style>
