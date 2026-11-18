/**
 * Inline Markdown of card text: `**bold**`, `__bold__` and `*italic*` (a single `_` is left alone:
 * it appears inside ordinary words).
 * Nothing else is interpreted and no HTML is produced — the result is styled runs of plain text,
 * rendered with Svelte text nodes. Unmatched markers stay literal; `\*` and `\_` are literal.
 *
 * Indices are UTF-16 offsets into the **source** text (word links store ranges in the source,
 * markers included), so runs can be intersected with link ranges.
 */

export interface StyledRun {
  /** Source range of the visible text (markers excluded). */
  start: number
  end: number
  text: string
  bold: boolean
  italic: boolean
}

interface Marks {
  hidden: boolean[]
  bold: boolean[]
  italic: boolean[]
}

function isSpace(char: string | undefined): boolean {
  return char === undefined || /\s/u.test(char)
}

function mark(text: string): Marks {
  const length = text.length
  const marks: Marks = { hidden: Array<boolean>(length).fill(false), bold: Array<boolean>(length).fill(false), italic: Array<boolean>(length).fill(false) }
  // Escapes: the backslash is hidden, the next character is literal.
  const literal = Array<boolean>(length).fill(false)
  for (let index = 0; index < length - 1; index += 1) {
    if (text[index] === '\\' && (text[index + 1] === '*' || text[index + 1] === '_') && !literal[index]) {
      marks.hidden[index] = true
      literal[index + 1] = true
    }
  }
  const free = (index: number) => !marks.hidden[index] && !literal[index]
  // A still-unused marker character next to a single marker makes it part of a run (`**`).
  const adjacent = (index: number, marker: string) => text[index] === marker && free(index)

  // Pairs of a marker; content non-empty and not starting/ending with whitespace.
  const pairs = (marker: string, style: 'bold' | 'italic') => {
    const size = marker.length
    let index = 0
    while (index <= length - size) {
      const opens =
        text.startsWith(marker, index) &&
        [...Array(size).keys()].every((offset) => free(index + offset)) &&
        !isSpace(text[index + size]) &&
        // A single marker must not be half of a double one.
        (size === 2 || !(adjacent(index - 1, marker) || adjacent(index + 1, marker)))
      if (!opens) {
        index += 1
        continue
      }
      let close = -1
      for (let candidate = index + size + 1; candidate <= length - size; candidate += 1) {
        const closes =
          text.startsWith(marker, candidate) &&
          [...Array(size).keys()].every((offset) => free(candidate + offset)) &&
          !isSpace(text[candidate - 1]) &&
          // `***` closing an inner italic: the bold closes on the last two stars.
          (size === 2 ? text[candidate + 2] !== '*' : !(adjacent(candidate - 1, marker) || adjacent(candidate + 1, marker)))
        if (closes) {
          close = candidate
          break
        }
      }
      if (close < 0) {
        index += 1
        continue
      }
      for (let offset = 0; offset < size; offset += 1) {
        marks.hidden[index + offset] = true
        marks.hidden[close + offset] = true
      }
      for (let inner = index + size; inner < close; inner += 1) marks[style][inner] = true
      index = close + size
    }
  }
  pairs('**', 'bold')
  pairs('__', 'bold')
  pairs('*', 'italic')
  return marks
}

/** Visible text split into runs of equal style. */
export function inlineRuns(text: string): StyledRun[] {
  const marks = mark(text)
  const runs: StyledRun[] = []
  for (let index = 0; index < text.length; index += 1) {
    if (marks.hidden[index]) continue
    const char = text[index] ?? ''
    const bold = marks.bold[index] === true
    const italic = marks.italic[index] === true
    const last = runs.at(-1)
    if (last && last.end === index && last.bold === bold && last.italic === italic) {
      last.end = index + 1
      last.text += char
    } else {
      runs.push({ start: index, end: index + 1, text: char, bold, italic })
    }
  }
  return runs
}

/** The text without Markdown markers (speech, plain labels). */
export function stripInlineMarkdown(text: string): string {
  return inlineRuns(text)
    .map((run) => run.text)
    .join('')
}

/** IPA transcriptions written between slashes, e.g. `/hiːz/`. */
const IPA = /(^|\s)\/[^/\n]+\/(?=$|[\s.,;:!?)])/gu

/**
 * The text to read aloud: the bold part when the card marks one (`**he's** /hiːz/` → `he's`),
 * otherwise the whole visible text; IPA between slashes is never read.
 */
export function speechText(text: string): string {
  const runs = inlineRuns(text)
  const spoken = runs.some((run) => run.bold)
    ? // Separate bold parts get a space; a bold part split only by style (`**a *b***`) stays whole.
      runs.map((run) => (run.bold ? run.text : ' ')).join('')
    : runs.map((run) => run.text).join('')
  return spoken.replace(IPA, '$1').replace(/\s+/gu, ' ').trim()
}
