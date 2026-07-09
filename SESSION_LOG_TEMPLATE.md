# Session Log Template

## Purpose

Records significant development sessions for continuity and decision history. This supplements `CLAUDE.md` — which tracks current status — with a detailed history of *what happened and why*. The living log is `SESSION_LOG.md`; this file is the template to copy from.

**When to add an entry:**
- End of any significant work session
- After completing a feature or major milestone
- After making important architectural decisions
- When you want a checkpoint you could return to

You don't need to log every short session. Log when something worth remembering happened.

---

## How to Use

**At the end of a session, say:**
> "Add a session log entry for what we did today."

Claude will review the conversation and write the entry into `SESSION_LOG.md`.

**At the start of a new session, if needed:**
> "Read the latest entry in `SESSION_LOG.md` for context."

(Usually `CLAUDE.md` is enough — use the session log when you need more detail.)

---

## Session Entry Template

```markdown
---

## Session [N] — [Date]

### What We Accomplished
- [Specific thing completed]
- [Another accomplishment]
- [Feature or component built]

### Technical Decisions Made

**[Decision topic]**
- What: [what was decided]
- Why: [rationale]
- Alternatives considered: [what else was considered]

### Files Created / Modified
- `path/to/file` — [what changed]
- `path/to/file` — NEW: [what it does]

### Blockers / Issues
- Resolved: [something that was stuck and got unstuck]
- Outstanding: [something still unresolved]

### Next Session Should
- [ ] [Specific next task]
- [ ] [Following task]

### Notes
- [Anything worth remembering that isn't captured above]
```

---

*Add new entries to `SESSION_LOG.md`, newest above the "Add new entries above this line" marker.*

*Framework v2.0 | February 2026*
