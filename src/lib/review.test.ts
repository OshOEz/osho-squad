import { describe, expect, it } from 'vitest';
import { agent, reviewItem } from '../test/ipc';
import { initialAnswers, shownReview, shownStatus } from './review';
import type { ConvItem } from './types';

describe('reviews', () => {
  it('shows an agent whose review waits as waiting, once its turn is over', () => {
    expect(shownStatus(agent({ status: 'done', reviews: ['rv1'] }))).toBe('waiting');
    expect(shownStatus(agent({ status: 'idle', reviews: ['rv1'] }))).toBe('waiting');
    // At work or failed, it says so.
    expect(shownStatus(agent({ status: 'running', reviews: ['rv1'] }))).toBe('running');
    expect(shownStatus(agent({ status: 'error', reviews: ['rv1'] }))).toBe('error');
    expect(shownStatus(agent({ status: 'done' }))).toBe('done');
  });

  it('picks the review chosen, else the latest that waits', () => {
    const items: ConvItem[] = [
      reviewItem({ id: 'rv1' }),
      { kind: 'notice', id: 'n1', ts: 2, level: 'info', text: 'x' },
      reviewItem({ id: 'rv2' }),
      reviewItem({ id: 'rv3', statut: 'repondue' }),
    ];
    expect(shownReview(items, undefined)?.id).toBe('rv2');
    expect(shownReview(items, 'rv3')?.id).toBe('rv3');
    // A choice that is no review of this conversation (another agent's) is not shown.
    expect(shownReview(items, 'zzz')?.id).toBe('rv2');
    expect(shownReview([reviewItem({ statut: 'repondue' })], undefined)).toBeNull();
  });

  it('starts from the recommendation, else from the answers kept', () => {
    expect(initialAnswers(reviewItem())).toEqual({ questions: { q1: 'sub' }, note: '' });
    const kept = { questions: { q1: 'inline', q2: 'lexer' }, note: 'Merci' };
    expect(initialAnswers(reviewItem({ reponses: kept }))).toEqual(kept);
  });
});
