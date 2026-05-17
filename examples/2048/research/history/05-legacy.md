# Legacy (2014 -- present)

## As a viral artifact

2048 was, alongside *Flappy Bird* a month earlier, one of the canonical
"weekend project goes worldwide" stories of the mid-2010s. It is still
routinely cited as a reference point when a small thing on Hacker News
takes off unexpectedly. The site `play2048.co` has remained online
continuously since 2014.

## As an open-source seed

Because Cirulli MIT-licensed the original, the codebase became one of
the most-forked single-page games on GitHub. Variants that got their
own viral moments include:

- *Doge 2048*, *Flappy 2048*, *2048: Doctor Who*
- *Tetris-2048* and other genre-mash hybrids
- 3D and 4D variants on cubic / hypercubic grids
- Larger-grid versions (5x5, 6x6, 8x8)
- The original `gabrielecirulli/2048` repo, used as a teaching template
  in countless intro web-development courses

The example in *this* repo is itself part of that tradition -- a
reimplementation whose value is not 2048 the game but the
infrastructure story it sits on top of.

## As an AI / search problem

2048 turned out to be a useful target for game-playing AI research.
Reasons:

- State is trivially small (a 16-cell board, integers).
- Rules are pure and well-specified.
- The stochastic tile spawn makes it a *probabilistic* game tree, which
  is interesting in ways pure adversarial games are not.

The standard reference algorithm is **expectimax search** with a
heuristic scoring function (monotonicity, smoothness, empty-cell count,
maximum-tile-in-corner). Strong implementations approach 100% success
at reaching the 2048 tile and routinely reach 16384.

More recent work has pushed further: by 2025, the better systems --
combining expectiminimax search, transposition tables, and
transformer-based value networks -- were reported reaching the **65536
tile** in roughly 8% of games, with 16384 essentially always
achievable. There is a sizable academic literature on it (Yiyuan Lee,
Robert Xiao, and others have published well-known analyses).

This durability as an AI benchmark is part of why 2048's footprint
outlived the viral spike. *Threes!*, despite being the better-designed
game, has a smaller research literature largely because its rules are
slightly fussier to encode and its state space is a bit larger.

## Cirulli's return

Cirulli stepped away from active work on the game for the better part
of a decade. He worked at 1Password as a designer through that period.
In 2024 he **quit 1Password to work on 2048 full-time**, and launched a
substantially reworked version of the game with new mechanics, better
performance on mobile, and a stated plan to keep the original free and
the source open. He has done a small media tour around that relaunch
(*Hey, Good Game* podcast, *Softonic*, *Download3K*, etc.; see
[sources.md](sources.md)), generally with the same posture he has had
since 2014: gratitude, no claim to originality he doesn't have, and
genuine fondness for the clones his own work spawned. From a 2024
interview:

> [the copycats are] a beautiful extension of creativity rather than
> theft.

## What this story is usually used to illustrate

Three things, mostly:

1. **The viral cost of friction.** Free + web + open source + a single
   HN post outran a fourteen-month $1.99 iOS game with a tighter
   design.
2. **Cloning ethics in a derivative medium.** Every puzzle game stands
   on others. There is no clean line. Threemails is one of the best
   primary sources on what that argument actually feels like from
   inside.
3. **Mechanical fragility vs. designed depth.** "An automated script
   can beat 2048" is a striking, true, and durable critique. It is a
   reminder that "easy to pick up" and "well-designed" are not the
   same thing, even when the market rewards them as if they were.

Sources for all chapters are in [sources.md](sources.md).
