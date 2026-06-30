interface ValidationRule {
  type: "wordCount" | "sectionWordCount" | "paragraphCount"
    | "sentenceCount" | "maxParagraphSize" | "mustInclude" | "mustNotInclude"
    | "mustHaveSection"
  params: Record<string, unknown>
}

interface ValidationResult {
  passed: boolean
  rule: ValidationRule
  actual: number | string | null
  message: string
}

interface ValidateInput {
  content: string
  rules: ValidationRule[]
}

const WORD_RE = /\b[a-zA-Z]{2,}\b/g
const SECTION_HEADING_RE = /^(#{1,6})\s+(.+)$/gm

function countWords(text: string): number {
  return (text.match(WORD_RE) || []).length
}

function countParagraphs(text: string): number {
  return text.split(/\n\s*\n/).filter((p) => p.trim().length > 0).length
}

function getSections(text: string): { title: string; content: string; wordCount: number }[] {
  const sections: { title: string; content: string; wordCount: number }[] = []
  const lines = text.split("\n")
  let currentTitle = "(preamble)"
  let currentLines: string[] = []

  for (const line of lines) {
    const m = line.match(SECTION_HEADING_RE)
    if (m) {
      if (currentLines.length > 0 || sections.length > 0) {
        const content = currentLines.join("\n")
        sections.push({ title: currentTitle, content, wordCount: countWords(content) })
      }
      currentTitle = line.replace(/^#+\s+/, "").trim()
      currentLines = []
    } else {
      currentLines.push(line)
    }
  }
  if (currentLines.length > 0) {
    const content = currentLines.join("\n")
    sections.push({ title: currentTitle, content, wordCount: countWords(content) })
  }
  return sections
}

function evalNumber(val: number, op: string, target: number): boolean {
  switch (op) {
    case "lt": return val < target
    case "gt": return val > target
    case "eq": return val === target
    case "lte": return val <= target
    case "gte": return val >= target
    case "ne": return val !== target
    default: return false
  }
}

function validateRule(text: string, rule: ValidationRule): ValidationResult {
  const { type, params } = rule

  switch (type) {
    case "wordCount": {
      const actual = countWords(text)
      const op = (params.op as string) || "gte"
      const target = params.value as number
      const passed = evalNumber(actual, op, target)
      return { passed, rule, actual, message: `wordCount=${actual} ${op} ${target}` }
    }

    case "paragraphCount": {
      const actual = countParagraphs(text)
      const op = (params.op as string) || "gte"
      const target = params.value as number
      const passed = evalNumber(actual, op, target)
      return { passed, rule, actual, message: `paragraphCount=${actual} ${op} ${target}` }
    }

    case "sentenceCount": {
      const actual = (text.match(/[.!?](?:\s|$)/g) || []).length
      const op = (params.op as string) || "gte"
      const target = params.value as number
      const passed = evalNumber(actual, op, target)
      return { passed, rule, actual, message: `sentenceCount=${actual} ${op} ${target}` }
    }

    case "sectionWordCount": {
      const sections = getSections(text)
      const sectionTitle = params.section as string | undefined
      const op = (params.op as string) || "gte"
      const target = params.value as number
      if (sectionTitle) {
        const sec = sections.find((s) => s.title.toLowerCase().includes(sectionTitle.toLowerCase()))
        if (!sec) return { passed: false, rule, actual: null, message: `Section "${sectionTitle}" not found` }
        const passed = evalNumber(sec.wordCount, op, target)
        return { passed, rule, actual: sec.wordCount, message: `section "${sectionTitle}" wordCount=${sec.wordCount} ${op} ${target}` }
      }
      const allPassed = sections.every((s) => evalNumber(s.wordCount, op, target))
      const worst = Math.min(...sections.map((s) => s.wordCount))
      return { passed: allPassed, rule, actual: worst, message: `min section wordCount=${worst} ${op} ${target}` }
    }

    case "maxParagraphSize": {
      const paragraphs = text.split(/\n\s*\n/).filter((p) => p.trim().length > 0)
      const maxWords = params.maxWords as number
      const over = paragraphs.filter((p) => countWords(p) > maxWords)
      const passed = over.length === 0
      return {
        passed,
        rule,
        actual: over.length > 0 ? `${over.length} paragraphs over ${maxWords} words` : "0",
        message: over.length > 0
          ? `Paragraphs over ${maxWords} words: ${over.length}`
          : `All paragraphs ≤ ${maxWords} words`,
      }
    }

    case "mustInclude": {
      const words = params.words as string[] || []
      const missing = words.filter((w) => !text.toLowerCase().includes(w.toLowerCase()))
      const passed = missing.length === 0
      return { passed, rule, actual: missing.join(", ") || "all found", message: missing.length > 0 ? `Missing: ${missing.join(", ")}` : "All required words present" }
    }

    case "mustNotInclude": {
      const words = params.words as string[] || []
      const found = words.filter((w) => text.toLowerCase().includes(w.toLowerCase()))
      const passed = found.length === 0
      return { passed, rule, actual: found.join(", ") || "none found", message: found.length > 0 ? `Found blocked: ${found.join(", ")}` : "No blocked words found" }
    }

    case "mustHaveSection": {
      const pattern = params.pattern as string
      const sections = getSections(text)
      const found = sections.some((s) => s.title.toLowerCase().includes(pattern.toLowerCase()))
      return { passed: found, rule, actual: found ? "found" : "missing", message: found ? `Section matching "${pattern}" exists` : `No section matching "${pattern}"` }
    }

    default:
      return { passed: false, rule, actual: null, message: `Unknown rule type: ${type}` }
  }
}

export default async function validate(args: ValidateInput): Promise<ValidationResult[]> {
  const results: ValidationResult[] = []
  for (const rule of args.rules) {
    const result = validateRule(args.content, rule)
    results.push(result)
  }
  return results
}
