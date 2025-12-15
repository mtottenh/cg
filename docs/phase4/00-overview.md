# Phase 4: Demo Integration & Result Review - Overview

> **Status**: Design Phase
> **Dependencies**: Phase 3 (Match System) - specifically 3.6 Result Submission, 3.7 Evidence System, 3.8 Demo Evidence Integration
> **Related Documents**: [tournament-system-design.md](../tournament-system-design.md), [phase3/05-evidence-system.md](../phase3/05-evidence-system.md)

---

## Executive Summary

Phase 4 implements **deep integration** between the demo catalog system and the tournament workflow. While Phase 3.8 established external demo stats fetching and evidence validation, Phase 4 creates a **bridge** allowing result claims to directly reference cataloged demos as authoritative evidence, with automated validation triggering a review workflow when discrepancies are detected.

### Key Capabilities

1. **Demo-Result Bridge** - Result claims can reference demos from the catalog as evidence via `demo_link_ids`
2. **Automated Validation** - Match completion validates claimed results against linked demo stats
3. **Result Review System** - Validation mismatches trigger human review workflows:
   - **Roster Mismatch**: Both captains must acknowledge unrecognized players
   - **Score/Winner Mismatch**: League admin approval required
4. **Missing API Handlers** - Get demos for match, unlink demo from match
5. **Comprehensive Tests** - 20+ integration tests covering end-to-end workflows

### Design Principles

- **Bridge Model**: Demos and evidence remain **separate first-class entities** with a linking bridge
- **Non-Blocking by Default**: Validation issues create reviews, not hard blocks
- **Two-Tier Review**: Minor issues (roster) vs. major issues (score/winner) have different approval paths
- **Audit Complete**: All validation results and review decisions logged for transparency

---

## Architecture: Bridge Model

Rather than merging demos into the evidence system, we use a **bridge approach**:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           DEMO-RESULT BRIDGE MODEL                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌─────────────────┐                              ┌─────────────────────────┐   │
│  │  RESULT CLAIMS  │                              │    DEMO CATALOG         │   │
│  │                 │                              │                         │   │
│  │ evidence_ids[]  │ ─────────────────────────┐   │  demos                  │   │
│  │                 │                          │   │  demo_players           │   │
│  │ demo_link_ids[] │ ──────┐                  │   │  demo_match_links       │   │
│  └─────────────────┘       │                  │   └─────────────────────────┘   │
│                            │                  │                                  │
│                            ▼                  ▼                                  │
│                   ┌─────────────────┐  ┌─────────────────┐                      │
│                   │ demo_match_links│  │ match_evidence  │                      │
│                   │                 │  │                 │                      │
│                   │ → Demo catalog  │  │ → Evidence sys  │                      │
│                   │   integration   │  │   (Phase 3.7)   │                      │
│                   └─────────────────┘  └─────────────────┘                      │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Why Separate?

1. **Clean Domain Boundaries**: Evidence system handles arbitrary uploads; demo catalog is specialized
2. **Avoid Duplication**: Demos have their own metadata (players, stats); evidence doesn't need copies
3. **Different Lifecycles**: Demos persist beyond individual matches; evidence is match-scoped
4. **Flexibility**: A result claim can have both evidence and demo links, or either

---

## Subsystem Map

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          DEMO INTEGRATION (Phase 4)                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────────────┐   │
│  │  DEMO CATALOG   │────▶│   DEMO-MATCH    │────▶│    RESULT SUBMISSION    │   │
│  │   (Existing)    │     │    LINKING      │     │       WITH DEMOS        │   │
│  │                 │     │                 │     │                         │   │
│  │ - Browse demos  │     │ - Link/unlink   │     │ - Submit with demo_ids  │   │
│  │ - View stats    │     │ - Match demos   │     │ - Auto-link on submit   │   │
│  │ - Categorize    │     │ - Game number   │     │ - Per-game demo refs    │   │
│  └─────────────────┘     └────────┬────────┘     └───────────┬─────────────┘   │
│                                   │                          │                  │
│                          ┌────────▼──────────────────────────▼─────────┐       │
│                          │           DEMO VALIDATION SERVICE            │       │
│                          │                                              │       │
│                          │  - Compare demo stats to claimed result      │       │
│                          │  - Detect roster mismatches                  │       │
│                          │  - Detect score/winner mismatches            │       │
│                          │  - Calculate confidence score                │       │
│                          └────────────────────┬─────────────────────────┘       │
│                                               │                                  │
│                          ┌────────────────────▼─────────────────────────┐       │
│                          │           RESULT REVIEW SYSTEM               │       │
│                          │                                              │       │
│                          │  Triggers:                                   │       │
│                          │  - Roster mismatch → Captain acknowledgment  │       │
│                          │  - Score mismatch  → Admin approval          │       │
│                          │  - Winner mismatch → Admin approval          │       │
│                          └────────────────────┬─────────────────────────┘       │
│                                               │                                  │
│                          ┌────────────────────▼─────────────────────────┐       │
│                          │           MATCH COMPLETION SAGA              │       │
│                          │           (Extended from Phase 3)            │       │
│                          │                                              │       │
│                          │  New Step: step_validate_demos()             │       │
│                          │  - Runs validation against linked demos      │       │
│                          │  - Creates ResultReview if issues found      │       │
│                          │  - Stores validation results                 │       │
│                          └──────────────────────────────────────────────┘       │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## Data Flow

```
Result Submission with Demo
         │
         ▼
┌─────────────────┐
│ Submit Result   │
│ + demo_link_ids │
└────────┬────────┘
         │
         ▼
┌─────────────────┐    ┌─────────────────┐
│ Validate demos  │───▶│ Demos exist and │  No  ┌─────────────────┐
│ belong to match │    │ linked to match?│─────▶│ Reject: Invalid │
└─────────────────┘    └────────┬────────┘      │ demo reference  │
                                │ Yes           └─────────────────┘
                                ▼
                       ┌─────────────────┐
                       │ Result confirmed│
                       │ (normal flow)   │
                       └────────┬────────┘
                                │
                                ▼
                       ┌─────────────────┐
                       │ Match Completion│
                       │ Saga triggered  │
                       └────────┬────────┘
                                │
                       ┌────────▼────────┐
                       │ Validate demos  │
                       │ against result  │
                       └────────┬────────┘
                                │
              ┌─────────────────┼─────────────────┐
              ▼                 ▼                 ▼
     ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
     │ No Issues       │ │ Roster Mismatch │ │ Score/Winner    │
     │                 │ │ Only            │ │ Mismatch        │
     └────────┬────────┘ └────────┬────────┘ └────────┬────────┘
              │                   │                   │
              │                   ▼                   ▼
              │          ┌─────────────────┐ ┌─────────────────┐
              │          │ Create Review   │ │ Create Review   │
              │          │ status: pending │ │ status: pending │
              │          │ _acknowledgment │ │ _admin_review   │
              │          └────────┬────────┘ └────────┬────────┘
              │                   │                   │
              │                   ▼                   │
              │          ┌─────────────────┐          │
              │          │ Both captains   │          │
              │          │ acknowledge     │          │
              │          └────────┬────────┘          │
              │                   │                   │
              │          ┌────────▼────────┐          │
              │          │ Only roster?    │          │
              │          └────────┬────────┘          │
              │                   │                   │
              │      Yes ┌────────┴────────┐ No      │
              │          ▼                 ▼         │
              │  ┌─────────────┐   ┌──────────────┐  │
              │  │ status:     │   │ Escalate to  │  │
              │  │ acknowledged│   │ admin review │  │
              │  └──────┬──────┘   └──────┬───────┘  │
              │         │                 │          │
              │         │    ┌────────────┴──────────┘
              │         │    │
              │         │    ▼
              │         │ ┌─────────────────┐
              │         │ │ Admin approves  │
              │         │ │ or rejects      │
              │         │ └────────┬────────┘
              │         │          │
              │         │    ┌─────┴─────┐
              │         │    ▼           ▼
              │         │ Approved    Rejected
              │         │    │           │
              │         └────┤           ▼
              │              │   ┌─────────────────┐
              │              │   │ Match returns   │
              │              │   │ to in_progress  │
              │              │   └─────────────────┘
              │              │
              └──────────────┤
                             ▼
                    ┌─────────────────┐
                    │ Continue saga:  │
                    │ Bracket advance │
                    └─────────────────┘
```

---

## Implementation Plan

### Sub-Phase Summary

| Sub-Phase | Name | Complexity | Dependencies | Key Deliverables |
|-----------|------|------------|--------------|------------------|
| 4.1 | Demo Handlers & Validation | M | 3.8 complete | Get demos for match, unlink, validation methods |
| 4.2 | Result Claim Demo Bridge | M | 4.1 | demo_link_ids column, submit with demos |
| 4.3 | Result Review System | H | 4.2 | ResultReview entity, captain acknowledgment |
| 4.4 | Review Workflow Integration | H | 4.3 | Saga integration, admin resolution |

**Complexity**: S = Small, M = Medium, H = High

### Dependency Graph

```
┌───────────────────────┐
│ Phase 3.8 Complete    │
│ (Demo Evidence)       │
└───────────┬───────────┘
            │
            ▼
    ┌───────────────┐
    │ 4.1 Demo      │
    │ Handlers &    │
    │ Validation    │
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 4.2 Result    │
    │ Claim Bridge  │
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 4.3 Result    │
    │ Review System │
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 4.4 Workflow  │
    │ Integration   │
    └───────────────┘
```

---

## New Entities

### ResultReview

Tracks validation issues requiring human review:

```rust
pub struct ResultReview {
    pub id: ResultReviewId,
    pub result_claim_id: ResultClaimId,
    pub match_id: TournamentMatchId,

    // Review triggers
    pub roster_mismatch: bool,
    pub score_mismatch: bool,
    pub winner_mismatch: bool,

    // Demo validation details
    pub demo_link_id: Option<DemoMatchLinkId>,
    pub validation_result: Option<DemoValidationResult>,
    pub unrecognized_players: Vec<UnrecognizedPlayer>,

    // Status tracking
    pub status: ResultReviewStatus,
    pub captain1_acknowledged: bool,
    pub captain1_acknowledged_at: Option<DateTime<Utc>>,
    pub captain2_acknowledged: bool,
    pub captain2_acknowledged_at: Option<DateTime<Utc>>,

    // Admin resolution
    pub reviewed_by_user_id: Option<UserId>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub admin_notes: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### DemoValidationResult

Stored validation outcome:

```rust
pub struct DemoValidationResult {
    pub is_valid: bool,
    pub confidence: f32,
    pub extracted_score: Option<(i32, i32)>,
    pub claimed_score: (i32, i32),
    pub map_match: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}
```

---

## Database Schema Summary

### New Tables

| Table | Purpose | Document |
|-------|---------|----------|
| `result_reviews` | Review workflow state | 02-result-review-system.md |

### Modified Tables

| Table | Changes | Document |
|-------|---------|----------|
| `result_claims` | Add `demo_link_ids UUID[]` | 01-demo-integration.md |
| `demo_match_links` | Add `validation_result JSONB` | 01-demo-integration.md |

### Migrations

- `0041_result_claims_demo_links.sql` - Add demo_link_ids to result_claims
- `0042_result_reviews.sql` - Create result_reviews table and enum

---

## New Services

| Service | Responsibility | Document |
|---------|---------------|----------|
| `DemoService` (extended) | Validation methods, match demos | 01-demo-integration.md |
| `ResultService` (extended) | Submit with demo_link_ids | 01-demo-integration.md |
| `ResultReviewService` | Review workflow management | 02-result-review-system.md |
| `MatchCompletionSaga` (extended) | Demo validation step | 02-result-review-system.md |

---

## API Summary

### New Endpoints

| Method | Path | Purpose | Document |
|--------|------|---------|----------|
| GET | `/v1/matches/{match_id}/demos` | Get demos linked to match | 01 |
| DELETE | `/v1/admin/demos/{id}/link/{match_id}` | Unlink demo from match | 01 |
| GET | `/v1/matches/{match_id}/result-review` | Get review status | 02 |
| POST | `/v1/matches/{match_id}/result-review/acknowledge` | Captain acknowledgment | 02 |
| GET | `/v1/admin/result-reviews` | List pending reviews | 02 |
| GET | `/v1/admin/result-reviews/{id}` | Get review details | 02 |
| POST | `/v1/admin/result-reviews/{id}/approve` | Admin approves | 02 |
| POST | `/v1/admin/result-reviews/{id}/reject` | Admin rejects | 02 |

### Modified Endpoints

| Method | Path | Changes | Document |
|--------|------|---------|----------|
| POST | `/v1/matches/{id}/result/submit` | Add demo_ids, per-game demo_id | 01 |

---

## Authorization Model

| Action | Permission Required | Context |
|--------|---------------------|---------|
| Get demos for match | Match participant or admin | Own matches |
| Unlink demo | `tournament.brackets.manage` | Admin only |
| Acknowledge review | Match captain | Own team only |
| List pending reviews | `tournament.disputes.resolve` | League scope |
| Approve/reject review | `tournament.disputes.resolve` | League scope |

---

## Success Criteria

Phase 4 is complete when:

1. **Demo Bridge**: Result claims can include demo_link_ids that reference the demo catalog
2. **Validation**: Demo stats are automatically validated against claimed results
3. **Roster Review**: Unrecognized players trigger captain acknowledgment workflow
4. **Admin Review**: Score/winner mismatches require admin approval
5. **API Complete**: All new endpoints implemented with OpenAPI docs
6. **Tests**: 20+ integration tests covering all workflows
7. **Saga Integration**: Match completion saga includes demo validation step

---

## Test Categories

| Category | Count | Focus |
|----------|-------|-------|
| A: Demo Catalog | 5 | List, get, players, links |
| B: Demo-Match Linking | 5 | Link, unlink, get for match |
| C: Result with Demos | 5 | Submit with demo_ids, validation |
| D: End-to-End Workflow | 3 | Full flow, mixed evidence |
| E: Admin Operations | 2 | Catalog, categorize |
| F: Review System | 5 | Acknowledgment, approval, rejection |

**Total: 25 tests**

---

## Related Documents

- [01-demo-integration.md](./01-demo-integration.md) - Demo handlers, validation, bridge
- [02-result-review-system.md](./02-result-review-system.md) - Review triggers, workflows, admin resolution
