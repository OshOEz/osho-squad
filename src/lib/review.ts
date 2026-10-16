// The reviews agents publish (`show_review`): what the window shows of them.

import type { Agent, AgentStatus, ConvItem, ReviewAnswers, ReviewItem } from './types';

/** The status an agent shows: « waiting » while a review it published waits for the user's answers, its turn over. */
export function shownStatus(a: Agent): AgentStatus {
  return a.reviews.length && (a.status === 'done' || a.status === 'idle') ? 'waiting' : a.status;
}

/** The review the side panel shows: the one chosen (its card clicked), else the latest that waits. */
export function shownReview(items: ConvItem[], chosen: string | undefined): ReviewItem | null {
  const reviews = items.filter((i): i is ReviewItem => i.kind === 'review');
  return reviews.find((r) => r.id === chosen) ?? [...reviews].reverse().find((r) => r.statut === 'attente') ?? null;
}

/** The answers a page starts from: those kept (a send that failed, or its answers once answered), else each recommended choice. */
export function initialAnswers(item: ReviewItem): ReviewAnswers {
  if (item.reponses) return { questions: { ...item.reponses.questions }, note: item.reponses.note };
  const questions: Record<string, string> = {};
  for (const q of item.contenu.questions) {
    const reco = q.choix.find((c) => c.reco);
    if (reco) questions[q.id] = reco.valeur;
  }
  return { questions, note: '' };
}
