<script lang="ts">
  import { t } from '../../lib/i18n';
  import { app } from '../../lib/state.svelte';
  import type { ReviewItem } from '../../lib/types';

  // A review's card in the conversation: its title and whether it waits; a click opens its page on the right.
  let { item, agentId }: { item: ReviewItem; agentId: string } = $props();
  const waiting = $derived(item.statut === 'attente');
</script>

<button class="card" class:waiting onclick={() => app.openReview(agentId, item.id)}>
  <span class="title">{t('conv.review.card', { title: item.contenu.titre })}</span>
  <span class="state">{waiting ? t('conv.review.waiting') : t('conv.review.answered')}</span>
</button>

<style>
  .card {
    margin-left: 34px;
    display: flex;
    align-items: center;
    gap: 10px;
    width: calc(100% - 34px);
    padding: 10px 14px;
    border: 1px solid var(--line);
    border-left: 3px solid var(--accent);
    border-radius: var(--r);
    background: var(--elev);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .card.waiting {
    border-color: var(--wait);
    border-left-color: var(--wait);
    background: var(--wait-soft);
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    font-size: 11px;
    color: var(--muted);
  }
  .waiting .state {
    color: var(--wait);
  }
</style>
