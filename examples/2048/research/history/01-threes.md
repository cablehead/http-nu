# Threes! -- the original (February 2014)

*Threes!* is the game everything in this lineage descends from. It was
made by **Sirvo LLC**, a three-person team:

- **Asher Vollmer** -- designer / programmer
- **Greg Wohlwend** -- illustrator (also known for *Ridiculous Fishing*,
  *TouchTone*, *Tumbleseed*)
- **Jimmy Hinson** -- composer

Released on **iOS, 6 February 2014**, $1.99. Later ported to Android
(March 2014), Xbox One (December 2014), Windows Phone (April 2015), and
eventually Steam (February 2024, the tenth-anniversary release).

## How long it took

The first playable prototype was written in a single night. The finished
game took roughly **fourteen months** -- somewhere over 570 emails
exchanged between Vollmer and Wohlwend across December 2012 to
December 2013. Most of that time was spent throwing out theming.

Wohlwend prototyped sushi tiles (pairing fish with rice), chess pieces,
animals, broccoli-and-cheese soup, military insignia, hydrogen atoms,
textile patterns. Playtesters were confused by all of it. Vollmer:

> these ideas made the game feel "unwieldy and unnatural"

A turning point came from fellow designer Zach Gage, who pushed them
toward stripping it back. Wohlwend, later:

> the game "always wanted to be simple"

What shipped was the now-recognisable form: numbered tiles, soft pastel
palette, the little anthropomorphised faces on the early tiles, a
distinctive sound design.

## The mechanics, and why they differ from 2048

This is the part the *Threes!* team cared most about and the part most
players never noticed.

- **Tiles merge only when adjacent and the merge is legal.** A 1 and a 2
  merge into a 3. After that, only equal pairs merge: two 3s make a 6,
  two 6s make a 12, and so on. You cannot stack-cascade arbitrary equal
  tiles together in one move.
- **A move slides all tiles exactly one cell in the chosen direction**,
  not all the way across the board. Position discipline matters.
- **The next incoming tile is shown to the player** before the move. The
  randomness is partly disclosed; planning is possible.

This makes *Threes!* a tight, constrained puzzle. The team's tagline was
"simple to learn, impossible to master." They claimed (in the
retrospective discussed in [04-clones-and-controversy.md](04-clones-and-controversy.md))
that as of the post being published, only six people in the world had
reached the 6144 tile, and nobody had ever exhausted the game.

The contrast with 2048 -- where tiles slide all the way across and any
equal pair merges -- is the substance of what Vollmer and Wohlwend later
went public about. They considered the looser merge rules a corruption,
not a simplification.

## Reception, on its own terms

*Threes!* was critically well received -- Top 10 in App Store Puzzle and
Card categories, Top 20 paid overall in the US -- and it stayed there
even after the clones arrived. Commercially it was fine. The
disappointment was cultural: the version of their game that took over
the world was the one they hadn't made.

Continue with [02-1024.md](02-1024.md) -- Veewo Studio's free clone, the
intermediate step between *Threes!* and *2048*.
