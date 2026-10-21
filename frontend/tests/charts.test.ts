import { describe, expect, it } from 'vitest'
import type { ReviewDayDto } from '../src/api/types'
import { aggregateReviews, bucketFor, bucketStart, columnPath, heatmapCells, labelIndices, niceMax, stepIndex } from '../src/features/stats/charts'

const days = (start: string, counts: number[]): ReviewDayDto[] =>
  counts.map((count, index) => ({ day: new Date(Date.parse(`${start}T00:00:00Z`) + index * 86_400_000).toISOString().slice(0, 10), count, activeMs: count * 1000 }))

describe('chart scales and marks', () => {
  it('rounds the axis up to 1/2/5×10ⁿ and never returns 0', () => {
    expect([0, -3, Number.NaN, 0.4, 1, 3, 7, 12, 480].map(niceMax)).toEqual([1, 1, 1, 0.5, 1, 5, 10, 20, 500])
  })

  it('draws a column with a rounded top and a square foot, and nothing for an empty value', () => {
    expect(columnPath(10, 100, 20, 0)).toBe('')
    const path = columnPath(10, 100, 20, 50)
    expect(path.startsWith('M10,100V54')).toBe(true)
    expect(path.endsWith('V100Z')).toBe(true)
    // A column lower than the radius keeps its height instead of growing.
    expect(columnPath(0, 10, 20, 2)).toContain('Q0,8 2,8')
  })

  it('steps a keyboard cursor and clamps at both ends', () => {
    expect(stepIndex('ArrowRight', null, 5)).toBe(4)
    expect(stepIndex('ArrowLeft', 0, 5)).toBe(0)
    expect(stepIndex('PageDown', 1, 30)).toBe(8)
    expect(stepIndex('Home', 3, 5)).toBe(0)
    expect(stepIndex('End', 0, 5)).toBe(4)
    expect(stepIndex('Enter', 0, 5)).toBeNull()
    expect(stepIndex('ArrowRight', null, 0)).toBeNull()
  })

  it('labels as many of first/middle/last as fit', () => {
    expect(labelIndices(30, 600, 60)).toEqual([0, 14, 29])
    expect(labelIndices(30, 200, 60)).toEqual([0, 29])
    expect(labelIndices(30, 70, 60)).toEqual([29])
    expect(labelIndices(1, 600, 60)).toEqual([0])
  })
})

describe('review buckets', () => {
  it('uses days for a month and sums weeks and months for longer ranges', () => {
    expect(bucketFor('1m')).toBe('day')
    expect(bucketFor('3m')).toBe('week')
    expect(bucketFor('1y')).toBe('month')
    // 2026-03-04 is a Wednesday; weeks start on Monday.
    expect(bucketStart('2026-03-04', 'week')).toBe('2026-03-02')
    expect(bucketStart('2026-03-04', 'month')).toBe('2026-03-01')
  })

  it('aggregates without losing reviews or time and clips the first bucket to the range', () => {
    const series = days('2026-03-04', [1, 2, 3, 4, 5, 6, 7, 8])
    const weeks = aggregateReviews(series, 'week')
    expect(weeks.map((week) => [week.start, week.end, week.count])).toEqual([
      ['2026-03-04', '2026-03-08', 15],
      ['2026-03-09', '2026-03-11', 21],
    ])
    expect(weeks.reduce((sum, week) => sum + week.activeMs, 0)).toBe(36_000)
    expect(aggregateReviews(series, 'day')).toHaveLength(8)
  })
})

describe('heatmap cells', () => {
  it('lays out the last weeks Monday-first and levels against the busiest day', () => {
    const series = days('2026-02-02', Array.from({ length: 28 }, (_, index) => (index === 27 ? 8 : index % 7 === 0 ? 2 : 0)))
    const cells = heatmapCells(series, 2)
    expect(cells[0]).toMatchObject({ day: '2026-02-16', week: 0, weekday: 0, level: 1 })
    expect(cells.at(-1)).toMatchObject({ day: '2026-03-01', week: 1, weekday: 6, level: 4 })
    expect(cells.filter((cell) => cell.count === 0).every((cell) => cell.level === 0)).toBe(true)
  })

  it('is empty without data, and all-zero data has no level above 0', () => {
    expect(heatmapCells([], 5)).toEqual([])
    expect(heatmapCells(days('2026-03-02', [0, 0, 0]), 5).every((cell) => cell.level === 0)).toBe(true)
  })
})
