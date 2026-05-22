# The prestige mechanic, faithfully traced

The single most important design innovation in long-running
incrementals is the **prestige** mechanic: the player voluntarily
resets their progress in exchange for a permanent multiplier on
future runs. This is the engine that converts a finite exponential
climb (which inevitably hits diminishing returns) into an
indefinite series of accelerating climbs.

The popular telling of where prestige came from is "Call of Duty 4
in 2007." That is where the word entered mass-gaming vocabulary,
and where the modern *shape* of the mechanic crystallised. But the
mechanic itself is older, and the word came from somewhere else
again, with a different meaning. The faithful lineage is worth
laying out because most accounts conflate stages of it.

## The word: D&D 3rd Edition (2000)

The term "prestige" entered tabletop-RPG vocabulary with **Dungeons
& Dragons 3rd Edition** (Wizards of the Coast, 2000), in the form
of **prestige classes**: specialised character classes that
characters could only enter after meeting prerequisites in their
base class. The Assassin, the Arcane Archer, the Loremaster.

Important: **D&D prestige classes are not a reset-for-bonus
mechanic.** They are a *progression* mechanic -- an advanced
subclass you unlock by meeting requirements. The word "prestige"
carried connotations of "earned status," not "voluntary reset."

But the word now existed in gaming vocabulary, attached to "earned
late-game advancement," and seven years later Call of Duty would
borrow the word while inventing a different mechanic underneath it.

## The mechanic: text MUDs and BatMUD (1990)

The actual *reset-for-bonus* mechanic appears to predate the word
by a decade. The earliest documented implementation in a game
context is **BatMUD** (1990), a text-based multi-user dungeon, which
let experienced players reset their character to start over while
retaining selected enhanced abilities -- the mechanical core of
modern prestige.

This is reported in coverage of the prestige mechanic's history
(notably the *Inverse* piece "Prestige 'Cold War': How Call of Duty
fixed a broken 'D&D' mechanic"). Caveat: BatMUD's contemporary
documentation is thin, and "earliest" claims about MUD mechanics
are notoriously hard to verify since many MUDs evolved over years.
Treat this as "the earliest well-attested implementation" rather
than as a clean origin point.

## The shape: NetStorm: Island At War (1997)

A decade before CoD4, Activision published **NetStorm: Island At
War** (developer: Titanic Entertainment, 1997), a real-time
strategy game that included an early prestige-style **metalevel**
system: players unlocked turrets and structures, then could
voluntarily relock their units to gain a metalevel displayed on
their account.

This is structurally striking -- it is essentially the CoD4
mechanic, ten years early, in a different genre. NetStorm did not
popularise the pattern; the game was a commercial disappointment.
But it establishes that the *idea* was in circulation in
mainstream commercial games long before CoD.

## The popular form: Call of Duty 4 (2007)

**Call of Duty 4: Modern Warfare** (Infinity Ward, November 2007)
is where the word "prestige" and the reset-for-bonus mechanic
combined in front of a mass audience. At rank 55 (the level cap),
players could choose to "Prestige": reset their rank and unlocks to
zero, in exchange for a small visual badge next to their name
showing they had done it.

Crucially: in the CoD4 version, the prestige reward was almost
purely *cosmetic*. The badge said you had done the work. There was
no permanent multiplier; subsequent prestiges just got more elaborate
badges. The mechanical reward was status.

This is the version that entered popular vocabulary. *To prestige*
became a verb. Every CoD title since (with one exception, the 2019
*Modern Warfare* reboot) has included some variant.

## The transplant: Cookie Clicker Ascension (2014)

When prestige migrated into incremental games, it was substantially
transformed. The first major incremental to adopt it was
**Cookie Clicker** -- not in its original August 2013 release, but
in **version 1.035 in 2014**, which introduced **Ascension** and
**Heavenly Chips**.

This is worth pinning down because it is often misremembered:
- Cookie Clicker (Aug 2013) did *not* originally have prestige.
  Players hit a wall and the wall stayed a wall.
- Ascension was added later as a way to make the late game
  navigable.
- The "Heavenly Chips" model -- reset everything, gain a permanent
  currency that grants permanent multipliers and unlocks a skill
  tree -- is **substantially different from CoD prestige.** The
  cosmetic badge became a mechanical permanent benefit.

So the genre-defining version of prestige in incrementals is *not*
the CoD4 form. It is a *renovation* of the CoD4 form -- borrowing
the word and the voluntary-reset framing, but reinstating the
permanent-mechanical-benefit of the earlier MUD-style versions.

It is more accurate to say: incremental-game prestige *recombines*
the BatMUD reset-for-retained-bonus mechanic with the CoD-popularised
word and ritualised choice point. The lineage is not a line; it is
a convergence.

## Why this distinction matters

Two reasons.

First, **the cosmetic-vs-mechanical reward difference is the entire
game.** CoD prestige is a status symbol you wear; it doesn't change
how you play the next run. Incremental prestige fundamentally
restructures the next run -- you are dramatically faster, and the
loops you were grinding become trivial. The same word names two
different mechanics with different psychological effects.

Second, **the nesting** -- prestige-on-prestige -- that defines
long-running incrementals like Antimatter Dimensions and Synergism
is a post-Cookie-Clicker innovation that has no clear CoD or D&D
ancestor. It is the genuine novel contribution of the incremental
genre to game design. You reset for currency X, then later reset
your X for currency Y, then later reset your Y for currency Z, with
each layer running on a slower time scale than the one inside it.

This nested structure does not come from anywhere upstream. It is
what happens when designers in this genre, looking for a way to
extend engagement past the wall their first prestige loop hits,
discover that the prestige trick is *itself* recursively applicable.
Once you notice that, you cannot stop noticing it -- and the late-
game of Antimatter Dimensions and its successors is what that
recursion looks like taken to its limit.

## The faithful summary

- **1990 -- BatMUD** (text MUD): earliest well-attested
  reset-for-retained-bonus mechanic.
- **1997 -- NetStorm: Island At War** (Activision/Titanic): same
  mechanical shape in a commercial RTS, with the "metalevel"
  framing.
- **2000 -- D&D 3rd Edition**: the *word* "prestige" enters gaming
  vocabulary, attached to a different mechanic (advanced subclass).
- **2007 -- Call of Duty 4: Modern Warfare**: the *word* meets the
  *mechanic* and reaches a mass audience. The reward in this version
  is cosmetic.
- **2014 -- Cookie Clicker Ascension (v1.035)**: the mechanic
  arrives in incrementals, recombined with permanent multipliers
  and a skill tree. The CoD ritual, the BatMUD payoff.
- **Post-2014 -- nested prestige** (Antimatter Dimensions, Synergism,
  etc.): the original contribution. Recursion of the prestige
  mechanic produces the multi-currency, multi-timescale structure
  that defines the modern long-running incremental.

Continue with [03-universal-paperclips.md](03-universal-paperclips.md).
