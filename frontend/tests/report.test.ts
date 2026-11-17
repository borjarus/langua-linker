import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fireEvent, screen, waitFor } from '@testing-library/svelte'
import { describe, expect, it } from 'vitest'
import type { AssociationDto } from '../src/api/types'
import AssociationPanel from '../src/components/AssociationPanel.svelte'
import { REPORT_EMAIL, reportBody, reportMailto, reportable, type AssociationReport } from '../src/services/report'
import { createTestServices, renderWith } from './helpers/services'

const association = (id: string, provenance: AssociationDto['provenance'], status: AssociationDto['status'] = 'ready'): AssociationDto => ({
  id,
  noteId: 'n1',
  text: `Text ${id}`,
  provenance,
  favorite: false,
  status,
  model: provenance === 'generated' ? 'model-x' : null,
  error: null,
})

const card = { front: 'der Bahnhof', back: 'dworzec', targetLanguage: 'de' as const }

const report = (overrides: Partial<AssociationReport> = {}): AssociationReport => ({
  reason: 'offensive',
  association: { text: 'Bahn — ban', provenance: 'generated', model: 'model-x' },
  card,
  appVersion: '0.4.0-beta.1',
  comment: '',
  ...overrides,
})

describe('association report (Play AI-generated content policy)', () => {
  it('goes to the same address the Rust navigation guard lets through', () => {
    const rust = readFileSync(resolve(process.cwd(), '../src-tauri/src/links.rs'), 'utf8')
    expect(rust).toContain(`pub const REPORT_EMAIL: &str = "${REPORT_EMAIL}";`)
  })

  it('reports only generated and bundled associations that are ready', () => {
    expect(reportable(association('a', 'generated'))).toBe(true)
    expect(reportable(association('b', 'bundled'))).toBe(true)
    expect(reportable(association('c', 'manual'))).toBe(false)
    expect(reportable(association('d', 'generated', 'blocked'))).toBe(false)
  })

  it('the message holds the association, the card, the reason and the app version only', () => {
    const body = reportBody(report({ comment: '  not   funny ' }))
    expect(body).toContain('Reason: offensive')
    expect(body).toContain('Association (generated, model-x):\nBahn — ban')
    expect(body).toContain('Front: der Bahnhof\nBack: dworzec')
    expect(body).toContain('Comment: not funny')
    expect(body).toContain('App: 0.4.0-beta.1')
  })

  it('builds a mailto link with one recipient and encoded subject and body', () => {
    const url = new URL(reportMailto(report({ card: { ...card, front: 'Füße & co?' } })))
    expect(url.protocol).toBe('mailto:')
    expect(url.pathname).toBe(REPORT_EMAIL)
    expect([...url.searchParams.keys()]).toEqual(['subject', 'body'])
    expect(url.searchParams.get('body')).toContain('Front: Füße & co?')
  })

  it('cuts long fields so the link stays short', () => {
    const long = 'x'.repeat(5000)
    const url = reportMailto(report({ comment: long, association: { text: long, provenance: 'bundled', model: null }, card: { ...card, front: long, back: long } }))
    expect(url.length).toBeLessThan(4000)
  })

  it('the panel offers Report and the dialog shows the message before opening the mail app', async () => {
    const services = await createTestServices({ language: 'en' })
    renderWith(AssociationPanel, services, {
      associations: [association('g', 'generated'), association('m', 'manual')],
      expanded: true,
      card,
    })
    const [button, ...others] = screen.getAllByRole('button', { name: 'Report' })
    expect(others).toHaveLength(0)
    await fireEvent.click(button!)
    const dialog = await screen.findByRole('dialog', { name: 'Report association' })
    expect(dialog.textContent).toContain(REPORT_EMAIL)
    await fireEvent.click(screen.getByRole('radio', { name: 'Wrong or misleading' }))
    const link = screen.getByRole('link', { name: 'Open e-mail' })
    await waitFor(() => expect(decodeURIComponent(link.getAttribute('href') ?? '')).toContain('Reason: wrong'))
    expect(link.getAttribute('href')).toMatch(new RegExp(`^mailto:${REPORT_EMAIL}\\?subject=`))
    expect(screen.getByText(/Text g/, { selector: 'pre' })).toBeTruthy()
  })

  it('without a card there is nothing to report', async () => {
    const services = await createTestServices({ language: 'en' })
    renderWith(AssociationPanel, services, { associations: [association('g', 'generated')], expanded: true })
    expect(screen.queryByRole('button', { name: 'Report' })).toBeNull()
  })
})
