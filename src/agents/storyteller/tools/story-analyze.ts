interface AnalyzeInput {
  content: string
}

interface SectionInfo {
  title: string
  wordCount: number
  lineStart: number
}

interface AnalyzeResult {
  wordCount: number
  characterCount: number
  paragraphCount: number
  sentenceCount: number
  sectionCount: number
  sections: SectionInfo[]
  authorNotes: string[]
  links: string[]
  possibleNames: string[]
  commonTypos: Array<{ word: string; line: number }>
}

const COMMON_TYPOS = new Set([
  "teh", "adn", "taht", "thier", "recieve", "beleive", "occured",
  "occuring", "seperate", "definately", "goverment", "alot",
  "untill", "wierd", "acheive", "acheiving", "arguement",
])

const SECTION_HEADING_RE = /^(#{1,6})\s+(.+)$/gm
const LINK_RE = /https?:\/\/[^\s"'\]>)]+/g
const NOTE_RE = /^\[(?:note|author|comment|meta):\s*(.+)$/im
const SENTENCE_RE = /[.!?](?:\s|$)/g
const WORD_RE = /\b[a-zA-Z]{2,}\b/g
const CAPITALIZED_WORD_RE = /\b[A-Z][a-z]{2,}\b/g

export default async function analyze(args: AnalyzeInput): Promise<AnalyzeResult> {
  const text = args.content
  const lines = text.split("\n")

  const words = text.match(WORD_RE) || []
  const wordCount = words.length
  const characterCount = text.length

  const paragraphs = text.split(/\n\s*\n/).filter((p) => p.trim().length > 0)
  const paragraphCount = paragraphs.length

  const sentences = text.match(SENTENCE_RE) || []
  const sentenceCount = sentences.length

  const sections: SectionInfo[] = []
  let match: RegExpExecArray | null
  const headingRe = new RegExp(SECTION_HEADING_RE.source, "g")
  while ((match = headingRe.exec(text)) !== null) {
    const title = match[2].trim()
    const lineNum = text.slice(0, match.index).split("\n").length
    sections.push({ title, wordCount: 0, lineStart: lineNum })
  }

  for (let i = 0; i < sections.length; i++) {
    const start = sections[i].lineStart
    const end = i + 1 < sections.length ? sections[i + 1].lineStart : lines.length
    const sectionText = lines.slice(start, end).join(" ")
    sections[i].wordCount = (sectionText.match(WORD_RE) || []).length
  }

  const links: string[] = []
  const linkRe = new RegExp(LINK_RE.source, "g")
  while ((match = linkRe.exec(text)) !== null) {
    links.push(match[0])
  }

  const authorNotes: string[] = []
  for (const line of lines) {
    const noteMatch = line.match(NOTE_RE)
    if (noteMatch) {
      authorNotes.push(noteMatch[1].trim())
    }
  }

  const nameCounts = new Map<string, number>()
  const capRe = new RegExp(CAPITALIZED_WORD_RE.source, "g")
  while ((match = capRe.exec(text)) !== null) {
    const w = match[0]
    if (["The", "This", "That", "They", "What", "When", "Where", "Which", "Then", "There", "Here", "How", "But", "And", "Not", "For", "With", "Was", "Were", "Had", "Has", "Have", "Are", "Will", "Would", "Could", "Should", "May", "Might", "Been", "Being"].includes(w)) continue
    nameCounts.set(w, (nameCounts.get(w) || 0) + 1)
  }
  const possibleNames = Array.from(nameCounts.entries())
    .filter(([_, count]) => count >= 2)
    .sort((a, b) => b[1] - a[1])
    .map(([name]) => name)

  const commonTypos: Array<{ word: string; line: number }> = []
  for (let i = 0; i < lines.length; i++) {
    const lineWords = lines[i].toLowerCase().match(/\b[a-z]{2,}\b/g) || []
    for (const w of lineWords) {
      if (COMMON_TYPOS.has(w)) {
        commonTypos.push({ word: lines[i].match(new RegExp(w, "i"))?.[0] || w, line: i + 1 })
      }
    }
  }

  return {
    wordCount,
    characterCount,
    paragraphCount,
    sentenceCount,
    sectionCount: sections.length,
    sections,
    authorNotes,
    links,
    possibleNames,
    commonTypos,
  }
}
