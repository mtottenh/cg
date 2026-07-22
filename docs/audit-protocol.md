# Protocol: driving a test-audit / bug-discovery campaign

A reusable operating brief. Hand §1 to a fresh session as its goal; the rest is the
protocol that makes it work. Derived from the campaign that produced
`web/e2e/COVERAGE-PLAN.md` (27 product findings from what began as a test-quality audit).

---

## 1. The goal (paste this)

> Work through `web/e2e/COVERAGE-PLAN.md` phase by phase. Fan out to agents within a
> phase, never across phases. Tick each checklist item only when it is genuinely done and
> verified. **The primary deliverable is the product bugs you find, not the tests you
> write** — record every one in the plan's findings register with a number, root cause,
> and `file:line` evidence. If you cannot make a test pass honestly because the
> application is wrong, do not weaken the assertion: drop it, record the finding, and move
> on. Verify every claim an agent reports back to you before you believe it. Do not fix
> anything a pending redesign will delete.

---

## 2. The reframe that makes this productive

A test audit looks like hygiene work. It isn't. **Writing a test that genuinely drives the
UI forces you to answer "what *should* happen here?" — and that question is what finds
bugs.** Most were invisible until someone tried to assert correct behaviour.

Consequences:

- **A vacuous test is worse than no test.** It reports coverage while asserting nothing,
  so the area looks examined and never gets examined. The single highest-value early move
  is finding and removing the vacuous ones.
- **Discovery and remediation are a pipeline, and it has a bottleneck.** Track the ratio.
  Discovery running 7:1 ahead of fixes means stop discovering and start draining — more
  findings in an undrained backlog have sharply diminishing value.
- **Expect the yield to be structural.** Findings cluster into *classes* with one root
  cause. Fixing them individually is the wrong unit of work (§5).

## 3. Non-negotiable rules

1. **Never accommodate a bug to get green.** A test that passes by asserting the buggy
   behaviour *certifies* the bug and makes it permanent. Drop the assertion, record the
   finding.
2. **Every finding gets a number in one authoritative register.** Not a checklist item, not
   a bullet in a sweep section — a numbered entry with severity and state. Findings parked
   in checklists get lost; four nearly were here, and they were the *functional* ones.
3. **Evidence before theory.** When a test fails, read the artefact — the accessibility
   snapshot, the actual DOM, the real query — *before* forming a hypothesis. Theorising
   first cost multiple wrong turns in this campaign; the snapshot had the answer
   immediately every time.
4. **Verify what agents report.** Not from distrust — from the fact that a confident,
   well-written summary is indistinguishable from a correct one until you check. Re-run the
   gates yourself. Read the diff of anything load-bearing. Spot-check that new tests assert
   what they claim.
5. **Install a mechanical ratchet.** A script that counts anti-patterns with a committed
   baseline that may only decrease. Judgement does not survive fan-out across agents;
   a failing script does. Give it an explicit escape hatch (`// audit-exempt: <reason>`)
   so honest exceptions are visible rather than hidden.
6. **Prove the fix, not the formatting.** If the bug is "matches in state X are hidden",
   the test must show a match in state X *appearing*. A test that only checks the label
   would pass against the bug.

## 4. Distinguishing a bug from a deliberate choice

Before "fixing" an inconsistency, establish it is not intentional. **This is the mistake
most worth avoiding, because it manufactures regressions while appearing to fix things.**

In this campaign: two status maps existed with different wording for the same states. It
looked like duplication. Collapsing them broke a test asserting "Live Now" — the
divergence was deliberate (public voice vs admin voice). The correct fix *preserved* the
divergence and added a fallback.

Checks, in order of cost:
- Does a test assert the current behaviour? A passing test is a specification.
- Does the git history explain it?
- Do the two sites serve different audiences, layers, or trust boundaries?
- Is there a comment, even a stale one, stating intent?

When a finding is a *design opinion*, reframe it to the inarguable defect underneath.
"Should hard-lock block substitutes?" is arguable and stalls. "The direct-add path and the
invitation path enforce different rules, so the same action is allowed or denied depending
on the route" is not arguable, and it holds under either answer.

## 5. Fix classes, not instances — then prevent recurrence

When three findings share a root cause, they are one finding with three sites. Sweep them
together, then ask what would have made the class impossible.

Here, six findings were all "a hand-rolled comparison against a status string that drifted
from what the backend emits, with a `default` branch leaking the raw enum" — because
nothing type-checked those strings. The durable fix is generated union types from the API
schema, so drift becomes a compile error. **Without that step, the sweep recurs.**

Corollary: **do not fix what a pending redesign deletes.** Four roster-lock findings were
deliberately left open because the lineup redesign makes one of them structurally
impossible rather than fixed. Recognising this removed four items from the backlog for
free.

## 6. Half-built features are a signal, not a defect

Watch for:
- a producer hardcoded to empty (`let unrecognized = Vec::new();`) feeding a complete,
  well-tested consumer
- a validator that returns "pass" on empty input, called with empty input
- a designed-but-never-created table referenced in docs
- a rule stated in a schema comment with no code enforcing it

Each looks like a small bug. **Several of them pointing at the same absence is a missing
domain concept**, and the right response is a design document, not four patches. Three
such features here all turned out to be blocked on one table that was specified and never
built.

Related: when a user's mental model conflicts with the schema, do not assume either is
wrong. Ask what the system *actually* models, in the schema. The gap between the two is
usually the real finding — and it may be that the requirement is not merely awkward to
express but *unrepresentable*, which is a redesign trigger.

## 7. Running agents in parallel

Fan out **within** a phase. Phases that touch the same files must be sequential — a
prevention step that rewrites every call site cannot run alongside the sweep fixing those
call sites.

- **Partition by file ownership, stated explicitly**, including an explicit do-NOT-touch
  list. Partition by *surface* (public / admin / player) rather than by finding number —
  findings cluster by page, so numeric splits collide.
- **Freeze shared files before dispatch.** Identify what every agent needs to edit, make
  those edits yourself first, then declare the file read-only and tell agents to report
  rather than edit if something is missing.
- **Forbid `git add -u` and `git add -A` absolutely.** Agents sharing a working tree will
  stage each other's in-flight work. Require explicit paths. Have each agent commit its
  own work so review has clean boundaries.
- **Give each agent the traps.** Every brief should carry the known failure modes: the
  deliberate divergence it must not collapse, the shared-identity test fixture that makes
  authorization tests pass for the wrong reason, the baseline counts it must not regress.
- **Require pasted verification output**, and require agents to say explicitly when they
  could not run something rather than reporting untested work as done.

## 8. Verifying risky changes yourself

Data migrations, authorization changes, and anything that mutates existing rows get
verified independently of the report.

For a migration: build a scratch database, insert **adversarial** data — the collision
whose replacement name is also taken, the value at the exact column length limit, the
control row that must not change — run it, and check the invariant holds and the control
survived. This found nothing wrong here, which is the point: it converted a plausible
claim into a checked one, cheaply.

For authorization: confirm the denial *and* that the legitimate path still works, and
assert the underlying state was untouched rather than only reading the status code.

## 9. Definition of done for a phase

- Every checklist item ticked is genuinely done — verified, not assumed.
- Every finding is numbered in the register with root cause, `file:line`, and severity.
- The ratchet baseline has not increased.
- The full suite passes, with the count stated. Pre-existing failures are named as such.
- Anything deliberately not done is written down with the reason.
- If the phase found a class, the prevention step is scheduled — not just the instances.
