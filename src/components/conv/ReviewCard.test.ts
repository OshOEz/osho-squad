import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { app } from '../../lib/state.svelte';
import { fakeBackend, resetApp, reviewItem } from '../../test/ipc';
import ReviewCard from './ReviewCard.svelte';

describe('ReviewCard', () => {
  beforeEach(() => resetApp());

  it('says whether the review waits and opens its page in the right half', async () => {
    fakeBackend();
    render(ReviewCard, { item: reviewItem(), agentId: 'a1' });
    const card = screen.getByRole('button', { name: /Revue : Plan du parseur/ });
    expect(card).toHaveTextContent('en attente');
    await userEvent.click(card);
    expect(app.panelTab).toBe('review');
    expect(app.reviewShown).toEqual({ a1: 'rv1' });
    expect(app.split).toBe(true);
  });

  it('says once it is answered', () => {
    fakeBackend();
    render(ReviewCard, { item: reviewItem({ statut: 'repondue' }), agentId: 'a1' });
    expect(screen.getByRole('button', { name: /Revue : Plan du parseur/ })).toHaveTextContent('répondue');
  });
});
