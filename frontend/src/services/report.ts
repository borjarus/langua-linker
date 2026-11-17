import type { AssociationDto, TargetLanguage } from '../api/types'

/**
 * Where reports of AI-generated or bundled associations go (Play AI-generated content policy,
 * ADR-21). The Rust navigation guard opens a `mailto:` link only for this address — keep
 * `src-tauri/src/links.rs` REPORT_EMAIL in sync (checked by tests/report.test.ts).
 */
export const REPORT_EMAIL = 'borjarus@gmail.com'

export const REPORT_REASONS = ['offensive', 'wrong', 'unclear', 'other'] as const
export type ReportReason = (typeof REPORT_REASONS)[number]

/** The card the association belongs to: what the learner sees, nothing else. */
export interface ReportCard {
  front: string
  back: string
  targetLanguage: TargetLanguage
}

export interface AssociationReport {
  reason: ReportReason
  association: Pick<AssociationDto, 'text' | 'provenance' | 'model'>
  card: ReportCard
  appVersion: string | null
  comment: string
}

/** Long fields are cut so the mailto URL stays within what mail apps accept. */
const MAX_SIDE = 300
const MAX_ASSOCIATION = 500
export const MAX_COMMENT = 500

const clip = (value: string, max: number) => {
  const text = value.replace(/\s+/g, ' ').trim()
  return text.length > max ? `${text.slice(0, max - 1)}…` : text
}

/** Only associations someone else wrote can be reported — generated or bundled, never the user's own. */
export const reportable = (association: AssociationDto): boolean =>
  association.status === 'ready' && association.provenance !== 'manual'

export function reportSubject(report: AssociationReport): string {
  return `LanguaLinker: association report (${report.reason})`
}

/** Plain, language-neutral text the user sees in full before sending. No ids, keys or settings. */
export function reportBody(report: AssociationReport): string {
  const lines = [
    `Reason: ${report.reason}`,
    `Association (${report.association.provenance}${report.association.model ? `, ${clip(report.association.model, 80)}` : ''}):`,
    clip(report.association.text, MAX_ASSOCIATION),
    '',
    `Card (${report.card.targetLanguage}):`,
    `Front: ${clip(report.card.front, MAX_SIDE)}`,
    `Back: ${clip(report.card.back, MAX_SIDE)}`,
  ]
  const comment = clip(report.comment, MAX_COMMENT)
  if (comment) lines.push('', `Comment: ${comment}`)
  lines.push('', `App: ${report.appVersion ?? 'unknown'}`)
  return lines.join('\n')
}

export function reportMailto(report: AssociationReport): string {
  return `mailto:${REPORT_EMAIL}?subject=${encodeURIComponent(reportSubject(report))}&body=${encodeURIComponent(reportBody(report))}`
}
