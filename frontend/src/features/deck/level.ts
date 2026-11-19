/** CEFR level for display: `A1-A2` (manifest form) becomes `A1–A2` (en dash). */
export function levelLabel(level: string): string {
  return level.replace('-', '–')
}
