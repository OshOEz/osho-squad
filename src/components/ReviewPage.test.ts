import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { setLang } from '../lib/i18n';
import { app } from '../lib/state.svelte';
import { fakeBackend, resetApp, reviewItem } from '../test/ipc';
import ReviewPage from './ReviewPage.svelte';

describe('ReviewPage', () => {
  beforeEach(() => resetApp());

  it('shows the page and preselects the recommended choice', () => {
    fakeBackend();
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    expect(screen.getByRole('heading', { name: 'Plan du parseur' })).toBeInTheDocument();
    expect(screen.getByText('Trois lots.')).toBeInTheDocument();
    expect(screen.getByText('Migration risquée')).toHaveClass('alerte');
    expect(screen.getByText('Tests verts')).not.toHaveClass('alerte');
    expect(screen.getByText('Le lot 1 crée le lexer.')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Sous-agents/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /Ici/ })).not.toBeChecked();
    expect(screen.getByText('recommandé')).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Nom du module ?' })).toHaveAttribute('placeholder', 'parser');
    expect(screen.getByText('en attente')).toBeInTheDocument();
  });

  it('sends the choices, the texts and the note', async () => {
    const backend = fakeBackend();
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    await userEvent.click(screen.getByRole('radio', { name: /Ici/ }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Nom du module ?' }), 'lexer');
    await userEvent.type(screen.getByRole('textbox', { name: 'Autre chose ?' }), 'Merci');
    await userEvent.click(screen.getByRole('button', { name: 'Envoyer' }));
    expect(backend.called('answer_review')).toEqual([
      {
        cmd: 'answer_review',
        args: { agentId: 'a1', reviewId: 'rv1', answers: { questions: { q1: 'inline', q2: 'lexer' }, note: 'Merci' } },
      },
    ]);
  });

  it('is read-only once answered, with its answers', () => {
    fakeBackend();
    const reponses = { questions: { q1: 'inline', q2: 'lexer' }, note: 'Merci' };
    render(ReviewPage, { item: reviewItem({ statut: 'repondue', reponses }), agentId: 'a1' });
    expect(screen.getByRole('radio', { name: /Ici/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /Ici/ })).toBeDisabled();
    expect(screen.getByRole('textbox', { name: 'Nom du module ?' })).toHaveValue('lexer');
    expect(screen.getByRole('textbox', { name: 'Autre chose ?' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Envoyer' })).not.toBeInTheDocument();
    expect(screen.getByText('répondue')).toBeInTheDocument();
  });

  it('tells a send that failed, keeps the answers and can send again', async () => {
    fakeBackend({
      answer_review: () => {
        throw 'L’agent est en vue Claude Code : ouvre-le pour démarrer son terminal.';
      },
    });
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    await userEvent.click(screen.getByRole('radio', { name: /Ici/ }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Nom du module ?' }), 'lexer');
    await userEvent.type(screen.getByRole('textbox', { name: 'Autre chose ?' }), 'Merci');
    const send = screen.getByRole('button', { name: 'Envoyer' });
    await userEvent.click(send);
    await waitFor(() => expect(app.toasts.at(-1)).toMatchObject({ kind: 'error', text: expect.stringMatching(/vue Claude Code/) }));
    await waitFor(() => expect(send).toBeEnabled());
    expect(screen.getByRole('radio', { name: /Ici/ })).toBeChecked();
    expect(screen.getByRole('textbox', { name: 'Nom du module ?' })).toHaveValue('lexer');
    expect(screen.getByRole('textbox', { name: 'Autre chose ?' })).toHaveValue('Merci');
  });

  it('disables the send button while sending, so a double click sends once', async () => {
    let release!: () => void;
    const backend = fakeBackend({ answer_review: () => new Promise<void>((r) => (release = r)) });
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    const send = screen.getByRole('button', { name: 'Envoyer' });
    await userEvent.click(send);
    await waitFor(() => expect(send).toBeDisabled());
    await userEvent.click(send);
    expect(backend.called('answer_review')).toHaveLength(1);
    release();
    await waitFor(() => expect(send).toBeEnabled());
    expect(backend.called('answer_review')).toHaveLength(1);
  });

  it('survives two choices of the same value', () => {
    fakeBackend();
    const item = reviewItem();
    item.contenu.questions[0].choix[1].valeur = 'sub';
    render(ReviewPage, { item, agentId: 'a1' });
    expect(screen.getAllByRole('radio')).toHaveLength(2);
  });

  it('speaks English', () => {
    setLang('en');
    fakeBackend();
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    expect(screen.getByRole('button', { name: 'Send' })).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Anything else?' })).toBeInTheDocument();
    expect(screen.getByText('recommended')).toBeInTheDocument();
  });
});
