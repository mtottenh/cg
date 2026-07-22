# Design: match lineups as a first-class concept

**Status:** proposal, not yet implemented
**Supersedes in part:** P-15, P-18 in `web/e2e/COVERAGE-PLAN.md` §9b
**Date:** 2026-07-22

---

## 1. The problem in one sentence

`league_team_members` is being asked to answer two different questions, and it can
only answer one of them well:

| Question | Should be answered by | Actually answered by |
|---|---|---|
| Who is **eligible** to play for this team this season? | the roster | `league_team_members` ✅ |
| Who **actually played** in this match? | a lineup | `league_team_members` ❌ |

There is no lineup. `tournament_matches` binds to `tournament_registrations`, which
binds to `league_team_seasons` — **the chain never reaches a player**
(`migrations/0030_create_tournaments.sql:314-315`). Nothing anywhere in the codebase
records, or can record, who took the server.

Every symptom below is downstream of that one gap.

## 2. Why the roster lock feels wrong

The lock is the most visible symptom, and it is what prompted this design.

Because a roster entry is the only way to express "this person might play", fielding an
ad-hoc substitute requires **permanently modifying the eligible pool**. That forces the
lock to be simultaneously strict (or it is not a lock) and lenient (or you cannot field a
short team). It resolves this with two predicates —
`allows_primary_changes()` and `allows_substitute_changes()`
(`crates/portal-core/src/types/league_team.rs:122,128`) — which is an approximation of
"frozen, but flexible", and it is exactly those two predicates that **P-15** found
drifting apart between the direct-add and invitation paths.

It also forces substitutes to be a *stored role* with a season cap
(`league_seasons.max_substitutes`, default 2) — a pre-registered bench. That is a real
model, used by some leagues, but it is not the common one, and it is not what this league
does. **The requirement that prompted this doc — "we usually don't have pre-registered
subs; we get someone in when we can't field a full team" — is unrepresentable in the
current schema.** Not merely awkward: there is no row you can write that means it.

## 3. This is a design that was started three times and never finished

The strongest argument for lineups is that the codebase keeps reaching for one and
finding nothing there. Three separate features are half-built around the missing table:

**(a) The roster-mismatch review flow is fully built and permanently dead.**
`result_reviews` has `roster_mismatch BOOLEAN` and `unrecognized_players JSONB`
(`migrations/0042_result_reviews.sql:18,25`), with a complete two-captain acknowledgment
flow in `services/tournament/result_review.rs:45-170`. Its producer is:

```rust
// crates/portal-api/src/adapters/demo_validator.rs:88
let unrecognized = Vec::new();
```

— declared, never mutated, passed at `:145`. `roster_mismatch` is **always false** in
production. The feature cannot fire, because computing it requires knowing who was
supposed to be playing.

**(b) The CS2 player verifier is wired to always pass.**
`Cs2EvidenceValidator::verify_players` short-circuits to `(true, 0, 0)` on empty input
(`crates/portal-plugins/src/games/cs2/evidence_validator.rs:182-186`), and its only
production call site passes two empty slices:

```rust
// crates/portal-plugins/src/games/cs2/mod.rs:1352
let validation = Cs2EvidenceValidator::validate(&stats, claimed_result, &[], &[]);
// comment on :1351 reads: "Steam IDs unavailable at this layer"
```

The comment is an accurate description of the gap. The verifier is correct; it has no
source of expected players to verify against.

**(c) `match_players` was specified and never built.**
`docs/gaming-portal-database-schema.md:1370-1426` already designs this table, including
`team_slot`, `is_substitute`, and `participation_status ∈ confirmed | no_show |
left_early | substituted | removed`. Open questions in
`docs/prompts/phase3-match-system-design.md:66,76` are about the same thing.

Three independent features, each blocked on the same missing table. This is not a new
idea being introduced — it is an existing design being completed.

## 4. The integrity problem (the strongest argument)

Independent of substitutes, the missing lineup means **player statistics are unverifiable
and currently wrong in two distinct ways.**

**Attribution is a bare global Steam-ID join.** Both resolution sites
(`crates/portal-db/src/adapters/demo.rs:899-919` and
`crates/portal-db/src/adapters/demo_stats.rs:118-132`) are:

```sql
UPDATE demo_player_stats s SET player_id = p.id
FROM players p
WHERE s.demo_id = $1 AND s.player_id IS NULL
  AND p.steam_id_64 = s.steam_id::bigint
```

No match, team, season, roster, or eligibility predicate. **Any Steam ID present in
`players` is credited for any demo from any match.** Leaderboards
(`demo_stats.rs:175-205`) and awards (`migrations/0064_awards.sql`) consume this directly.
A ringer's stats count, and nothing in the system can notice.

**Participation is credited to the whole roster.** The match-completion saga bumps
`matches_played / wins / losses / win_streak` on `player_game_profile` for the rostered
set, recording idempotency in `player_match_stats_applied (player_id, match_id)`
(`migrations/0073_player_match_stats_applied.sql:20-28`). **A player who sat out is
credited with the match.** Anyone benched for a season still accrues a full record.

Both are fixed by the same table, and neither is fixable without it.

## 5. Proposed model

Split the two questions:

- **Roster** (`league_team_members`) — the *pool of eligible players* for a team-season.
  Season-scoped, governed by the roster lock. Unchanged in shape.
- **Lineup** (new) — *who is playing this match*, declared per match per registration.
  Lives and dies with the match.

### Two abuses, two controls

An earlier draft of this document proposed the invariant `lineup ⊆ roster`, on the
reasoning that if a lineup could name anyone, the roster lock would mean nothing. **That
was wrong, and it contradicted the actual requirement** — the ability to pull in a sub
from *any registered player*, not merely from the pool. It also quietly re-broke the
motivating case: a sub who must already be rostered is not an ad-hoc sub.

The error was trying to make one mechanism defend against two different abuses:

| Abuse | Defended by |
|---|---|
| **A.** Stack the team mid-season — swap weak players for strong ones | **roster lock**, on the primary roster |
| **B.** Bring in a ringer for one important match | **substitute policy**, at lineup submission |

The current schema has only mechanism A, so it is forced to defend against B with it too
— which is precisely why the lock needed two predicates approximating "frozen but
flexible", and why it fails at both jobs. Separating them lets each be strict about the
thing it actually governs.

**Revised invariant:** lineup entries are *typed*. Entries marked `rostered` must be in
the pool — the lock's domain, unchanged. Entries marked `substitute` **may be any
registered player in the system**, subject to the policy in §5a. The roster lock is
therefore *not* the ringer defence and never should have been.

### What keeps the lock meaningful

Without a further constraint, "anyone can sub" is the roster lock with extra steps: a
team could field five different ringers every week and never touch its roster.

The constraint that closes this is **a per-season appearance cap per (team, player)**.
After N appearances as a substitute for the same team, that player must be *rostered* —
where the lock governs them normally. This is a common real-league rule ("after three
appearances a sub must be signed"), and it is the load-bearing rule of this whole design:
it converts unlimited ringers into a metered emergency valve, and forces genuinely
recurring players onto the roster. Everything else in §5a is a refinement.

### Where substitutes go

`substitute` **stops being a stored role and becomes derived.** It is a property of a
lineup entry (`is_substitute`), not of a roster entry. Consequences:

- `league_seasons.max_substitutes` changes meaning from "how many subs may be
  pre-registered" to "how many non-regular players may appear in one lineup" — a
  per-match rule, which is what leagues actually enforce.
- The `LeagueTeamRole::Substitute` variant is retired from the roster
  (`crates/portal-core/src/types/league_team.rs:275-283`). Everyone in the pool is simply
  a player; captaincy remains.
- The unenforced intent in `migrations/0025_league_teams_and_seasons.sql:16` —
  *"Substitutes can be on multiple teams (but cannot play against their primary team)"* —
  **becomes enforceable for the first time**, because a lineup is where you can see who
  is playing for whom in a specific match. Today it is a comment with no code behind it.
- The partial index `idx_one_primary_team_per_season`
  (`0026_restructure_league_teams.sql:275-277`) needs revisiting: with the role gone, the
  "one primary team per season" rule applies to all roster membership, and cross-team
  play is policed at lineup time instead.

### Roster lock, simplified

With subs out of the roster, the lock guards exactly one thing, so it needs exactly one
predicate — and **P-15 becomes structurally impossible rather than fixed**, because there
are no longer two predicates that can disagree.

`open | soft_lock | hard_lock` collapses toward `open | locked`. The nuance that
`soft_lock` was carrying moves to the lineup layer, where it belongs, as a per-season
policy on how many non-regulars a lineup may contain.

This is a breaking enum change; see §9.

## 5a. Substitute policy

A substitute may be any registered player. The controls are *restrictions on that
choice*, evaluated when the lineup is submitted — not restrictions on who exists in a
pool.

**The rating half of this is already built.** `EligibilityRestrictions`
(`crates/portal-domain/src/entities/eligibility.rs:10-41`) is exactly the vocabulary
required:

| Field | Use for substitutes |
|---|---|
| `max_rating_per_player` | "subs can't be above X elo" — the stated requirement |
| `max_peak_rating_per_player` | already commented *"anti-smurf check"* — the right tool for a ringer on a fresh account |
| `min_rating_per_player`, `allowed_rank_tiers` | tier-restricted divisions |
| `min_matches_played` | blocks brand-new accounts |
| `max_team_total_rating`, `max_team_average_rating` | caps the *whole lineup*, so a legal sub cannot push the team over |

`EligibilityService::check_players(restrictions, game_id, &[PlayerId])`
(`services/eligibility_service.rs:48`) already takes an arbitrary player slice, is
per-game aware, and **returns a `Vec<EligibilityViolation>` rather than failing fast** —
so it can report every problem with a proposed lineup at once. It needs **no signature
change** to be reused here. Today it has exactly two call sites, both at registration
time (`handlers/tournaments/registration.rs:127,186`; `handlers/leagues.rs:34`). Lineup
submission becomes the third, and the first that runs at match time.

Restrictions are parsed from a `settings` JSONB `"eligibility"` key
(`entities/eligibility.rs:59-66`), so a season can carry a **separate, stricter set for
substitutes** than for registration — which is usually what you want: a sub who would be
legal as a signed player may still be too strong as a one-off ringer.

The counting rules are new and belong on `league_seasons`:

| Rule | Purpose |
|---|---|
| `max_substitutes_per_match` | replaces the old `max_substitutes` reading (§9 q5) |
| `max_substitute_appearances_per_player_per_season` | **the load-bearing cap** — forces recurring subs onto the roster |
| `max_substitute_appearances_per_team_per_season` | stops a team living permanently on borrowed players |

Plus two integrity rules that are only expressible once lineups exist:

- **A sub may not appear on both lineups of the same match** (enforce with
  `UNIQUE (match_id, player_id)`, §6).
- **A sub may not play against a team they are rostered on** — this is **P-26**, stated
  in `migrations/0025_league_teams_and_seasons.sql:16` and never enforced because the rule
  is about a match and nothing bound a player to one. Whether they may sub for a rival
  *at all* should be a season flag; the minimum is that they cannot face their own team.
- **A banned player is never eligible** (`bans`, `migrations/0012_create_bans.sql`) —
  currently unchecked at match time.

**A substitute appearance must not create a roster row.** It is tempting for audit, but
it would recreate exactly the conflation this design removes. The lineup *is* the record.

### Short-handed and emergency play

This is what the current model cannot express, and it splits into three cases that
deserve different answers:

1. **Fewer players than `team_size_min`.** A lineup should be *submittable* while short —
   record it and let the match proceed or forfeit per league policy — rather than
   blocking submission. Today there is no lineup to be short *in*.
2. **A non-regular from within the pool.** A lineup entry with `is_substitute`. No
   roster change, no lock involvement.
3. **Someone outside the pool entirely — any registered player.** Also just a lineup
   entry with `is_substitute`, subject to §5a. **No roster change and no admin
   intervention.** This is the case that motivated the redesign and the one the current
   schema cannot represent at all; under the typed-entry model it is ordinary, not an
   escape hatch.

Because case 3 no longer needs a roster addition, **P-18** (the missing admin override)
stops being the answer to "we can't field a team". It remains worth doing for its own
sake — a locked roster with no override is still wrong when a *signing*, not a fill-in,
is genuinely needed — but it drops from blocking to routine.

## 6. Schema sketch

```sql
CREATE TABLE match_lineups (
    id                UUID PRIMARY KEY,
    match_id          UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    registration_id   UUID NOT NULL REFERENCES tournament_registrations(id) ON DELETE CASCADE,
    status            VARCHAR(32) NOT NULL DEFAULT 'draft',   -- draft|submitted|locked
    declared_by       UUID REFERENCES users(id),
    declared_at       TIMESTAMPTZ,
    locked_at         TIMESTAMPTZ,
    short_handed      BOOLEAN NOT NULL DEFAULT false,
    notes             TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (match_id, registration_id)
);

CREATE TABLE match_lineup_players (
    id                    UUID PRIMARY KEY,
    lineup_id             UUID NOT NULL REFERENCES match_lineups(id) ON DELETE CASCADE,
    player_id             UUID NOT NULL REFERENCES players(id),
    is_substitute         BOOLEAN NOT NULL DEFAULT false,
    was_rostered          BOOLEAN NOT NULL,   -- snapshot at declaration time
    slot                  INTEGER,
    participation_status  VARCHAR(32) NOT NULL DEFAULT 'confirmed',
        -- confirmed | no_show | left_early | substituted | removed
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (lineup_id, player_id)
);

-- one player cannot appear on both sides of a match (§5a)
CREATE UNIQUE INDEX idx_lineup_player_per_match
    ON match_lineup_players (lineup_id, player_id);
-- (enforce the cross-lineup case in the service, or via a match_id column
--  denormalized onto match_lineup_players with UNIQUE (match_id, player_id))
```

`was_rostered` is a **snapshot, not derived**. Roster membership at time T is not
reconstructable later — a player added or removed next week would silently rewrite the
history of last week's match. The lineup is an immutable historical record, so anything
the eligibility decision depended on must be frozen into it.

New columns on `league_seasons` for §5a:

```sql
ALTER TABLE league_seasons
    ADD COLUMN lineup_required                    BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN max_substitutes_per_match          INTEGER,
    ADD COLUMN max_sub_appearances_per_player     INTEGER,
    ADD COLUMN max_sub_appearances_per_team       INTEGER,
    ADD COLUMN substitute_eligibility             JSONB;  -- EligibilityRestrictions
```

FK style follows `forfeit_records` (match + registration,
`migrations/0038_forfeits.sql:4-26`). `participation_status` values are taken verbatim
from the existing design at `docs/gaming-portal-database-schema.md:1370-1426` so the two
do not drift. Structural precedent for "a player, scoped to a tournament, bound to a
team-season" already exists in `veto_delegates`
(`migrations/0044_veto_delegates.sql:4-30`).

**Note:** a `UNIQUE (match_id, player_id)` constraint would additionally prevent one
player appearing on *both* sides of a match. Worth having.

## 7. Write path: check-in becomes lineup declaration

Check-in is the natural hook. It is already a per-match window with a deadline, a
system-driven opening, and a forfeit path:

- opened by `open_check_in_window` (`crates/portal-api/src/background/mod.rs:252-313`)
- deadline on `tournament_matches.check_in_deadline` (`migrations/0031_match_lifecycle.sql:10`)
- no-show handled by `process_check_in_timeout` (`background/mod.rs:636-692`)
- `check_in_opens_at` (`0031:9`) exists and is **never written** — available

Today check-in's entire success criterion is *"a timestamp became non-null"*
(`crates/portal-db/src/adapters/tournament/match_.rs:518-560`). The change is to make
checking in **mean** submitting a lineup: extend `MatchCheckInRequest` with the player
list, write both tables transactionally, and gate the `both_checked_in()` auto-advance
(`services/tournament/match_lifecycle.rs:244`) on lineup validity against
`team_size_min` / `team_size_max` / the substitute cap.

**Two problems must be fixed on the way in.**

First — **`match_check_in` performs no authorization whatsoever**
(`crates/portal-api/src/handlers/tournaments/match_lifecycle.rs:116-155`). It takes
`AuthenticatedUser`, no `PermissionChecker`, and the service only checks that the
`registration_id` is one of the two match participants
(`services/tournament/match_lifecycle.rs:201-208`). **Any authenticated user can check in
any team for any match** — and because check-in auto-advances to `PickBan`/`InProgress`,
a stranger can force a match to start. This is filed separately as **P-24** and must be
fixed regardless of whether lineups are built. Reuse
`VetoAuthorizationService::is_captain / is_owner / is_delegate`
(`services/tournament/veto_authorization.rs:321,329,353`), which already solves exactly
this for veto.

Second — the activity predicate is inconsistent in the adapters: most use
`left_at IS NULL` (`crates/portal-db/src/adapters/league_team/member.rs:53,169,212,…`)
while `member.rs:240` and the demo auto-link query use `status = 'active'`. The lineup
eligibility query must pick one **deliberately** and the other call sites should be
reconciled to it.

## 8. What this fixes downstream

Once a lineup exists, three dead features come alive with small changes:

- **Attribution.** Gate the two Steam-ID resolution UPDATEs (`demo.rs:899`,
  `demo_stats.rs:118`) on the declared lineup instead of the global `players` table. A
  ringer no longer silently earns leaderboard positions and awards.
- **Auto-link confidence.** `list_auto_link_candidates`
  (`adapters/tournament/match_.rs:1030-1094`) currently expands to the *entire* active
  roster — precisely the set a lineup replaces, and a strictly better signal.
- **Roster-mismatch review.** Populate `unrecognized_players` at
  `adapters/demo_validator.rs:88` by diffing the demo's Steam IDs against the lineup. The
  captain-acknowledgment flow behind it is already written and tested.
- **Participation counters.** Credit `player_game_profile` from the lineup rather than
  the roster, so benched players stop accruing matches.

## 9. Migration and rollout

Nothing here needs to be a flag day.

1. **Additive first.** Create both tables. Add `league_seasons.lineup_required BOOLEAN
   NOT NULL DEFAULT false`. When false, everything behaves exactly as today and
   eligibility falls back to the roster.
2. **Fix P-24 immediately** — independent of this work, and a live authorization hole.
3. **Backfill is impossible and should not be attempted.** Historical matches have no
   lineup and the information does not exist anywhere; a lineup derived from today's
   roster would be fiction. Treat `NULL` lineup as "unknown, fall back to roster" —
   permanently, for pre-cutover matches.
4. **Opt in per season.** Enable `lineup_required` on a new season; leave existing
   seasons alone.
5. **Retire the substitute role last**, once no active season depends on it. Until then
   `LeagueTeamRole::Substitute` stays readable and stops being writable.

The `RosterLockStatus` collapse (§5) is the only genuinely breaking change and should
land in its own migration, after lineups are proven on one season.

## 10. Open questions

1. ~~Forfeit or fallback if no lineup by the deadline?~~ **DECIDED: forfeit.** If the
   system is optional it doesn't count, and a deadline with a fallback is not a deadline.

   **This costs nothing to implement.** If submitting a lineup *is* the act of checking
   in, then no lineup ⇒ not checked in ⇒ the existing no-show path already fires:
   `process_check_in_timeout` (`crates/portal-api/src/background/mod.rs:636-692`) already
   calls `process_double_forfeit` when neither side checks in (`:653-665`) and
   `process_no_show` otherwise (`:666-682`). No new forfeit logic, no new deadline, no new
   background job — the behaviour falls out of making check-in mean something.

   This is a good independent check on the design: the write path was chosen because it
   fit the domain, and the desired failure semantics turned out to already be implemented
   there. Worth confirming during implementation that a *short-handed but submitted*
   lineup (§5 case 1) is treated as checked in, not as a no-show — those must not collapse
   together.
2. **Can a lineup be edited after submission but before match start?** Real leagues
   generally allow it up to the deadline. Suggests `status: draft → submitted → locked`,
   with `locked_at` stamped on the transition to `PickBan`/`InProgress`.
3. **Does the opponent get to see the lineup before the match?** Affects whether
   declaration is public at submit time or only at lock time.
4. **Should a mid-match substitution be representable?** `participation_status` has
   `substituted` and `left_early`, implying yes, but nothing would write them without a
   post-match amendment path. Recommend deferring — record the intent in the enum,
   build the flow later.
5. ~~What happens to `max_substitutes` when its meaning changes?~~ **RESOLVED** — §6 adds
   `max_substitutes_per_match` as a new column. The old `max_substitutes` default of 2 is
   coincidentally plausible under both readings, which makes silent reinterpretation
   tempting and wrong; leave the old column in place, unread, until the substitute role is
   retired (§9 step 5).
6. **How strict should substitute eligibility be relative to registration eligibility?**
   §5a allows a separate, stricter `substitute_eligibility` set. Needs a policy default:
   inherit the season's restrictions, or require explicit configuration? Recommend
   **inherit, then allow override** — so a league that has already thought about ratings
   gets sane sub rules for free.
7. **May a player sub for a team in a league where they are rostered on a rival?** §5a
   requires at minimum that they cannot face their own team (P-26). Whether they may sub
   for a rival at all is a season flag with no obviously correct default.

## 11. Recommendation

Build it. Not primarily for the substitute ergonomics — though that is the requirement
that surfaced it — but because **the statistics this platform publishes are currently
unverifiable**, three separate features are already blocked on this table, and one of
them (§3a) is fully written and permanently unreachable.

Sequence: **P-24 (authorization hole) → tables + check-in write path → substitute policy
(§5a, reusing `EligibilityService`) → attribution gating → roster-mismatch revival →
roster-lock simplification.**

Hold **P-15** until this lands; its clean fix is a consequence of the redesign rather
than a patch. **P-18** (admin override) is no longer on the critical path — §5 case 3 is
answered by a lineup entry, not by an override — but remains worth doing on its own merit.

The single most important rule to get right is the **per-player substitute appearance
cap** (§5). Everything else in §5a tunes who may sub; that cap is what stops "any
registered player may sub" from quietly becoming a bypass of the roster lock.
