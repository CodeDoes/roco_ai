# UX Plan — RoCo AI

> The human is the author. The AI is the tool. Every screen should make the
> human feel in control, never reviewed-to-death.

## Core flows that must be first-class in the UI

These already exist as *logic* in `crates/agent`. The UI must expose them:

1. **Pace control** (`interaction.rs`) — Full / Moderate / No / Go-Ham modes.
   The UI must let the human switch modes mid-run and must visibly pause at
   the right moments (after each chapter in Full, at batch boundaries in
   Moderate). Test: a human can pause and resume.
2. **Accept / Modify / Skip / Stop** — every generated artifact (chapter,
   outline node, wiki entry) shows these controls. Test: clicking skip jumps
   ahead without regenerating; stop ends and offers publish.
3. **Outline editor** (`outline_editing.rs`) — add / remove / move / modify /
   regenerate nodes, with live renumbering. Not a JSON dump.
4. **Chapter steering** (`chapter_steering.rs`) — pause / steer / resume
   mid-generation. The human can inject direction while a chapter streams.
5. **Commentary** (`commentary.rs`) — bidirectional notes + verdicts on every
   artifact. The human can annotate; the agent explains.
6. **Story direction** (`story_direction.rs`) — capture premise / themes /
   tone once, see it applied, edit it.
7. **Revision with diff** (`writing_assistant.rs`) — show a diff, not a
   replacement. Human accepts or rejects the diff.
8. **Persistence** (`story_persistence.rs`) — list / resume / destructive-load
   with a clear confirm. The neglected UX surface per the old plan.

## Layout principles

- **One artifact in focus.** The human is working on one chapter (or outline
  node, or wiki entry) at a time. Side panels, not tabs-fighting-for-focus.
- **The AI's output is a suggestion until accepted.** Visually distinct from
  accepted content.
- **Controls are always visible, never hidden behind a menu** for the
  accept/skip/stop triad.
- **Streaming is a preview, not a commitment.** A streaming chapter can be
  stopped; stopping does not publish.

## What we are NOT doing

- No new model/inference features. The core is frozen.
- No grammar-coverage work unless it changes what the human sees.
- No more example binaries as a stand-in for a real surface.
