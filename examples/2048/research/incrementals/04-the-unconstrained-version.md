# The unconstrained version: what the toolkit does with no exit

[03-universal-paperclips.md](03-universal-paperclips.md) describes
the version of the genre where the designer puts an ending in
deliberately. This chapter describes what happens when nobody does.

The contrast is sharper than the press treatment suggests.
"Long-running incremental" is not just "a clicker that goes on
longer." It is a distinct design tradition with its own innovations
-- nested prestige, asymptotic number systems, offline progress --
and its own internal logic. Some of it is genuinely interesting as
mathematics and as design. None of it ends.

## Cookie Clicker, as the institutional example

The canonical long-running incremental is **Cookie Clicker** itself,
in its post-2014 form -- after the addition of Ascension (see
[02-the-prestige-mechanic.md](02-the-prestige-mechanic.md)) and
the subsequent years of additions. It is still being updated by
Orteil as of the mid-2020s.

The shape of mature Cookie Clicker is layered loops at different
timescales:

- **The click loop** (seconds): manual clicks for small bonuses,
  golden cookies to catch.
- **The build loop** (minutes): saving for the next building tier.
- **The upgrade loop** (minutes to hours): saving for milestone
  upgrades.
- **The prestige loop** (hours to days): when to ascend.
- **The garden loop** (real-time hours): plant crossbreeding.
- **The stock market loop** (real-time hours): trading cookie
  commodities.
- **The seasonal loop** (real-time weeks): event-specific upgrades.

The key property: **these loops do not replace each other. They
accumulate.** Unlike Paperclips, which substitutes act two's
mechanics for act one's, Cookie Clicker keeps every loop you have
ever engaged with active forever. The click loop is still there in
the late game when you have an Antimatter Condenser producing 10^40
cookies per second. You can still click. The earlier loops do not
become *useful*; they remain *present*.

This is not a small choice. It is the design pattern that allows
the game to fill any gap in the player's day, regardless of how
much time they have. Five seconds? A click. Five minutes? A
strategic upgrade. Five hours? An ascension. Five weeks? A seasonal
event. The genre's accommodation of all available time is the
property that makes "I play Cookie Clicker on a background tab at
work for years" a normal sentence within its community.

## Antimatter Dimensions and the nested prestige stack

**Antimatter Dimensions** (Hevipelle, 2016 onward) is the formal
embodiment of nested prestige. The structure is described in detail
in [02-the-prestige-mechanic.md](02-the-prestige-mechanic.md), but
briefly:

- Layer 0: accumulate **antimatter** by buying Dimensions.
- Layer 1: reset for **Infinity Points** when you hit the JavaScript
  number ceiling (~1.8 x 10^308).
- Layer 2: reset Infinity Points for **Eternity Points**.
- Layer 3: reset Eternity Points for **Reality Machines**.
- Beyond: Glyphs, Celestials, additional currencies.

Each layer's outer loop runs on a slower timescale than the layer
inside it. By the late game, the player is managing currencies at
four or more nested scales simultaneously. The numbers get large
enough that the game uses **OmegaNum.js** -- a JavaScript library
for representing tetrational and higher-order numbers (10^^N for
arbitrarily large N) -- to display them.

This is not a Cookie Clicker variant. It is a genuinely novel
contribution. The recursive application of the prestige trick to
itself is one of the few late-2010s game-design innovations that
has no clear analogue in any other genre.

## Asymptotic number systems

The mathematical side of the long-running incremental is worth
naming, because it is its own small craft tradition.

When numbers exceed `Number.MAX_VALUE` (~1.8 x 10^308 in
double-precision floats), the game cannot use the JavaScript native
type. Several libraries have emerged to handle this:

- **break_eternity.js** (Patashu, 2018): handles up to 10^^(2^53),
  using tetration notation.
- **OmegaNum.js** (Naruyoko, 2019): handles up to **BEAF** (Bowers
  Exploding Array Function) values, with arrow notation.
- Beyond that, games use community-maintained higher-order notations
  (chained-arrow, hyper-E) that exist mostly within this scene.

Players in this corner of the genre develop genuine fluency in
mathematical notations that, outside the genre, only show up in
combinatorics research and large-cardinal set theory. A late-game
**Synergism** (Pseudonian, 2020) player will speak casually about
tetrations and Knuth up-arrows.

This is the thing the genre's defenders mean when they argue it is
not just a Skinner box: there is real intellectual substance in the
optimization problems, and real mathematical curiosity in the
notation. The criticism is not that this is fake -- it is real.
The criticism is whether it is *enough* to justify what the
mechanics do to attention.

## Offline progress, and the inversion of "playing"

One of the design choices that most clearly marks the long-running
incremental as a distinct tradition: **offline progress**. The game
continues to produce currency while you are not playing. When you
return, you see a summary: "you earned X cookies while away."

This sounds humane. It is not, structurally. What it actually does:

- It makes the game *active* in your awareness even when closed,
  because the longer you wait, the larger the bonus you can collect.
- It removes the cost of *not* playing: you are still progressing,
  just slower.
- It establishes that *being absent* is itself a form of play.
- It primes you to come back, because there is always something
  waiting.

The argument from designers who include offline progress is that it
respects the player's time -- you don't have to grind. The argument
against is that it converts the player's entire day into game time,
because every minute away is a minute of slowly-accruing reward.

Frank Lantz did not include offline progress in Universal
Paperclips for act one. He did, partly, for act three (probe
production while away), but the resource produced is small relative
to active play, and the act ends. The pattern in the unconstrained
version of the genre is the opposite: aggressive offline accrual,
generous catch-up multipliers, push notifications when bonuses are
ready.

## Live-ops and seasonal accumulation

Long-running incrementals optimised for retention add another layer:
**seasonal events** (Halloween, Christmas, Easter, Valentine's, etc.)
that introduce time-limited upgrades. These upgrades, once obtained,
persist permanently. The player who misses an event misses a
permanent collectible.

This is the same FOMO architecture used by free-to-play mobile
games and battle passes. It is one of the most studied retention
techniques in the field. Importantly, **it is the technique most
clearly imported from monetised mobile design into the otherwise
mostly-free incremental scene.** Most long-running incrementals do
not charge money. But they have absorbed the engagement structures
of the games that do.

This is the deepest argument the design-ethics critics make:
the incremental genre learned its retention techniques from the
business model it claims not to be part of, and the techniques
work regardless of whether money is involved. Frank Lantz's
position, restated from the psychology bundle, is roughly this --
that the same machinery can be used for depth or for compulsion,
and the difference is what the designer chooses to do with it. The
unconstrained version of the genre is what choosing the second
option looks like.

## A diagnostic, not a verdict

A useful test that recurs in the design-ethics conversation: ask
of any incremental whether the designer intended the player to
reach a particular ending.

- Universal Paperclips: yes. Four to six hours, ending on purpose.
- Candy Box!: yes. Roughly the same length, ending on purpose.
- A Dark Room: yes. A few hours, ending on purpose.
- Cookie Clicker: no. There is no "completed" state. Updates keep
  adding new layers.
- Antimatter Dimensions: no. The late game extends asymptotically.
- Synergism: no.
- Realm Grinder: no.
- Most mobile idle games on the App Store: no.

That distinction maps cleanly onto something like "the game is an
art object" vs "the game is an ongoing service that benefits from
your presence." Both are legitimate game-design outputs. They are
not the same kind of thing, and the genre's habit of using one
vocabulary for both is part of why the design-ethics argument
inside the field is unresolved.

Back to [README.md](README.md), or check
[sources.md](sources.md).
