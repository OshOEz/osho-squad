<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import { api } from '../lib/ipc';
  import { initialAnswers } from '../lib/review';
  import { app } from '../lib/state.svelte';
  import type { ReviewItem } from '../lib/types';

  // A review an agent published (`show_review`), answered here once and read-only after. Mounted under `{#key item.id}`:
  // the answers start from its item when it mounts. Its texts are shown as written, never as Markdown or HTML.
  let { item, agentId }: { item: ReviewItem; agentId: string } = $props();

  const start = untrack(() => initialAnswers(item));
  let choices = $state<Record<string, string>>(start.questions);
  let note = $state(start.note);
  let sending = $state(false);
  const c = $derived(item.contenu);
  const answered = $derived(item.statut === 'repondue');

  async function send() {
    sending = true;
    // Keep this page shown once answered, even when the tab was opened with no review chosen.
    app.reviewShown[agentId] = item.id;
    await app.run(api.answerReview(agentId, item.id, { questions: $state.snapshot(choices), note }));
    sending = false;
  }
</script>

<div class="review">
  <header>
    {#if c.sur}<span class="sur">{c.sur}</span>{/if}
    <h2>{c.titre}</h2>
    <span class="state" class:done={answered}>{answered ? t('conv.review.answered') : t('conv.review.waiting')}</span>
  </header>
  {#if c.chapo}<p class="chapo">{c.chapo}</p>{/if}
  {#if c.puces.length}
    <ul class="puces">
      {#each c.puces as p, i (i)}<li class:alerte={p.ton === 'alerte'}>{p.texte}</li>{/each}
    </ul>
  {/if}
  {#each c.sections as s, i (i)}
    <section class:alerte={s.ton === 'alerte'}>
      <h3>{s.titre}</h3>
      {#each s.paragraphes as para, j (j)}<p>{para}</p>{/each}
      {#if s.items.length}
        <ul>
          {#each s.items as it, j (j)}<li>{it}</li>{/each}
        </ul>
      {/if}
    </section>
  {/each}
  <h3 class="qhead">{c.titreQuestions ?? t('conv.review.questions')}</h3>
  {#each c.questions as q (q.id)}
    <fieldset class="q" disabled={answered || sending}>
      <legend>{q.titre}</legend>
      {#if q.contexte}<p class="ctx">{q.contexte}</p>{/if}
      {#if q.choix.length}
        {#each q.choix as ch, ci (ci)}
          <label class="opt">
            <input type="radio" name="{item.id}-{q.id}" value={ch.valeur} bind:group={choices[q.id]} />
            <span class="lib">{ch.libelle}</span>
            {#if ch.reco}<span class="reco">{t('conv.review.recommended')}</span>{/if}
            {#if ch.detail}<span class="detail">{ch.detail}</span>{/if}
          </label>
        {/each}
      {:else}
        <textarea rows="3" aria-label={q.titre} placeholder={q.placeholder ?? ''} bind:value={choices[q.id]}></textarea>
      {/if}
    </fieldset>
  {/each}
  <label class="note">
    <span>{t('conv.review.note')}</span>
    <textarea rows="3" bind:value={note} disabled={answered || sending}></textarea>
  </label>
  {#if !answered}
    <button class="btn primary send" disabled={sending} onclick={send}>{t('conv.review.send')}</button>
  {/if}
</div>

<style>
  .review {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 18px 18px;
    color: var(--text);
    font-size: 13px;
    line-height: 1.55;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-left: 3px solid var(--accent);
    padding-left: 10px;
  }
  .sur {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  .state {
    align-self: flex-start;
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 99px;
    background: var(--wait-soft);
    color: var(--wait);
  }
  .state.done {
    background: var(--accent-soft);
    color: var(--text);
  }
  .chapo,
  p,
  li,
  .detail,
  .ctx {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .chapo {
    margin: 0;
    font-size: 14px;
  }
  ul {
    margin: 0;
    padding-left: 18px;
  }
  .alerte {
    color: var(--del);
  }
  section {
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }
  section.alerte {
    border-top-color: var(--del);
  }
  h3 {
    margin: 0 0 4px;
    font-size: 13px;
  }
  .q {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--elev);
  }
  legend {
    font-weight: 600;
    padding: 0 4px;
  }
  .ctx {
    margin: 0;
    color: var(--muted);
  }
  .opt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .opt input {
    accent-color: var(--accent);
  }
  .reco {
    font-size: 10.5px;
    padding: 1px 7px;
    border-radius: 99px;
    background: var(--accent-soft);
    color: var(--text);
  }
  .detail {
    flex-basis: 100%;
    padding-left: 22px;
    color: var(--dim);
  }
  textarea {
    width: 100%;
    resize: vertical;
    font: inherit;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    padding: 6px 8px;
  }
  .note {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-weight: 600;
  }
  .send {
    align-self: flex-end;
  }
</style>
