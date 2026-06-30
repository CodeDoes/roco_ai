import { RwkvEngine } from "./rwkv-engine.ts"
import { SessionManager } from "./session.ts"
import { StoryState, ChapterInfo, DEFAULT_GEN_OPTS, RwkvMessage, GenerateOpts } from "./types.ts"

const STORYTELLER_SYSTEM = `You are a creative writing AI assistant. You write compelling fiction with rich worldbuilding, consistent character development, and engaging plots.

Core rules:
- Write proactively. Do not ask questions.
- Maintain consistent tone, POV, and tense throughout.
- Show, don't tell. Use sensory details, dialogue, and action.
- Each chapter section should advance plot, develop character, or deepen worldbuilding.
- Track word counts. Chapter sections target 400-800 words.
- Keep responses minimal — no verbose summaries or step-by-step narration. Just write.`

function cleanOutput(text: string): string {
  return text
    .replace(/^Assistant:\s*/i, "")
    .replace(/<think>[\s\S]*?<\/think>\n*/g, "")
    .trim()
}

export class StorytellerAgent {
  private engine: RwkvEngine
  private session: SessionManager
  private storyState: StoryState | null = null
  private fixParagraphBreak: boolean

  constructor(
    engine: RwkvEngine,
    session: SessionManager,
    config?: { fixParagraphBreak?: boolean },
  ) {
    this.engine = engine
    this.session = session
    this.fixParagraphBreak = config?.fixParagraphBreak ?? false
  }

  async init() {
    await this.session.ensureDir()
    const sess = await this.session.load()

    if (sess.status === "new") {
      await this.engine.bakeSystemPrompt(STORYTELLER_SYSTEM)
      await this.session.save()
    } else {
      await this.engine.loadBaseline()
    }
  }

  async continueStory(userInput: string, opts: Partial<GenerateOpts> = {}): Promise<string> {
    const sess = this.session.get()
    sess.status = "active"

    const history = this.session.buildPrompt(STORYTELLER_SYSTEM)
    const fullPrompt = history + userInput + "\n\n"

    const raw = await this.engine.generate(fullPrompt, {
      ...DEFAULT_GEN_OPTS,
      temperature: 0.85,
      ...opts,
      fixParagraphBreak: this.fixParagraphBreak,
    })

    const cleaned = cleanOutput(raw)
    this.session.addMessage({ role: "user", content: userInput })
    this.session.addMessage({ role: "assistant", content: cleaned })
    await this.session.save()

    return cleaned
  }

  async continueStoryStream(
    userInput: string,
    onText: (text: string) => void,
    opts: Partial<GenerateOpts> = {},
  ): Promise<string> {
    const sess = this.session.get()
    sess.status = "active"

    const history = this.session.buildPrompt(STORYTELLER_SYSTEM)
    const fullPrompt = history + userInput + "\n\n"

    const raw = await this.engine.generateStream(
      fullPrompt,
      { onText },
      { ...DEFAULT_GEN_OPTS, temperature: 0.85, ...opts, fixParagraphBreak: this.fixParagraphBreak },
    )

    const cleaned = cleanOutput(raw)
    this.session.addMessage({ role: "user", content: userInput })
    this.session.addMessage({ role: "assistant", content: cleaned })
    await this.session.save()

    return cleaned
  }

  async saveChapterCheckpoint(chapterNum: number, slug: string) {
    const name = `chapter_${String(chapterNum).padStart(3, "0")}_${slug}`
    await this.engine.saveCheckpoint(name)
    this.session.registerCheckpoint(name, this.engine.statePath(name))
    await this.session.save()
  }

  async loadChapterCheckpoint(chapterNum: number) {
    const sess = this.session.get()
    const key = Object.keys(sess.statePaths.checkpoints).find(
      (k) => k.startsWith(`chapter_${String(chapterNum).padStart(3, "0")}_`),
    )
    if (!key) {
      await this.engine.loadBaseline()
      return false
    }
    await this.engine.loadCheckpoint(key)
    return true
  }

  async resumeFromBaseline() {
    await this.engine.loadBaseline()
  }

  async dispose() {
    await this.session.save()
    await this.engine.dispose()
  }
}
