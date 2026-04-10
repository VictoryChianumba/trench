# Research Plan: AI safety research being conducted at frontier labs

## Questions
1. Which organizations count as "frontier labs" for this brief, and what evidence justifies including them?
2. What AI safety research themes are these labs currently pursuing (for example: alignment, interpretability, model evaluations, red teaming, safeguards, robustness, bio/cyber misuse, autonomy/control, governance-preparedness)?
3. What concrete research outputs exist for each lab in the last ~3 years: papers, technical reports, system cards, preparedness frameworks, benchmarks, safety cases, repos, and public evaluation artifacts?
4. How much of each lab's safety work appears to be research versus policy/communications versus deployment guardrails?
5. Where do the labs meaningfully differ in emphasis, maturity, openness, and empirical rigor?
6. What evidence exists from outside the labs that supports, qualifies, or contradicts their own portrayal of their safety work?
7. What important gaps remain: missing evaluations, sparse publication, weak external validation, or unclear links between research and deployment decisions?

## Strategy
- Scope the brief around widely recognized frontier-model developers with substantial frontier-model activity and public safety work. Tentative inclusion set for confirmation: OpenAI, Anthropic, Google DeepMind, Meta, and xAI. If evidence supports it, include a short note on adjacent labs that are frontier-relevant but less transparent.
- Prioritize 2023-2026 sources for current reality, with older foundational materials included only when they still structure current programs.
- Evidence mix:
  - Official web sources: safety pages, preparedness/risk frameworks, model cards, system cards, safety blogs, policy docs, repos
  - Primary research: papers and technical reports authored by lab researchers
  - Code/data artifacts: eval repositories, benchmark releases, safety tooling
  - Independent sources: third-party audits, regulator statements, expert reporting, external benchmark/eval work, credible journalism only when primary evidence is absent
- Expected researcher allocations and dimensions:
  - R1: Official lab sources and public safety programs for OpenAI + Anthropic
  - R2: Official lab sources and public safety programs for Google DeepMind + Meta + xAI
  - R3: Papers, reports, and code artifacts across all included labs; map themes and publication density
  - R4: External validation/critique: independent evaluations, policy analysis, reporting on preparedness, red teaming, and controversies
- Expected rounds:
  - Round 1: Broad evidence collection across the four dimensions above
  - Round 2: Targeted gap-filling on single-source claims, contradictions, and unclear scope boundaries
- Planned synthesis structure:
  - Landscape and scope
  - Thematic safety research areas
  - Per-lab profiles
  - Cross-lab comparison matrix
  - Gaps, disagreements, and open questions

## Acceptance Criteria
- [ ] Frontier lab inclusion/exclusion criteria are explicit and evidenced
- [ ] All key questions answered with at least 2 independent sources for critical claims
- [ ] Each included lab has a grounded profile with concrete research outputs, not just mission statements
- [ ] Distinction between published research, internal/operational safeguards, and PR/policy claims is made explicit
- [ ] Contradictions or evidence gaps are identified and addressed
- [ ] No single-source claims on major comparative conclusions
- [ ] Final brief includes direct URLs for all cited sources

## Task Ledger
| ID | Owner | Task | Status | Output |
|---|---|---|---|---|
| T1 | lead | Define scope, frontier-lab criteria, and comparison dimensions | todo | outputs/.plans/frontier-lab-ai-safety.md |
| T2 | researcher | Gather official safety/program evidence for OpenAI and Anthropic | todo | notes/frontier-lab-ai-safety-research-r1.md |
| T3 | researcher | Gather official safety/program evidence for Google DeepMind, Meta, and xAI | todo | notes/frontier-lab-ai-safety-research-r2.md |
| T4 | researcher | Gather papers, technical reports, and code artifacts across labs | todo | notes/frontier-lab-ai-safety-research-r3.md |
| T5 | researcher | Gather external validation, critiques, and independent assessments | todo | notes/frontier-lab-ai-safety-research-r4.md |
| T6 | lead | Synthesize findings, resolve contradictions, and draft brief | todo | outputs/.drafts/frontier-lab-ai-safety-draft.md |
| T7 | verifier | Add inline citations and validate URLs | todo | outputs/frontier-lab-ai-safety-brief.md |
| T8 | reviewer | Verification pass on support/confidence/gaps | todo | notes/frontier-lab-ai-safety-verification.md |
| T9 | lead | Finalize brief and provenance record | todo | outputs/frontier-lab-ai-safety.md |

## Verification Log
| Item | Method | Status | Evidence |
|---|---|---|---|
| Included-lab list is justified | direct fetch of official model/research pages + external corroboration | pending | outputs/.plans/frontier-lab-ai-safety.md |
| Major claim about each lab's safety focus | source cross-read across official materials and at least one independent source | pending | notes/frontier-lab-ai-safety-research-r*.md |
| Comparative claims about openness/maturity/evals | cross-source synthesis with contradiction check | pending | outputs/.drafts/frontier-lab-ai-safety-draft.md |
| Any quantitative counts/trends in publications or outputs | direct source enumeration and manual spot-check | pending | notes/frontier-lab-ai-safety-research-r3.md |
| Final citations and URLs | verifier URL validation | pending | outputs/frontier-lab-ai-safety-brief.md |
| Final confidence calibration | reviewer verification pass | pending | notes/frontier-lab-ai-safety-verification.md |

## Decision Log
- Initial scope assumption: focus on labs that both build frontier general-purpose models and publicly discuss safety research. Final inclusion set may be revised after evidence review.
- Initial time window: emphasize 2023-2026 to capture current safety programs and model-era changes.
- Comparison principle: distinguish empirical research outputs from governance statements and deployment controls.
