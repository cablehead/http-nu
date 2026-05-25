# Seed Data Schema

This document makes the implicit schema in the seed CSVs explicit. The CSVs declare *shape* (fields, links, nullability); this document declares *meaning* (allowed values, uniqueness, what each link means).

The dev team should treat this file as the source of truth for the data model. If a CSV cell value disagrees with this doc, the doc wins — fix the CSV.

---

## Conventions (apply across all CSVs)

- **IDs**: text, prefixed by entity type (`usr_`, `org_`, `team_`, `ply_`, `evt_`, `ven_`, `div_`). Unique globally within their file. Never reused after deletion.
- **Reference codes**: stable text codes in reference tables (e.g. `province_code` = `BKK`, `age_group_code` = `U16`, `position_code` = `PG`, `gender_code` = `M`). All controlled enums are reference tables, not free text — this guarantees referential integrity and prevents typo drift.
- **Foreign keys**: column name ends in `_id` (entity FK) or `_code` (reference FK). The dev team enforces referential integrity at load time.
- **Required fields**: must have a non-empty value in every row.
- **Optional fields**: may be left blank.
- **Dates**: ISO 8601 (`YYYY-MM-DD`). Internal storage is Gregorian.
- **Phone numbers**: E.164 format with country code (`+66812345678`).
- **Locale codes**: `th` (Thai) or `en` (English).
- **Bilingual names**: `name_th` is optional, `name_en` is required. UI displays based on user locale, falls back to `name_en` if `name_th` is missing.
- **Booleans**: `true` or `false` (lowercase).

---

# Reference tables

Stable lookup lists, owned by the PO, referenced by code from core entities.

## provinces.csv

The 77 Thai provinces. Referenced by code from `orgs`, `venues`, `events`. The seed includes a starter set of common provinces; PO extends as the pilot expands geographically.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | 3-letter uppercase (e.g. `BKK`, `CMI`) | Unique. Stable across renames. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |

**Uniqueness**: `code` globally.

---

## age_groups.csv

The basketball age categories. Referenced by code from `teams` and `divisions`.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | e.g. `U10`, `U12`, `U14`, `U16`, `U18`, `U21`, `OPEN`, `SENIOR` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |
| `min_age` | integer | N | inclusive minimum age | Empty = no minimum (e.g. youth groups). |
| `max_age` | integer | N | inclusive maximum age | Empty = no maximum (e.g. `OPEN`, `SENIOR`). |

**Uniqueness**: `code` globally.

---

## positions.csv

Basketball playing positions. Referenced by code from `players`.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `PG`, `SG`, `SF`, `PF`, `C` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script (typically same as code) | |
| `full_name_en` | text | Y | e.g. `Point Guard` | Used in detailed player profile views. |

**Uniqueness**: `code` globally.

---

## genders.csv

Gender categories used by teams and divisions. Display names are basketball-conventional (`Boys`/`Girls` for youth) — UI may swap to `Men`/`Women` for adult age groups.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `M`, `F`, `COED` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |

**Uniqueness**: `code` globally.

---

## roles.csv

The six [actor types](../../domain/actors.md). Referenced by code from `users`. Driving auth off this table (rather than a hard-coded enum) lets the dev team load it directly into a Zanzibar-style policy store (SpiceDB / OpenFGA / Permify).

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `ADMIN`, `ORGANIZER`, `COACH`, `PLAYER`, `SPECTATOR`, `REFEREE` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |
| `description_en` | text | Y | one-line role description | Sourced from [actors.md](../../domain/actors.md). |

**Uniqueness**: `code` globally.

**Note**: The canonical narrative definition of each role still lives in [actors.md](../../domain/actors.md). This CSV is the data form of that domain doc.

---

## event_types.csv

The four [event types](../../domain/event-types.md). Referenced by code from `events`. Used together with `roles` and `features` to drive the access matrix.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `TOURNAMENT`, `LEAGUE`, `CAMP`, `SHOWCASE` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |
| `description_en` | text | Y | one-line type description | Sourced from [event-types.md](../../domain/event-types.md). |

**Uniqueness**: `code` globally.

---

## event_formats.csv

The basketball game format for an event. Referenced by code from `events`. Phase 1 treats format as a label only — the scoring engine, bracket model, and score-entry UI are format-agnostic. See [Q-and-A §23.2](../../decisions/Q-and-A.md).

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `5x5`, `3x3` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |

**Uniqueness**: `code` globally.

---

## org_types.csv

The kinds of organisations teams can belong to. Referenced by code from `orgs`.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `SCHOOL`, `CLUB`, `FEDERATION`, `GRASSROOTS` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |

**Uniqueness**: `code` globally.

---

## coach_roles.csv

The roles a coach can hold on a team. Referenced by code from `team_coaches`. Distinct from the user-level [roles.csv](roles.csv) (which is the actor type).

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `HEAD`, `ASSISTANT`, `MANAGER` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |

**Uniqueness**: `code` globally.

---

## relationships.csv

The kinds of guardian–player relationships. Referenced by code from `guardians`.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `PARENT`, `GRANDPARENT`, `GUARDIAN`, `OTHER` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |

**Uniqueness**: `code` globally.

---

## locales.csv

Supported UI languages. Referenced by code from `users`.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | IETF BCP 47 language tag (e.g. `th`, `en`) | Lowercase — overrides the uppercase-codes convention to follow the IETF standard. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |

**Uniqueness**: `code` globally.

---

## user_statuses.csv

Lifecycle states a user account can be in. Referenced by code from `users`. Drives the gated-signup flows (e.g. a `REFEREE` self-signup creates an account with `PENDING_APPROVAL`; an admin runs `APPROVE_REFEREE` to flip it to `ACTIVE`).

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `ACTIVE`, `PENDING_APPROVAL`, `SUSPENDED`, `DEACTIVATED` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |
| `description_en` | text | Y | one-line state description | |

**Uniqueness**: `code` globally.

---

## notification_channels.csv

The delivery channels notifications can be sent over. Referenced by code from `user_notification_channels`. The notification engine selects which channels to dispatch on, given a user's enabled+verified set.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `LINE`, `EMAIL`, `SMS`, `PUSH`, `IN_APP` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |
| `address_format` | text | Y | one-line description of what an address for this channel looks like | E.g. `LINE personal ID`, `RFC 5322 email`. The dev team validates per-channel. |
| `description_en` | text | Y | one-line channel description | Includes Thai-market context (e.g. LINE has 90%+ penetration). |

**Uniqueness**: `code` globally.

---

## object_types.csv

The kinds of resources authorisation applies to (the **object types** in Zanzibar/SpiceDB/OpenFGA terms). Referenced by code from `relations` and `actions`.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | `EVENT`, `TEAM`, `PLAYER`, `ORG`, `DIVISION`, `PLATFORM` | Unique. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |
| `description_en` | text | Y | one-line type description | |

**Uniqueness**: `code` globally.

**Note**: `EVENT` is one type; the four event subtypes (`TOURNAMENT`, `LEAGUE`, `CAMP`, `SHOWCASE`) live in [event_types.csv](event_types.csv) and are used to scope event-type-specific permissions. `PLATFORM` is the synthetic object for global actions (sign-in, install app, manage all users).

---

## relations.csv

The relations a user can have to an object (the **relations** in Zanzibar/SpiceDB/OpenFGA terms). The actual relation tuples are **derived from existing entity/join tables** — `derived_from` documents where each tuple comes from. The dev team writes a view/loader that materialises these tuples for the auth store.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | SCREAMING_SNAKE_CASE (e.g. `OWNER`, `HEAD_COACH`) | Unique. |
| `object_type_code` | FK | Y | references `object_types.code` | The type of object this relation applies to. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |
| `derived_from` | text | Y | free text | Where the tuples for this relation come from in the existing data (e.g. `events.organizer_user_id`, `team_coaches where coach_role_code=HEAD`). |

**Uniqueness**: `code` globally.

**`PLATFORM` relations** (e.g. `ANY_ORGANIZER`, `PLATFORM_ADMIN`, `PUBLIC`) are how role-based grants are expressed in the relation model — they apply at the platform level, not to a specific object instance.

---

## actions.csv

What can be done (the **actions/permissions** in Zanzibar/SpiceDB/OpenFGA terms). Replaces the old `features.csv` — actions are explicit verbs (`EDIT_EVENT`, `DELETE_EVENT`) tagged with the object type they act on.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | SCREAMING_SNAKE_CASE (e.g. `EDIT_EVENT`, `MANAGE_ROSTER`) | Unique. |
| `object_type_code` | FK | Y | references `object_types.code` | The object type this action acts on. `PLATFORM` for global actions. |
| `category` | text | Y | one of `Auth`, `Events`, `Teams`, `Players`, `Schedules`, `Scores`, `Rankings`, `Live`, `AI`, `Admin` | UI grouping. Could become `action_categories.csv` later. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |

**Uniqueness**: `code` globally.

---

## permissions.csv

The authorisation policy expressed as data — one row per `(action, relation) → granted` rule, optionally scoped to an event subtype. Loadable directly into a Zanzibar-style auth store (SpiceDB / OpenFGA / Permify). **Sparse**: rows only exist where access is granted — absence means denied.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `action_code` | FK | Y | references `actions.code` | |
| `relation_code` | FK | Y | references `relations.code` | The relation a user must hold to be granted this action. The relation's `object_type_code` must match the action's `object_type_code` (or be a `PLATFORM` relation). |
| `event_type_code` | FK | N | references `event_types.code` | Only used when the action is on `EVENT` and applies only to certain event subtypes. Empty = applies to all event types. |

**Uniqueness**: composite (`action_code`, `relation_code`, `event_type_code`).

**Reading a row**: a row `(EDIT_EVENT, OWNER, "")` means: a user who has the `OWNER` relation to an event can perform `EDIT_EVENT` on that event, regardless of event subtype. A row `(REGISTER_PLAYER_FOR_EVENT, GUARDIAN, CAMP)` means: a user who is a `GUARDIAN` of a player can register that player for events of subtype `CAMP`.

**Permission check**: "can user U perform action A on object O?" → look up the user's relations to O (or to PLATFORM), then check `permissions.csv` for any row matching `(A, relation, O.event_type_code if applicable)`. Row exists = granted.

**Convention**: PO edits this CSV to change auth policy. [matrix.md](../../access/matrix.md) is regenerated as a human-readable view of these rules.

---

# Core entities

The main records that the PO and dev team reason about.

## users.csv

One row per person. Covers all six [actor types](../../domain/actors.md). **Each user has exactly one role** — a person who, for example, coaches one team and plays on another needs two separate accounts.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `id` | text | Y | prefix `usr_` | Unique. |
| `role_code` | FK | Y | references `roles.code` | Exactly one role per user. |
| `status_code` | FK | Y | references `user_statuses.code` | Lifecycle state. New `REFEREE` accounts default to `PENDING_APPROVAL`; everything else defaults to `ACTIVE`. |
| `name_th` | text | N | Thai script | |
| `name_en` | text | Y | Latin script | Used as fallback display name. |
| `email` | text | N | RFC 5322 | Optional — many Thai users have no email, only phone + LINE. |
| `phone` | text | N | E.164 (`+66...`) | At least one of `email`, `phone`, `line_id` must be present. |
| `line_id` | text | N | LINE personal ID | Used for notifications and contact. |
| `locale_code` | FK | Y | references `locales.code` | Drives UI language. |

**Uniqueness**: `id` globally; `email` globally if present; `phone` globally if present; `line_id` globally if present.

---

## orgs.csv

Schools, clubs, federations — the organisations that teams belong to.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `id` | text | Y | prefix `org_` | Unique. |
| `name_th` | text | N | Thai script | |
| `name_en` | text | Y | Latin script | |
| `org_type_code` | FK | Y | references `org_types.code` | |
| `city` | text | Y | free text | |
| `province_code` | FK | Y | references `provinces.code` | |

**Uniqueness**: `id` globally.

---

## venues.csv

Physical locations where events are held.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `id` | text | Y | prefix `ven_` | Unique. |
| `name_th` | text | N | Thai script | |
| `name_en` | text | Y | Latin script | |
| `address` | text | Y | free text | Single-line street address. |
| `city` | text | Y | free text | |
| `province_code` | FK | Y | references `provinces.code` | |

**Uniqueness**: `id` globally.

---

## teams.csv

Team profiles. Each team belongs to exactly one org. Coaches are linked via [team_coaches.csv](team_coaches.csv) — a team can have multiple coaches (head + assistants + manager).

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `id` | text | Y | prefix `team_` | Unique. |
| `name_th` | text | N | Thai script | |
| `name_en` | text | Y | Latin script | |
| `org_id` | FK | Y | references `orgs.id` | Every team belongs to an org. |
| `age_group_code` | FK | Y | references `age_groups.code` | The team's age category — drives division eligibility. |
| `gender_code` | FK | Y | references `genders.code` | |

**Uniqueness**: `id` globally.

---

## players.csv

Individual players. May or may not have a user account — minors often don't. Team links live in [player_teams.csv](player_teams.csv) — a player can belong to multiple teams in the same season (e.g. school + club, or U16 called up to U18).

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `id` | text | Y | prefix `ply_` | Unique. |
| `name_th` | text | N | Thai script | |
| `name_en` | text | Y | Latin script | |
| `user_id` | FK | N | references `users.id` where `role_code = PLAYER` | Empty = minor or non-account-holder; coach manages on their behalf. |
| `jersey_number` | integer | Y | `0`–`99` | Unique per (player, team) combination — see [player_teams.csv](player_teams.csv). Note: same player may wear different numbers on different teams. |
| `position_code` | FK | Y | references `positions.code` | |
| `dob` | date | Y | `YYYY-MM-DD` | Used to validate `team.age_group_code` eligibility. |

**Uniqueness**: `id` globally.

---

## divisions.csv

Competitive divisions — what teams compete in within an event. Independent entity (not derived from team age_group + gender) so organizers can create custom skill tiers like "U16 Premier" vs "U16 Recreational" within a single age group.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `id` | text | Y | prefix `div_` | Unique. |
| `name_th` | text | N | Thai script | |
| `name_en` | text | Y | Latin script | |
| `age_group_code` | FK | Y | references `age_groups.code` | |
| `gender_code` | FK | Y | references `genders.code` | |
| `skill_tier` | text | N | free text (e.g. `Premier`, `Recreational`) | Optional skill bracket within an age/gender division. |

**Uniqueness**: `id` globally.

**Convention**: Divisions are reusable across events. A "U16 Boys" division can be referenced by multiple tournaments. Custom one-off divisions can be created and used by a single event — that's allowed but the row stays in this table.

---

## events.csv

A scheduled tournament, league, camp, or showcase. Venue links live in [event_venues.csv](event_venues.csv) — an event can use multiple venues.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `id` | text | Y | prefix `evt_` | Unique. |
| `name_th` | text | N | Thai script | |
| `name_en` | text | Y | Latin script | |
| `type_code` | FK | Y | references `event_types.code` | |
| `format_code` | FK | Y | references `event_formats.code` | `5x5` or `3x3`. Defaults to `5x5`. Label only for Phase 1 — see [Q-and-A §23.2](../../decisions/Q-and-A.md). |
| `organizer_user_id` | FK | Y | references `users.id` where `role_code = ORGANIZER` | One primary organizer per event. |
| `org_id` | FK | N | references `orgs.id` | The organising body (school, club, federation, grassroots). Empty = individual organiser, no org affiliation. Display uses `COALESCE(org.name, user.name)` for the "organised by" label. See [Q-and-A §23.1](../../decisions/Q-and-A.md). |
| `start_date` | date | Y | `YYYY-MM-DD` | |
| `end_date` | date | Y | `YYYY-MM-DD` | Must be `>= start_date`. |
| `city` | text | Y | free text | Denormalised for browse/filter performance — usually the city of the primary venue. |
| `province_code` | FK | Y | references `provinces.code` | Denormalised for browse/filter. |
| `is_fiba_certified` | bool | Y | `true` / `false` | Self-declared by the organiser at event creation. No admin review for Phase 1 — see [Q-and-A §23.3](../../decisions/Q-and-A.md). Defaults to `false`. |

**Uniqueness**: `id` globally.

---

# Join tables

Many-to-many relationships between core entities.

## team_coaches.csv

Links coaches to teams. A team can have multiple coaches; a coach can coach multiple teams.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `team_id` | FK | Y | references `teams.id` | |
| `user_id` | FK | Y | references `users.id` where `role_code = COACH` | |
| `coach_role_code` | FK | Y | references `coach_roles.code` | Exactly one `HEAD` coach per team is recommended (not enforced). |

**Uniqueness**: composite (`team_id`, `user_id`) — a coach holds one role per team.

---

## player_teams.csv

Links players to teams over time. A player can be on multiple teams concurrently (e.g. school team + club team) and can move between teams during a season.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `player_id` | FK | Y | references `players.id` | |
| `team_id` | FK | Y | references `teams.id` | |
| `from_date` | date | Y | `YYYY-MM-DD` | When the player joined this team. |
| `to_date` | date | N | `YYYY-MM-DD` | When the player left. Empty = still on the team. |

**Uniqueness**: composite (`player_id`, `team_id`, `from_date`) — a player can be re-added to the same team after leaving.

---

## guardians.csv

Links a Spectator (parent/guardian) to a Player so the parent can follow that player's schedule and receive notifications. Required for the Spectator notification feature to be useful for the parent segment.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `user_id` | FK | Y | references `users.id` where `role_code = SPECTATOR` | |
| `player_id` | FK | Y | references `players.id` | |
| `relationship_code` | FK | Y | references `relationships.code` | |

**Uniqueness**: composite (`user_id`, `player_id`).

**Note**: Production will need a verification flow (e.g. coach confirms) before a Spectator can claim a guardian relationship — privacy/PII risk if anyone can claim any minor as their child.

---

## user_notification_channels.csv

Per-user channel preferences — which channels each user has set up, the address for each, and whether they're enabled. The notification engine reads this to decide where to deliver.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `user_id` | FK | Y | references `users.id` | |
| `channel_code` | FK | Y | references `notification_channels.code` | |
| `address` | text | Y | format depends on channel (see [notification_channels.csv](notification_channels.csv) `address_format`) | E.g. LINE personal ID, email address, E.164 phone, push subscription token. |
| `address_label` | text | Y | short label (e.g. `primary`, `family_group`, `work`, `alerts`) | Distinguishes multiple addresses on the same channel. Default `primary` for the user's main address. |
| `is_enabled` | boolean | Y | `true`, `false` | User can disable a channel without deleting the address. |
| `verified_at` | date | N | `YYYY-MM-DD` | Empty = unverified. Address ownership must be verified (LINE add-friend, email click, SMS OTP) before this channel is used for delivery. |

**Uniqueness**: composite (`user_id`, `channel_code`, `address_label`). One user can have multiple addresses on the same channel (e.g. a Spectator wants notifications delivered to both their personal LINE and a family group LINE chat — common pattern in Thai households).

**Note**: [users.csv](users.csv) keeps `email`, `phone`, `line_id` as **primary contact** (used by Better Auth for login, by admin for direct contact). This table holds **delivery preferences**. They'll usually be the same value, but allowing them to differ is flexible.

---

## notification_types.csv

The kinds of notifications the system can send. Referenced by code from `user_notification_preferences`. Lets users opt in/out per type and per channel.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `code` | text | Y | SCREAMING_SNAKE_CASE (e.g. `MATCH_START`, `WEEKLY_DIGEST`) | Unique. |
| `category` | text | Y | one of `Live`, `Reminder`, `Discovery`, `Team`, `Registration`, `Digest`, `Announcement`, `Workflow` | UI grouping in the notification settings screen. |
| `name_th` | text | Y | Thai script | |
| `name_en` | text | Y | Latin script | |
| `description_en` | text | Y | one-line type description | |

**Uniqueness**: `code` globally.

---

## user_notification_preferences.csv

Per-user, per-type, per-channel opt-in/out. Lets a user say "score updates via LINE only" or "weekly digest via email only".

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `user_id` | FK | Y | references `users.id` | |
| `notification_type_code` | FK | Y | references `notification_types.code` | |
| `channel_code` | FK | Y | references `notification_channels.code` | |
| `is_enabled` | boolean | Y | `true`, `false` | Whether to deliver this notification type via this channel. |

**Uniqueness**: composite (`user_id`, `notification_type_code`, `channel_code`).

**Default behaviour (no row for a (user, type, channel) combo)**: deliver via every channel the user has enabled and verified in [user_notification_channels.csv](user_notification_channels.csv). Rows in this table are **explicit preferences that override the default** — usually `is_enabled=false` rows that opt out of specific (type, channel) combinations.

---

## subscriptions.csv

Polymorphic join table linking a user to any object they want to follow. Provides the `FOLLOWER_PLAYER`, `FOLLOWER_TEAM`, and `FOLLOWER_EVENT` relation tuples — used by the notification engine to decide who receives notifications about a given object. Lets a fan / scout / friend follow players, teams, or events without needing a guardian or coach relationship.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `user_id` | FK | Y | references `users.id` | The user doing the following. |
| `object_type_code` | FK | Y | references `object_types.code` | One of `PLAYER`, `TEAM`, `EVENT`. Other object types are not currently followable. |
| `object_id` | text | Y | references the corresponding entity's `id` (e.g. `ply_001`, `team_002`, `evt_004`) | Polymorphic FK — the dev team enforces referential integrity per `object_type_code`. |
| `subscribed_at` | date | Y | `YYYY-MM-DD` | When the subscription was created. |

**Uniqueness**: composite (`user_id`, `object_type_code`, `object_id`).

**Note**: `GUARDIAN` and `SELF` already imply notifications for a player (no need to subscribe separately). The `RECEIVE_PLAYER_NOTIFICATIONS` action grants to all three relations. Same idea for teams (head coach implies team notifications) and events (owner implies event notifications) — see [matrix.md](../../access/matrix.md).

---

## event_co_organizers.csv

Links additional organizers (besides the primary `events.organizer_user_id`) to an event. Provides the `CO_ORGANIZER` relation tuples for the auth model. A co-organizer can edit the event but cannot delete it — only the primary OWNER can.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `event_id` | FK | Y | references `events.id` | |
| `user_id` | FK | Y | references `users.id` where `role_code = ORGANIZER` | |
| `added_at` | date | Y | `YYYY-MM-DD` | When the co-organizer was added. |

**Uniqueness**: composite (`event_id`, `user_id`).

---

## event_venues.csv

Links events to venues. An event can use multiple venues (e.g. a tournament across 3 gyms).

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `event_id` | FK | Y | references `events.id` | |
| `venue_id` | FK | Y | references `venues.id` | |
| `is_primary` | boolean | Y | `true`, `false` | Exactly one row per event must be `true` (the headline venue shown in listings). |

**Uniqueness**: composite (`event_id`, `venue_id`).

---

## event_teams.csv

Which teams are registered to which events, and in which division. Used for Tournament, League, and Showcase. Camps use [event_players.csv](event_players.csv) instead.

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `event_id` | FK | Y | references `events.id` | |
| `team_id` | FK | Y | references `teams.id` | |
| `division_id` | FK | Y | references `divisions.id` | |
| `registered_at` | date | Y | `YYYY-MM-DD` | When the team registered. |

**Uniqueness**: composite (`event_id`, `team_id`, `division_id`) — a team can register in multiple divisions of the same event (e.g. a U18 team may also enter U18 Premier).

---

## event_players.csv

Individual player registrations. Used for Camps (individual reg, no team) and Showcases (players may register individually for scouting visibility).

| Column | Type | Required | Allowed values / format | Notes |
|---|---|---|---|---|
| `event_id` | FK | Y | references `events.id` | |
| `player_id` | FK | Y | references `players.id` | |
| `registered_at` | date | Y | `YYYY-MM-DD` | |

**Uniqueness**: composite (`event_id`, `player_id`).

---

## FK summary

```
                        provinces ◄──── orgs ◄──────┐
                             ▲                      │
                             └──── venues           ▼
                                     ▲           teams ────► age_groups
                                     │             ▲    ────► genders
                                     │             │
                                     │     team_coaches ────► users (coaches)
                                     │             ▲
                                     │             │
                                     │     player_teams
                                     │             ▲
                                     │             │
                                     │          players ────► positions
                                     │             ▲    ────► users (player accounts)
                                     │             ▲
                                     │      guardians ──────► users (spectators)
                                     │             ▲
                                     │             │
                                 event_venues  event_players
                                     ▲             ▲
                                     │             │
                                     └────────── events ────► users (organizers)
                                                   ▲    ────► provinces
                                                   │    ────► orgs (optional)
                                                   │    ────► event_formats
                                                   │    ────► event_types
                                              event_teams ──► divisions ────► age_groups
                                                                       ────► genders
```

---

## Decisions log

The schema reflects these PO decisions. Recorded here so the dev team understands *why* the model is shaped this way.

| # | Question | Decision | Rationale |
|---|---|---|---|
| 1 | Multi-role users | NO — one role per user | Keeps permissions simple. People with mixed roles (coach + player) need two accounts. |
| 2 | Multiple coaches per team | YES — `team_coaches` join | Real teams have head + assistant + parent manager; all need roster access. |
| 3 | Multi-team players | YES — `player_teams` join with effective dates | Standard in Thai basketball: kids play school + club concurrently; U16 may be called up to U18. |
| 4 | Parent–child guardian links | YES — `guardians` join | Without this, Spectator notifications are useless for the largest spectator segment (parents of minors). |
| 5 | Controlled province list | YES — `provinces` reference table by code | Free text breaks browse/filter; controlled list ensures referential cleanliness. |
| 6 | Multi-venue events | YES — `event_venues` join with `is_primary` flag | Real tournaments use multiple gyms; needed from day one. |
| 7 | Divisions relational | YES — `divisions` reference table, FK from `event_teams` | Free text caused inconsistency across organizers; relational allows reuse and custom skill tiers. |
| 8 | Promote enums to reference tables (age_group, position, gender) | YES — `age_groups`, `positions`, `genders` reference tables | Same referential-integrity argument as provinces. Free-text enums drift (`U16` vs `U-16` vs `Under16`) and break joins. |
| 9 | Promote `users.role` and `events.type` to reference tables | YES — `roles`, `event_types` reference tables | Enables Zanzibar-style data-driven authorisation: roles and event types load directly into SpiceDB / OpenFGA / Permify policies, and the access matrix can be expressed as relational data instead of code. |
| 10 | Promote remaining loose enums to reference tables (`org_types`, `coach_roles`, `relationships`, `locales`) | YES | All controlled vocabularies need Thai display names for the multi-language UI. Same referential-integrity argument as the others — every enum becomes a reference table. |
| 11 | Express the access matrix as data | YES — first cut: `features.csv` + `permissions.csv` (role × event_type × feature → W/R) | The full authorisation policy becomes a CSV the PO edits. Loadable into a Zanzibar-style auth store at deploy time. [matrix.md](../../access/matrix.md) becomes a human-readable view of this data, not the source of truth. |
| 12 | Move to full Zanzibar/relation-based auth (object-scoped permissions) | YES — replaced `features.csv` with `actions.csv` + added `object_types.csv` + `relations.csv` + rewrote `permissions.csv` (action × relation × event_type) + added `event_co_organizers.csv` | PO statement "an organizer cannot delete another's event; a coach cannot edit another team's roster" revealed that role-based permissions can't express object scope. The full Zanzibar model uses **relations** (a user's link to a specific object) instead of just **roles**. Relation tuples are derived from existing entity/join tables — no separate tuple store needed. |
| 13 | Model signup actions explicitly + commit to a Zanzibar-style auth engine for enforcement | YES — added 11 signup/admin/invite actions, `user_statuses.csv` (for the REFEREE pending-approval flow), recorded as [decision-002](../../decisions/decision-002-authorisation-engine.md) | Q&A revealed the model could express *who can edit an event* but not *who can sign up*. Without enforcement, the CSVs are aspirational. Closing the signup gap and committing to a Zanzibar engine (SpiceDB / OpenFGA / Permify) makes the model operational. |
| 14 | Add follower model so any user can subscribe to players, teams, or events | YES — added `subscriptions.csv` join table, `FOLLOWER_PLAYER`/`FOLLOWER_TEAM`/`FOLLOWER_EVENT` relations, 9 follow/unfollow/notification actions | Q&A "can a Spectator follow several players?" revealed the only existing notification path was via `GUARDIAN` — fans, scouts, friends, and team supporters had no way to follow. The polymorphic `subscriptions` table provides the FOLLOWER tuples; the per-object `RECEIVE_X_NOTIFICATIONS` actions are what the notification engine checks before sending. Notification-implying relations (GUARDIAN, SELF, HEAD_COACH, OWNER, etc.) are also granted, so e.g. a parent doesn't need to separately follow their own kid. |
| 15 | Model notification delivery channels (LINE-first for Thailand) | YES — added `notification_channels.csv` (LINE / EMAIL / SMS / PUSH / IN_APP) + `user_notification_channels.csv` (per-user prefs with `is_enabled` and `verified_at`) + `MANAGE_OWN_NOTIFICATION_CHANNELS` action | The auth side answered *who* receives notifications; this answers *how*. Critical for Thailand where LINE has 90%+ penetration — without modeling channels, the dev team would hardcode LINE and refactor later. `users.email/phone/line_id` stay as primary login/contact; this table holds delivery preferences (allowed to differ). |
| 16 | Per-notification-type preferences + multiple addresses per channel | YES — added `notification_types.csv` (14 types) + `user_notification_preferences.csv` (per-type, per-channel opt-in/out) + `address_label` column on `user_notification_channels.csv` so users can have multiple addresses per channel (e.g. personal LINE + family group LINE) + `MANAGE_OWN_NOTIFICATION_PREFERENCES` action | Real users want fine-grained control: "score updates via LINE, weekly digests via email." The `family_group` LINE pattern is common in Thai households where parents share a chat for kids' activities. Sparse-by-default: rows are explicit overrides; absence means default-deliver-via-all-enabled-channels. |
| 17 | Position broader than schools — events get optional org link, format flag, FIBA-cert flag | YES — `events.org_id` (nullable FK to `orgs`) + `events.format_code` (FK to new `event_formats.csv` — `5x5`/`3x3`) + `events.is_fiba_certified` boolean + `GRASSROOTS` row added to `org_types`. Rejected `users.org_id` (user-affiliation ≠ event-organising-body) and rejected first-class org-as-tenant (no `ORG_ADMIN`/`ORG_MEMBER` relations, no public org profiles). | Remy 2026-05-08 flagged the platform is for all Thai basketball events — schools, grassroots, and basketball associations — not just schools. [Q-and-A §23](../../decisions/Q-and-A.md) resolves: keep user-as-organiser for auth, but add a controlled-vocab org link on events for browse/filter/display. Format is metadata-only for Phase 1 (engine stays format-agnostic). FIBA cert is self-declared (zero ops cost; revisit if misused). |

---

## Future considerations

Not yet decided — these will surface as the dev team builds and as more of the pilot is understood.

- **`action_categories.csv`** — `actions.category` is the last loose enum. Could be promoted for Thai category names in the matrix view. Low priority.
- **Organizer signup gating** — currently `SIGN_UP_AS_ORGANIZER` is `PUBLIC` for the Thailand pilot bootstrap (low friction, anyone running a school tournament can sign up). Long-term, may need to flip to approval-required to control content quality. Revisit after pilot.
- **Invite tokens** — `INVITE_CO_ORGANIZER` and `ACCEPT_CO_ORGANIZER_INVITE` are modelled as actions but the underlying token generation/expiry/redemption mechanics are workflow concerns the dev team handles separately.
- **`notification_type_categories.csv`** — `notification_types.category` is currently free text. Could be promoted for full consistency.
- **Match-level relations** — `ENTER_SCORES` and `CONFIRM_MATCH_STATUS` currently use the broad `ANY_REFEREE` relation. When the match entity exists (Epic 005), this should become `REFEREE_OF_MATCH` (only the assigned ref for that specific match) — true Zanzibar-style scoping.
- **Guardian verification flow** — how a Spectator proves they're actually the parent of a Player (Q4's open sub-question)
- **Matrix view regeneration** — [matrix.md](../../access/matrix.md) is now a derived view of the auth data. A small script should regenerate it from `actions.csv`, `relations.csv`, and `permissions.csv` so they stay in sync (currently kept aligned manually).
- **Conditional / ABAC rules** — full Zanzibar/OpenFGA also supports conditional rules (e.g. "only during registration window"). Out of scope for now; add when needed.
- **Match-level data** — `matches.csv`, `match_scores.csv`, `match_officials.csv` will be needed when Epic 005 (Scores & Results) starts
- **Sponsors / partners** — likely a future entity once the league sponsor model takes shape
