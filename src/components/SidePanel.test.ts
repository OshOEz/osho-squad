import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { applyConvOps } from '../lib/conversations.svelte';
import { setLang } from '../lib/i18n';
import { app } from '../lib/state.svelte';
import { agent, fakeBackend, project, resetApp, reviewItem } from '../test/ipc';
import SidePanel from './SidePanel.svelte';
import { getByTip, queryByTip } from '../test/tip';

const change = (path: string) => ({ path, status: 'M', add: 1, del: 0, agentId: 'a1' });
const LOG = {
  commits: [{ hash: 'aaaa1111', parents: [], author: 'Ada', time: 1790000000, refs: ['HEAD', 'main'], subject: 'premier commit' }],
  head: 'main',
};

describe('SidePanel', () => {
  beforeEach(() => resetApp({ projects: [project()], agents: [agent()] }));

  it('switches between the uncommitted files and the history', async () => {
    fakeBackend({ git_files: () => [change('src/a.ts'), change('src/b.ts')], git_log: () => LOG });
    render(SidePanel, { project: project(), agent: app.agents.a1, docked: true });
    expect(await screen.findByRole('tab', { name: /Non commités\s*2/ })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('a.ts')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('tab', { name: 'Historique' }));
    expect(await screen.findByText('premier commit')).toBeInTheDocument();
    expect(screen.queryByText('a.ts')).not.toBeInTheDocument();
    expect(app.panelTab).toBe('history');
  });

  it('counts every uncommitted file in its tab, whatever the list shows', async () => {
    fakeBackend({ git_files: () => Array.from({ length: 620 }, (_, i) => change(`src/f${i}.ts`)), git_log: () => LOG });
    render(SidePanel, { project: project(), agent: app.agents.a1, docked: true });
    expect(await screen.findByRole('tab', { name: /Non commités\s*620/ })).toBeInTheDocument();
  });

  it('can be closed in the classic layout, where it is optional', async () => {
    fakeBackend({ git_files: () => [] });
    app.filesOpen = true;
    render(SidePanel, { project: project(), agent: app.agents.a1 });
    await userEvent.click(getByTip(document.body, 'Fermer'));
    expect(app.filesOpen).toBe(false);
  });

  it('cannot be closed in the split layout', () => {
    fakeBackend({ git_files: () => [] });
    render(SidePanel, { project: project(), agent: app.agents.a1, docked: true });
    expect(queryByTip(document.body, 'Fermer')).not.toBeInTheDocument();
  });

  it('shows the latest review that waits, else the one its card chose', async () => {
    const a9 = agent({ id: 'a9', reviews: ['rv2'] });
    app.agents.a9 = a9;
    fakeBackend({
      git_files: () => [],
      get_conversation: () => [
        reviewItem({ id: 'rv1', statut: 'repondue', contenu: { ...reviewItem().contenu, titre: 'Spec' } }),
        reviewItem({ id: 'rv2', contenu: { ...reviewItem().contenu, titre: 'Plan' } }),
      ],
    });
    app.panelTab = 'review';
    render(SidePanel, { project: project(), agent: app.agents.a9, docked: true });
    expect(screen.getByRole('tab', { name: /Revue\s*1/ })).toHaveAttribute('aria-selected', 'true');
    expect(await screen.findByRole('heading', { name: 'Plan' })).toBeInTheDocument();
    app.reviewShown = { a9: 'rv1' };
    expect(await screen.findByRole('heading', { name: 'Spec' })).toBeInTheDocument();
  });

  it('keeps the answered page shown when the tab was opened with no review chosen', async () => {
    app.agents.a7 = agent({ id: 'a7', reviews: ['rv1'] });
    fakeBackend({
      git_files: () => [],
      get_conversation: () => [reviewItem()],
      // The agent's side: the review is patched to answered.
      answer_review: () => {
        applyConvOps('a7', [{ op: 'patch', id: 'rv1', patch: { statut: 'repondue' } }]);
      },
    });
    app.panelTab = 'review';
    render(SidePanel, { project: project(), agent: app.agents.a7, docked: true });
    await userEvent.click(await screen.findByRole('button', { name: 'Envoyer' }));
    expect(await screen.findByText('répondue')).toBeInTheDocument();
    expect(screen.queryByText('Aucune revue à afficher pour cet agent.')).not.toBeInTheDocument();
  });

  it('says when the agent has no review', async () => {
    fakeBackend({ git_files: () => [], get_conversation: () => [] });
    app.agents.a8 = agent({ id: 'a8' });
    app.panelTab = 'review';
    render(SidePanel, { project: project(), agent: app.agents.a8, docked: true });
    expect(await screen.findByText('Aucune revue à afficher pour cet agent.')).toBeInTheDocument();
  });
});

describe('SidePanel in English', () => {
  beforeEach(() => {
    resetApp({ projects: [project()], agents: [agent()] });
    setLang('en');
  });

  it('names its two tabs and its close button in English', async () => {
    fakeBackend({ git_files: () => [change('src/a.ts'), change('src/b.ts')], git_log: () => LOG });
    render(SidePanel, { project: project(), agent: app.agents.a1 });
    expect(await screen.findByRole('tab', { name: /Uncommitted\s*2/ })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: 'History' })).toBeInTheDocument();
    expect(getByTip(document.body, 'Close')).toBeInTheDocument();
  });
});
