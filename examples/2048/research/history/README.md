# 2048: a short history

A research bundle attached to this example. The example itself is an
event-sourced reimplementation of 2048 -- this directory documents where
2048 came from, so the demo isn't sitting on a story you have to look up
elsewhere.

The arc, in one paragraph: in early 2014 a small team called Sirvo
released *Threes!*, a paid iOS puzzle game they'd spent roughly fourteen
months designing. Three weeks later a Chinese studio, Veewo, shipped a
free clone called *1024*. A few weeks after that a 19-year-old Italian
web developer, Gabriele Cirulli, built his own variant -- *2048* -- over
a weekend, posted it on Hacker News, and watched it go viral. *Threes!*
remained successful, but the cultural moment belonged to the clone of a
clone. The *Threes!* creators wrote a 45,000-word public retrospective
in response. The episode is now one of the canonical case studies in
indie-game cloning.

## Reading order

1. [01-threes.md](01-threes.md) -- the original. Sirvo, fourteen months,
   the design that everything else copied.
2. [02-1024.md](02-1024.md) -- Veewo Studio's *1024*. The first clone,
   shipped three weeks after *Threes!*, free.
3. [03-2048-launch.md](03-2048-launch.md) -- Cirulli's weekend project,
   the Hacker News post, the viral spike.
4. [04-clones-and-controversy.md](04-clones-and-controversy.md) -- the
   "Threemails" retrospective, the "broken game" critique, the App Store
   flood (15 new clones a day at the peak).
5. [05-legacy.md](05-legacy.md) -- variants, AI solvers, academic uses,
   Cirulli's recent return to working on 2048 full-time.
6. [sources.md](sources.md) -- annotated index of every URL cited
   across the chapters.

## Why this is in a code example's repo

The example deliberately uses 2048 because it is small, pure, and
familiar -- a good canvas for the event-sourcing story the demo is
actually about. Familiarity is doing real work here, so it's worth
knowing whose game we are using and what they thought of how it spread.
