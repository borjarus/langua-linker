import { fireEvent, screen } from '@testing-library/svelte'
import { describe, expect, it } from 'vitest'
import type { AssociationDto } from '../src/api/types'
import AssociationPanel from '../src/components/AssociationPanel.svelte'
import { createTestServices, renderWith } from './helpers/services'

const association: AssociationDto = {
  id: 'a1',
  noteId: 'n1',
  text: 'Gebirge – zbiór gór',
  provenance: 'bundled',
  favorite: true,
  status: 'ready',
  model: null,
  error: null,
  extended: '## 1. Rozkład dźwiękowy\n\n**Gebirge** brzmi jak *ge-birge*\n\n- **Ge** – zbiór\n\n> **„Wir wandern im Gebirge.”**\n\n<b>raw</b>',
}

describe('extended association', () => {
  it('starts collapsed and opens into rendered Markdown without HTML', async () => {
    const services = await createTestServices({ language: 'en' })
    const { container } = renderWith(AssociationPanel, services, { associations: [association], expanded: true })
    const toggle = screen.getByRole('button', { name: 'Show extended version' })
    expect(toggle.getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByRole('heading', { name: '1. Rozkład dźwiękowy' })).toBeNull()

    await fireEvent.click(toggle)
    expect(screen.getByRole('button', { name: 'Hide extended version' }).getAttribute('aria-expanded')).toBe('true')
    expect(screen.getByRole('heading', { level: 4, name: '1. Rozkład dźwiękowy' })).toBeTruthy()
    expect(container.querySelector('.extended strong')?.textContent).toBe('Gebirge')
    expect(container.querySelector('.extended blockquote')?.textContent).toContain('Wir wandern im Gebirge.')
    expect(container.querySelector('.extended b')).toBeNull()
    expect(screen.getByText('<b>raw</b>')).toBeTruthy()
  })

  it('shows no toggle without an extended version', async () => {
    const services = await createTestServices({ language: 'en' })
    renderWith(AssociationPanel, services, { associations: [{ ...association, extended: undefined }], expanded: true })
    expect(screen.queryByRole('button', { name: 'Show extended version' })).toBeNull()
  })
})
