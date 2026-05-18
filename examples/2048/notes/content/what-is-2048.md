# The Rules

2048 is a sliding-tile puzzle on a 4x4 grid. You combine matching
numbers, build doubles, and try to reach the 2048 tile.

**Moves.** Each turn you slide every tile in one direction -- up, down,
left, or right. Tiles travel as far as they can. Equal neighbors that
collide along the way merge into a single tile worth twice as much
(two 4s become an 8). A tile can only participate in one merge per
move.

**Scoring.** Each merge adds the value of the new tile to your score.
Combining two 64s adds 128 points; combining two 1024s adds the win.

**Spawning.** After every move that actually changed the board, a new
tile appears in a random empty square. It's a 2 nine times out of ten,
a 4 the rest of the time.

**Winning.** You "win" the first time a 2048 tile appears. You don't
have to stop. Most players push for 4096, 8192, or the theoretical
maximum of 131072.

**Losing.** When the board is full and no two neighbors share a value,
no move can change anything. Game over.

**The tactic.** Pick a corner. Keep your biggest tile there.
Everything else organizes around that anchor. The game gets harder
the longer you play -- doubling is exponential, the board is not.

Next: [how 2048 happened](./backstory) -- the clone wars and the
weekend project that ate March 2014.

# Backstory

**Threes! had a 14-month design process.** Asher Vollmer and Greg
Wohlwend exchanged 570 emails between December 2012 and early 2014,
iterating through prototypes that involved monsters, holes, arrows,
and merge mechanics before the doubling system emerged around June
2013. They wrapped it in the 1+2=3 quirk, character tiles, and sound
design. Their stated goal: *"We wanted players to be able to play
Threes over many months, if not years."* Their critique of 2048 is
that a scripted alternation of up and right beats it -- the strategic
depth they spent fourteen months protecting is exactly what the clone
strips away. The full exchange is in their post,
*[Threes! -- the rumination on game design](http://asherv.com/threes/threemails/)*.

[Gabriele Cirulli](https://gabrielecirulli.com/) wrote
[2048](https://gabrielecirulli.github.io/2048/) in a weekend in March
2014. Inside a week, four million people had played it. Inside a
month, copies of it were among the most downloaded games on the App
Store -- and Cirulli's own version was open source, free, and ad-free
on the web.

**The lineage.** Cirulli built 2048 as a clone of *1024*, a free
ad-supported iOS app by Veewo Studio. 1024 was itself a riff on
Threes!, released six weeks earlier. Cirulli has been explicit about
this: he described 2048 as "a clone of 1024, which is a clone of
Threes!" and pointed people to the original.

**The clone wars.** Within weeks of 2048 going viral, the App Store
filled with copies of copies. The Threes! post is partly a defense of
the design choices that 1024 and 2048 had stripped away, partly a
meditation on what it feels like to spend a year crafting a game and
watch a weekend project eclipse it. *"Threes was cloned and beat to a
different market within 6 days of release on iOS. 2048 isn't that
clone. But it's sort of the Commander Keen to Super Mario Bros.
situation."*

**Cirulli's framing.** He [later wrote](https://medium.com/@gabrielecirulli/2048-success-and-me-7dc664f7a9bd)
that he hadn't expected anyone to play 2048 -- it was a thing he made
over a weekend to learn JavaScript. The
[open-source repo](https://github.com/gabrielecirulli/2048) on GitHub
became the canonical version; people forked and remixed it for years.

**Why it caught.** Doubling. The math is simple enough to hold in your
head and rich enough to surprise you. Each merge is a tiny dopamine
hit. The board is small enough that every move feels consequential.
And it's free -- no install, no account, no ads.

Next: [the ones worth playing](./best-of-2048) -- the ancestor, the
canonical version, and a variant worth your time.

# Best of 2048

**Threes!** -- the careful 14-month original. Pay for it. It's the
version made by people who hoped you'd put it down sometimes.

- [iOS](https://apps.apple.com/us/app/threes/id779157948)
- [Threes!+ on Apple Arcade](https://apps.apple.com/us/app/threes/id1551561086)
- [Threes! Freeplay (ad-supported)](https://apps.apple.com/us/app/threes-freeplay/id976851174)
- [Android](https://play.google.com/store/apps/details?id=vo.threes.exclaim)
- [Steam](https://store.steampowered.com/app/1818570/Threes/)
- [official site](https://asherv.com/threes/)

**2048 by Gabriele Cirulli** -- the canonical version. Open source,
free, in any browser tab.

- [play](https://gabrielecirulli.github.io/2048/) ·
  [source](https://github.com/gabrielecirulli/2048)

**[2048 Numberwang](https://louh.github.io/2048-numberwang/)** -- a
parody variant where merges resolve by absurdist British game-show
logic instead of arithmetic. Worth a few rounds for the joke alone.

Next: [how this is built](./in-nushell) -- the same game on top of
event-sourcing, SSE, and view transitions.

# In Nushell

This implementation is a long-running experiment in seeing how much
sophistication comes out of a few hundred lines of shell script.

**The stack.**

- **http-nu** -- a Nushell-scriptable HTTP server. Routes are
  closures; the request and response live in the pipeline.
- **cross.stream (xs)** -- an append-only event store. Every move,
  every snapshot, every player session is a frame.
- **Datastar** -- server-sent events drive the UI. The server pushes
  HTML fragments; the client morphs them in place via morphdom.
  View-transitions animate tile slides at the browser level.

**The actor.** A snapshot-actor watches the stream. When a
`game.<id>.move` frame lands, it resumes the game state from prior
frames, applies the move, and appends a `game.<id>.snapshot` with the
new state in the frame's meta. The actor is the single writer for
snapshots; every reader pulls them by topic.

**The pipeline.** The /play page subscribes to `game.<id>.*` via SSE.
The server tails the topic, threshold-gates pulses, renders state to
HTML, wraps each render in a Datastar patch event, and writes the SSE
stream. Nu pipelines all the way down -- `.cat --follow | pulse-keepalive
| frames-to-states | threshold-gate-states | states-to-html | html-to-patches | to sse`.

**The insight.** Spectating a live game uses the same pipeline as
playing one. There's no separate "watch mode" -- /sse/&lt;id&gt; just
tails the same stream the player's session is tailing. Tee the
pipeline, get a spectator for free.

**Why this is interesting.** Event-sourcing usually arrives wrapped
in Kafka and ceremony. Here the entire system is a few hundred lines
of Nushell, a single binary for the server, and a streaming append-log
for the database. The whole game is a stream of frames; the UI is a
stream of patches; the player is a stream of intents.

The source: [examples/2048 on
GitHub](https://github.com/cablehead/http-nu/tree/main/examples/2048).
Browse the [/design](../design/) component viewer for the live mirror
of every server-rendered piece.
