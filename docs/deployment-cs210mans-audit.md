# Deployment audit — www.cs210mans.uk (2026-07-19)

Exploratory audit of the target deployment host, plus a deployment
recommendation, a user-migration plan for the previous (Python) site, and a
plan for retaining Steam login.

## 1. What is on the box

| Item | Finding |
|---|---|
| Host | Linode VM, `www.cs210mans.uk` / `178.79.138.74` |
| OS | Alpine Linux 3.20.10, kernel 6.6.119-virt, x86_64, OpenRC |
| Size | 1 vCPU, 2 GB RAM (+512 MB swap), 48.7 GB disk (5.6 GB used) |
| Uptime | 175 days |
| Ingress | Caddy on 80/443 — **run manually as `sudo caddy_aws` inside a tmux session**, not a supervised service |
| Frontend | Old Vite SPA served from `/var/www` (5.3 MB) |
| API | Caddy proxies `/api/*` → `localhost:8000` — **nothing is listening; the old backend is down** |
| Demos | `demos.cs210mans.uk` reverse-proxies a Linode Object Storage bucket (`cs2-10mans-demo-files`, gb-lon-1) with S3 credentials inline in the Caddyfile |
| Old backend | `~/src/backend` — FastAPI + SQLModel + asyncpg + Alembic, git remote `mtottenh/tenmans_be` |
| Old DB | Postgres 15 via compose (podman). Only a rootless **test** volume is visible; the production volume (if any survives) is under root podman — needs `sudo podman ps -a && sudo podman volume ls` to confirm |
| User data | `~/src/backend/players.csv` — full export of **77 players**: `Player ID, Steam ID, Name, Status, Current Team, Is Captain, Current ELO, Highest ELO, Created At` |
| Personal data | `~/LAPTOP_BACKUP` (396 MB) — must be preserved before any re-image |

### Secrets that need rotating

Both of these are sitting in plaintext in configs on the box (and both
predate this audit):

- **Linode Object Storage access/secret key** — inline in `/etc/caddy/Caddyfile`.
- **Steam Web API key** — in `~/src/backend/.env`.

Rotate both when we cut over; put the replacements in the Ansible vault
(`vault_linode_access_key` / `vault_linode_secret_key` slots already exist).

## 2. Deployment recommendation

**Re-image the Linode to Debian 12 and run our existing `deploy/ansible`
stack against it.**

Why this beats keeping Alpine:

- Our deployment is deb-based (portal + demo-stats CI already builds debs);
  Alpine has no dpkg and musl would force a separate build pipeline.
- The old site is effectively already gone from the box — the backend is
  down, only the static frontend still serves. There is no live service to
  preserve in place.
- Everything the box currently does (Caddy TLS termination, static SPA,
  API reverse-proxy, Postgres) is exactly what `site.yml` deploys — but
  supervised by systemd instead of a root Caddy in a tmux pane.

Pre-wipe preservation checklist (all small enough to scp down or stash in
the object-storage bucket):

1. `sudo podman ps -a && sudo podman volume ls` — if a prod postgres volume
   exists, start the container and `pg_dump` `cs210mans_db`.
2. `~/LAPTOP_BACKUP` (396 MB), `~/src/backend/.env`, `players.csv`,
   `/etc/caddy/Caddyfile`, `/var/www` (old frontend, for posterity).
3. `~/src/backend` is on GitHub (`mtottenh/tenmans_be`) — verify it's pushed
   (`git status` was clean at audit time).

Sizing: 2 GB / 1 vCPU is workable for Caddy + API + Postgres, but demo
parsing is the pressure point (demo-stats holds a full demo in memory).
Either resize to 4 GB, or keep demo-stats at concurrency 1 and accept the
512 MB swap as headroom. The existing bucket means `portal-storage`'s S3
backend can point at `cs2-10mans-demo-files` (with the rotated keys) and the
`demos.` subdomain proxy behaviour is preserved.

Gaps to close in `deploy/ansible` before the run (already tracked):
frontend build+ship step (`/var/www/portal`), demo-stats + scanner roles in
`site.yml`, real vault values, inventory host.

## 3. User migration plan

Old model: `players(id uuid, name, steam_id unique NOT NULL, email nullable,
auth_type steam|email, password_hash bcrypt nullable, current_elo,
highest_elo)`. Every account has a Steam ID; only `email`-type accounts have
passwords (bcrypt via passlib).

**Confirmed from the recovered prod DB** (root-podman volume
`backend_postgres_data`, extracted 2026-07-19, dump at
`C:\Users\Max\cs210mans_db.dump.sql` — contains emails + a password hash, do
NOT commit): **90 players** (89 steam-auth, exactly 1 email+bcrypt account:
`gwoody`), **10 teams** with 72 roster entries in "Season 2", and **zero**
tournaments/fixtures/results/pugs/bans — no competition history exists to
migrate. The DB is newer than `players.csv` (12 players joined after the CSV
export; latest 2026-01-07, i.e. the site was live until the January reboot
broke podman). **Migrate from the dump, not the CSV.**

**Decision (2026-07-19): migrate players only.** Teams are reset on the new
platform — no team/roster/logo import, which also drops the need to preserve
the `backend_logo_store` / `backend_map_store` volumes.

Ours: `users` (argon2id password) 1:1 `players` (`steam_id_64` set-once).

Plan, in order of preference per account type:

- **Steam-auth users (the majority):** don't migrate credentials at all —
  there are none. Pre-provision `users` + `players` rows from the old data
  (username = old `name`, `steam_id_64` = old `Steam ID`, created_at
  preserved) with no usable password, and let **Steam login (§4)** match on
  `steam_id_64` at first sign-in. Nothing for the user to do.
- **Email users:** small set (needs the DB dump to enumerate). Two options:
  (a) import the bcrypt hash and add a verify-then-rehash-to-argon2 fallback
  in our login path; (b) import with no password and require "reset
  password" once. Recommendation: **(b)** — the fallback code isn't worth
  carrying for a handful of accounts on a site where everyone has Steam.
- **ELO (current/highest):** our platform has no generic ladder rating yet.
  Import as a `player_rating_history` snapshot per player (source-tagged
  `tenmans_import`) so it's queryable, and archive `players.csv` in the
  repo's `docs/import/` for reference. Decide later whether the new
  matchmaking design consumes it.
- **Teams:** not migrated (decision above) — teams re-form on the new
  platform.
- **Demo files:** already in the object-storage bucket; the scanner's batch
  catalog endpoint can ingest them into the demo catalog as-is.

Mechanism: a `portal-cli` subcommand (`portal-cli users import-tenmans
--csv players.csv [--dump dump.sql]`) that upserts users+players idempotently
by `steam_id_64`. CSV alone suffices for the steam-auth majority even if the
prod DB volume turns out to be gone.

## 4. Steam login (port from the old site)

Old implementation (`src/auth/routes.py`): standard **Steam OpenID 2.0** —
`GET /login/steam` begins an OpenID checkid_setup against
`https://steamcommunity.com/openid`, the callback verifies the assertion and
extracts the SteamID64 from the claimed-id URL
(`steamcommunity.com/openid/id/<id64>`), then find-or-create-player by
steam_id and issue a session.

Port to the Rust backend (no heavyweight OpenID dep needed — Steam's flow is
a fixed template):

1. `GET /v1/auth/steam/login` → 302 to
   `https://steamcommunity.com/openid/login` with the static checkid_setup
   params and `openid.return_to = https://<host>/v1/auth/steam/callback`.
2. `GET /v1/auth/steam/callback` → re-POST the received params to
   steamcommunity.com with `openid.mode=check_authentication` (direct
   verification — this is the security-critical step; `is_valid:true` or
   reject), parse the SteamID64 out of `openid.claimed_id`, **validate
   `return_to` matches our own host**.
3. Find `players.steam_id_64` → issue our JWT + refresh token exactly like
   password login. No match → create user+player (auth_provider `steam`,
   no password), or — during the migration window — match the pre-provisioned
   imported row.
4. Optional enrichment: `ISteamUser.GetPlayerSummaries` with the (rotated)
   Steam API key for persona name/avatar. Not required for auth.

Schema change: `users.auth_provider` (`local` | `steam`, default `local`) and
`password_hash` nullable for `steam` accounts; login-by-password rejects
provider-`steam` accounts with a "sign in through Steam" error. Frontend
gets the standard "Sign in through Steam" button on the login page.

Estimated effort: backend flow + tests ~1 day, import CLI ~½ day, frontend
button + e2e ~½ day, re-image + ansible converge ~½ day.

## 5. Open items

1. **Prod DB probably doesn't exist on this box.** Follow-up findings:
   podman (root *and* rootless) fails to initialize storage — the `overlay`
   kernel module isn't loaded (post-reboot), so no containers can have been
   running since. The only rootless volume (`backend_postgres_test_data`) is
   8 KB / empty, there's no native postgres, no dumps in home, and Caddy has
   been proxying `/api/*` to a dead port. `players.csv` (root-owned,
   2025-03-02) looks like the surviving export and is sufficient for the §3
   migration. Definitive check (needs sudo):
   `sudo find / -xdev -name PG_VERSION` — any surviving postgres data dir
   will contain one.
2. Decide ELO import target (rating-history snapshot vs archive-only).
3. Rotate the two exposed secrets at cutover (§1).
4. Ansible gaps listed in §2.
