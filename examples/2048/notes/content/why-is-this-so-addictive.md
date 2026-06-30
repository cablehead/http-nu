# Why is this so addictive!?

I ported 2048 to Nushell over a weekend, for fun. Then I noticed I
kept playing it. Not testing it. Playing it. I'd close the tab to get
back to work and find it open again before I'd decided to. That's when
I stopped and asked: what is going on here? Has anyone actually written
about the pull? Turns out a _lot_ of people have.

The most-quoted answer is from [Judy Willis](https://radteach.com/), a
UC Santa Barbara neurologist the press talked to in 2014. She named
[two dopamine boosts](https://web.archive.org/web/20150311024653/https://www.popularmechanics.com/culture/gaming/a10341/why-the-2048-game-is-so-addictive-16659899/).
The first is prediction: every move is a guess at what the board does
next, and brains (in her phrase, "like bookies and psychics") love
making predictions, so the dopamine arrives just for guessing. The
second is that the game stays challenging but achievable, which keeps
that reward flowing instead of tipping into frustration.

But the part that actually got me is something her account skips: the
[fake sense of progress](https://en.wikipedia.org/wiki/Incremental_game). Every new high tile lands
like a level-up. Your first 256, then 512, then 1024, each one a new
personal best, new territory, a little "I'm finally getting somewhere".
But nothing has actually changed: same 4x4 grid, same four swipes, same
game you were playing back at tile 16, just a bigger number. And
because every step is a doubling, the jump to 512 feels every bit as
exciting as the jump to 256 did. The climb doesn't lose steam. The
progress is just a number going up, one of the stickiest things a game
can dangle.
(An entire genre, the [idle and clicker
games](https://www.vice.com/en/article/cookie-clicker-wasnt-meant-to-be-fun-why-is-it-so-popular-8-years-later/),
is built on nothing else.
That's its own rabbit hole, and its own page, later.)

The rest pile on top:

- **Variable-ratio reward.** Each move's payoff turns on a random spawn
  (nine times in ten a 2, otherwise a 4) in a square you didn't pick.
  Rewards on an unpredictable schedule are the [hardest to walk away
  from](https://en.wikipedia.org/wiki/Reinforcement); it's what slot
  machines run on.
- **Free restart.** No login, no install, no score to keep, no "are you
  sure?". "One more" costs nothing, so you take it.
- **Near-miss.** Only [about 347,000 of the first 42 million
  games](https://www.buzzfeednews.com/article/hillaryreinsberg/why-this-free-puzzle-game-is-the-most-addictive-thing-on-the)
  reached 2048, under one percent, so nearly every game ends just
  short. And just short
  is the addictive part: gambling studies find a [near miss makes people
  keep playing](https://pmc.ncbi.nlm.nih.gov/articles/PMC2658737/) more
  than a clean loss does, even though it's still a loss.
- **The unfinished board.** The [Zeigarnik effect](https://en.wikipedia.org/wiki/Zeigarnik_effect):
  we remember interrupted tasks about twice as well as finished ones.
  2048 almost never gives you a finished one. Lose, and the game ends
  mid-climb, before you got where you were headed. And even when you
  win, you're not finished: 2048 isn't the end, you keep going for 4096,
  8192, up to a [theoretical maximum of
  131072](http://www.science4all.org/article/2048-game/). So it ends
  unfinished either way.

If you'd rather play the version its makers actually hoped you'd be
able to put down, [Threes! and the others are over in the
backstory](./backstory).

Next: [how this is built](./in-nushell).
