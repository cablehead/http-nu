# The Rules

2048 is a sliding-tile puzzle on a 4x4 grid. Combine matching numbers,
build doubles, reach the 2048 tile.

You start with two tiles. Each move slides every tile to one side; equal
neighbors merge into a single tile worth twice as much. A new 2 (or
occasionally 4) appears in an empty square. Repeat until you reach 2048
-- or until the board fills up and nothing can move.

It is deceptively hard. The grid is small. Doubling is fast. A single
careless slide can lock you into a dead end three moves later.

# Backstory

Gabriele Cirulli wrote 2048 in a weekend in March 2014. Inside a week,
four million people had played it. Inside a month, copies of it were
among the most downloaded games on the App Store -- and Cirulli's own
version was open source, free, and ad-free on the web.

The lineage is its own story. Cirulli built it as a clone of *1024* by
Veewo Studio, which itself riffed on *Threes!* by Asher Vollmer and
Greg Wohlwend. Threes! had been in development for over a year; 2048
arrived two months after its release and arguably outpaced it.

*more soon: the clone wars, why doubling is addictive, the ethics of
the inspiration chain ...*

# In Nushell

This implementation is a long-running experiment in seeing how much
sophistication can come out of a few hundred lines of shell script.

The whole stack is:

- **http-nu** -- a Nushell-scriptable HTTP server. Routes are closures;
  the request and response live in the pipeline.
- **cross.stream (xs)** -- an event store. Every move is an immutable
  frame; a snapshot-actor watches the stream and materializes the
  current game state.
- **Datastar** -- server-sent events drive the UI. The server pushes
  HTML fragments; the client morphs them in place. View-transitions
  animate tile slides.

*more soon: the actor loop, the SSE pipeline, how viewing a live game
is just tee-ing the same stream the player is on ...*
