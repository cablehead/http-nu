# Seed Data

Starter data the dev team loads into the test and development databases. The Product Owner owns *what the data looks like*; the dev team owns the loader script that pulls these CSVs into the database.

This is **fictional sample data** for the Thailand domestic pilot — realistic-looking Thai school names, teams, and people, but no real personal contact details. Real pilot contacts live in [../README.md](../README.md), which is in a private repo.

---

## Why this exists

A dev team needs realistic data to build and demo against. If they invent it themselves, the data ends up unrealistic — wrong age groups, wrong tournament shapes, wrong roster sizes — and bugs hide until real users arrive. The PO already knows what real Thai school basketball looks like, so the PO defines the seed and the dev team consumes it.

---

## Files

Each file is one entity. IDs are human-readable so the PO can edit by hand without breaking links between files.

| File | What it contains |
|---|---|
| [schema.md](schema.md) | Column-by-column rules: allowed values, required vs optional, uniqueness, FK semantics. Source of truth for the data model. |
| **Reference tables** | |
| [provinces.csv](provinces.csv) | The 77 Thai provinces — referenced by code from orgs, venues, events |
| [age_groups.csv](age_groups.csv) | Age categories (U10–U21, Open, Senior) — referenced from teams and divisions |
| [positions.csv](positions.csv) | Basketball positions (PG, SG, SF, PF, C) — referenced from players |
| [genders.csv](genders.csv) | Gender categories (M, F, Co-ed) — referenced from teams and divisions |
| [roles.csv](roles.csv) | The six actor types — referenced from users |
| [event_types.csv](event_types.csv) | The four event types — referenced from events |
| [org_types.csv](org_types.csv) | School / club / federation — referenced from orgs |
| [coach_roles.csv](coach_roles.csv) | Head / assistant / manager — referenced from team_coaches |
| [relationships.csv](relationships.csv) | Parent / grandparent / guardian / other — referenced from guardians |
| [locales.csv](locales.csv) | Supported UI languages (th, en) — referenced from users |
| [user_statuses.csv](user_statuses.csv) | Account lifecycle (Active, Pending Approval, Suspended, Deactivated) — referenced from users |
| [notification_channels.csv](notification_channels.csv) | Delivery channels (LINE, Email, SMS, Push, In-app) — Thailand-realistic with LINE first |
| [notification_types.csv](notification_types.csv) | Notification kinds (MATCH_START, SCORE_UPDATE, WEEKLY_DIGEST, etc.) — referenced from user_notification_preferences |
| [divisions.csv](divisions.csv) | Competitive divisions (age × gender × skill tier) — referenced from event_teams |
| **Authorisation (full Zanzibar / relation-based)** | |
| [object_types.csv](object_types.csv) | What gets authorised on (EVENT, TEAM, PLAYER, ORG, DIVISION, PLATFORM) |
| [relations.csv](relations.csv) | How a user connects to an object (OWNER, HEAD_COACH, GUARDIAN, etc.) — tuples derived from existing entities/join tables |
| [actions.csv](actions.csv) | What can be done (EDIT_EVENT, DELETE_EVENT, MANAGE_ROSTER, etc.) tagged by object_type |
| [permissions.csv](permissions.csv) | Auth policy as data: action × relation × event_type → granted. Source of truth for [matrix.md](../../access/matrix.md). |
| **Core entities** | |
| [users.csv](users.csv) | One row per person — covers all six [actor types](../../domain/actors.md). Each user has exactly one role. |
| [orgs.csv](orgs.csv) | Schools, clubs, federations — the organisations teams belong to |
| [venues.csv](venues.csv) | Gyms, sports halls, stadiums where events are held |
| [teams.csv](teams.csv) | Team profiles — each linked to an org |
| [players.csv](players.csv) | Players — optionally linked to a user account (minors may have no account) |
| [events.csv](events.csv) | One of each [event type](../../domain/event-types.md) — Tournament, League, Camp, Showcase |
| **Join tables (relationships)** | |
| [team_coaches.csv](team_coaches.csv) | Which coaches coach which teams (head / assistant / manager) |
| [player_teams.csv](player_teams.csv) | Which players are on which teams, with effective dates (a player can be on multiple teams) |
| [guardians.csv](guardians.csv) | Which Spectator (parent) is the guardian of which Player |
| [event_venues.csv](event_venues.csv) | Which venues host which events (one primary per event, plus secondaries) |
| [event_co_organizers.csv](event_co_organizers.csv) | Additional organizers on an event (provides CO_ORGANIZER relation tuples) |
| [subscriptions.csv](subscriptions.csv) | Polymorphic follower table — user follows player/team/event (provides FOLLOWER relation tuples for the notification engine) |
| [user_notification_channels.csv](user_notification_channels.csv) | Per-user delivery preferences — which channels (LINE/Email/etc) each user has set up, with verified status |
| [user_notification_preferences.csv](user_notification_preferences.csv) | Per-user, per-type, per-channel opt-in/out (e.g. "score updates via LINE only") |
| [event_teams.csv](event_teams.csv) | Team registrations into events (Tournament, League, Showcase) |
| [event_players.csv](event_players.csv) | Individual player registrations into events (Camp, Showcase) |

---

## Format rules

These rules exist so the dev team's loader can parse the files reliably.

- **Encoding:** UTF-8 (Thai script must render correctly)
- **Separator:** comma
- **Header row:** required, exact column names as shown in each file
- **IDs:** prefixed and stable (`usr_`, `team_`, `evt_`, etc.). Never reuse an ID even after deletion.
- **Foreign keys:** by ID. Dev team validates referential integrity at load time.
- **Phone numbers:** E.164 format with country code (`+66812345678`, no dashes or spaces)
- **Dates:** ISO 8601 (`YYYY-MM-DD`). Internal storage is Gregorian; UI may display Buddhist calendar.
- **Locale codes:** `th` or `en`
- **Email domains:** use `.test` TLD for fictional addresses (reserved by IETF, prevents accidental sends)
- **Empty values:** leave the cell blank (e.g. a player with no user account has an empty `user_id`)

---

## How the PO edits this

1. Open the CSV in Google Sheets or Excel
2. Add or change rows
3. Save / export as CSV (UTF-8 encoding)
4. Commit the change to this repo

If a new entity type is needed (e.g. sponsors, divisions), discuss with the dev team before adding a new file — the schema is shared.

---

## How the dev team consumes this

The dev repo (the actual application code) reads these CSVs at build or test-setup time and inserts rows into the database. Implementation is up to the dev team — common patterns are:

- A make/mise task that copies these files into the dev repo at build time
- A git submodule pointing at this repo
- A CI step that pulls the latest seed before running tests

The contract is **the file names and column headers in this folder** — those should not change without coordination.

---

## Regenerating derived views

[../../access/matrix.md](../../access/matrix.md) is generated from the auth CSVs in this folder. Whenever `actions.csv`, `relations.csv`, `permissions.csv`, `object_types.csv`, or `event_types.csv` changes, regenerate it:

```sh
mise run access:regenerate-matrix            # write the new view
mise run access:regenerate-matrix --check    # CI-friendly: exit non-zero if out of date
```

The script lives in [../../scripts/regenerate-matrix.nu](../../scripts/regenerate-matrix.nu) and uses nushell only (no third-party deps) — invoked via the mise task so nushell is auto-resolved from the toolchain. Add `--check` to a CI step to fail the build if matrix.md isn't in sync with the CSVs.

---

## What the seed covers

Enough rows to exercise every flow in the [access matrix](../../access/matrix.md):

- All 6 actor types have at least one user
- All 4 event types have at least one event
- At least 2 teams registered per Tournament/League/Showcase
- At least 2 players registered per Camp
- At least one event in Bangkok and one outside (Chiang Mai), to test multi-province display

Sample size is intentionally small — ~5 rows per file. The dev team only needs enough data to demonstrate that flows work end-to-end. Scale comes later, from real users.

---

## Out of scope

- **Real personal data** — never put real names, emails, or phone numbers here
- **Production data** — this is dev/test only
- **Performance / load testing data** — if the dev team needs 10,000 fake teams to load test, they generate it programmatically; not the PO's job
