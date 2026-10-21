import type { ReviewDayDto, StatsRange, StudyDay } from '../../api/types'

/**
 * Framework-neutral chart helpers: scales, mark geometry, keyboard stepping and the time buckets
 * of the statistics screen. Pure TypeScript, so the arithmetic is tested without a DOM.
 */

/** Smallest 1/2/5×10ⁿ at or above `value`; an empty series still gets a non-zero axis. */
export function niceMax(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 1
  const magnitude = 10 ** Math.floor(Math.log10(value))
  const fraction = value / magnitude
  return (fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10) * magnitude
}

/** Column with a 4 px rounded data end and a square foot on the baseline. */
export function columnPath(x: number, baseline: number, width: number, height: number, radius = 4): string {
  if (height <= 0 || width <= 0) return ''
  const r = Math.min(radius, width / 2, height)
  const top = baseline - height
  return `M${x},${baseline}V${top + r}Q${x},${top} ${x + r},${top}H${x + width - r}Q${x + width},${top} ${x + width},${top + r}V${baseline}Z`
}

/** Next index for a chart's keyboard cursor, or `null` when the key is not a chart key. */
export function stepIndex(key: string, index: number | null, count: number, page = 7): number | null {
  if (count === 0) return null
  const current = index ?? count - 1
  const clamp = (value: number) => Math.min(count - 1, Math.max(0, value))
  switch (key) {
    case 'ArrowRight':
      return clamp(current + 1)
    case 'ArrowLeft':
      return clamp(current - 1)
    case 'PageDown':
      return clamp(current + page)
    case 'PageUp':
      return clamp(current - page)
    case 'Home':
      return 0
    case 'End':
      return count - 1
    default:
      return null
  }
}

/** Which x positions get a label: as many of first/middle/last as fit without touching. */
export function labelIndices(count: number, plotWidth: number, labelWidth: number): number[] {
  if (count === 0) return []
  const last = count - 1
  if (last === 0) return [0]
  const fits = Math.floor(plotWidth / (labelWidth + 16))
  if (fits >= 3 && count >= 3) return [0, Math.floor(last / 2), last]
  if (fits >= 2) return [0, last]
  return [last]
}

const DAY_MS = 86_400_000
const dayMs = (day: StudyDay) => Date.parse(`${day}T00:00:00Z`)
const toDay = (ms: number): StudyDay => new Date(ms).toISOString().slice(0, 10)

export type Bucket = 'day' | 'week' | 'month'

/** Daily bars for a month; longer ranges are summed so a phone shows a readable number of bars. */
export function bucketFor(range: StatsRange): Bucket {
  return range === '1m' ? 'day' : range === '3m' ? 'week' : 'month'
}

export interface ReviewBucket {
  /** First study day of the bucket (Monday for weeks, the 1st for months, clipped to the range). */
  start: StudyDay
  end: StudyDay
  count: number
  activeMs: number
  days: number
}

export function bucketStart(day: StudyDay, bucket: Bucket): StudyDay {
  if (bucket === 'day') return day
  if (bucket === 'month') return `${day.slice(0, 7)}-01`
  const ms = dayMs(day)
  const weekday = (new Date(ms).getUTCDay() + 6) % 7
  return toDay(ms - weekday * DAY_MS)
}

export function aggregateReviews(days: ReviewDayDto[], bucket: Bucket): ReviewBucket[] {
  const buckets: ReviewBucket[] = []
  for (const day of days) {
    const key = bucketStart(day.day, bucket)
    const last = buckets.at(-1)
    if (last && bucketStart(last.start, bucket) === key) {
      last.end = day.day
      last.count += day.count
      last.activeMs += day.activeMs
      last.days += 1
    } else {
      buckets.push({ start: day.day < key ? key : day.day, end: day.day, count: day.count, activeMs: day.activeMs, days: 1 })
    }
  }
  return buckets
}

export interface HeatCell {
  day: StudyDay
  count: number
  /** 0 = no reviews; 1–4 = quartiles of the busiest day. */
  level: 0 | 1 | 2 | 3 | 4
  /** Column (week, oldest first) and row (Monday = 0). */
  week: number
  weekday: number
}

/** The last `weeks` calendar weeks of the range, Monday-first, levelled against the busiest shown day. */
export function heatmapCells(days: ReviewDayDto[], weeks: number): HeatCell[] {
  if (days.length === 0 || weeks <= 0) return []
  const lastDay = days.at(-1)?.day as StudyDay
  const firstShown = dayMs(bucketStart(lastDay, 'week')) - (weeks - 1) * 7 * DAY_MS
  const shown = days.filter((day) => dayMs(day.day) >= firstShown)
  const max = Math.max(0, ...shown.map((day) => day.count))
  return shown.map((day) => {
    const offset = Math.round((dayMs(day.day) - firstShown) / DAY_MS)
    const level = day.count === 0 || max === 0 ? 0 : (Math.min(4, Math.ceil((day.count / max) * 4)) as 1 | 2 | 3 | 4)
    return { day: day.day, count: day.count, level, week: Math.floor(offset / 7), weekday: offset % 7 }
  })
}
