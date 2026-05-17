# Where the evidence thins

The previous three chapters lay out the standard explanations for why
2048 is hard to put down. They are plausible, internally consistent,
and probably substantially correct. They are also softer than the
confident pop-science prose around them suggests, and it is worth
being explicit about where.

## "Addictive" vs. *addiction*

Almost every 2014 article called 2048 "addictive." Very few of them
meant clinical addiction. There is no DSM-5 entry for puzzle-game
addiction; the closest thing -- *Internet Gaming Disorder* -- is in
DSM-5's appendix as a condition for further study, and it is targeted
at MMORPG-style multi-hour engagement, not single-tab browser puzzles.

When the press talks about 2048 addiction, what they almost always
mean is one of:

- Compulsive *engagement* -- short, repeated sessions across a day.
- Loss of *time tracking* -- flow-induced time distortion.
- Difficulty *quitting* a single sitting once started -- the
  variable-ratio reinforcement story.

These are real, well-documented effects. They are not the same as the
chemical-dependency or behavioral-disorder framing that "addictive"
implies in lay use. Be careful with the word.

## The dopamine claims

The dopamine-on-prediction story (Judy Willis in
[01-the-dopamine-account.md](01-the-dopamine-account.md)) is a
plausible simplification of a more complex literature on
**reward-prediction error** (Schultz, Dayan, Montague -- the canonical
work in the late 1990s on dopamine neurons in primates). The accurate
version is that midbrain dopamine neurons encode the *difference*
between expected and received reward, not reward itself.

The "your brain releases dopamine just from predicting" framing
flattens this. The "80% failure rate still produces dopamine" claim is
even thinner; we could not locate a primary source for that exact
figure, and it appears to have entered the press cycle through
secondary citations of Willis. It might be a real result; it might be
a simplification she made for a teacher audience that then got quoted
as if it were a hard finding. Treat it as evocative, not as a
citeable number.

## The Skinner / variable-ratio framing

This part is on much firmer ground. The schedule-of-reinforcement
literature is sixty years old, robust, well-replicated, and the
**variable-ratio schedule** is one of the most reliable findings in
behaviorism. Applying it to 2048 is not a stretch: the random spawn
behavior fits the variable-ratio template very directly.

The thing this framing *doesn't* tell you is whether 2048-the-game
will affect any particular *person*. Schedules of reinforcement
describe how a behavior is shaped under conditions of reinforcement,
not how susceptible a given individual is. Real outcomes will vary --
some people bounce off 2048 after a single round, others lose
weekends to it.

## The near-miss claims

The 1-in-10^16 figure for the absolute end-state, and the ~1% figure
for reaching the 2048 tile, both check out -- they are derivable from
the spawn probabilities and the geometry of the board. Where the
argument gets weaker is the leap from "the game produces near-misses"
to "near-misses cause sustained engagement in 2048 specifically."

The Clark et al. (2009) near-miss fMRI work is solid, but it was done
on **gambling tasks** (slot-machine analogues) where money was at
stake. The translation to a free puzzle game with no financial loss
on the line is intuitive but, as far as we found, not directly
studied.

## What would actually settle this

If you wanted to make these claims rigorously rather than
suggestively, you would want:

- A behavioural study with 2048 sessions, capturing session-length
  distributions, return-time distributions, and quit-points relative
  to losses vs. near-misses.
- An fMRI variant of the Clark near-miss design using 2048-style
  loss-of-position rather than gambling loss.
- Comparison studies against the immediate predecessors (*Threes!*,
  *1024*) holding the loop structure constant and varying the spawn
  schedule -- to see whether the variable-ratio component is in fact
  load-bearing or whether the other design features (low friction,
  short rounds) carry most of the effect.

None of this exists in the literature as far as we could tell. The
2014 press wave generated a lot of confident prose and very little
new empirical work.

## A defensible summary

What we can confidently say about 2048's pull:

1. The game's reward structure has a variable-ratio character, and
   that family of reinforcement schedules is known to produce
   persistent behavior.
2. The game is designed (or stumbled into) extremely low friction for
   restarting, which makes "one more game" cheap in a way few of its
   peers achieve.
3. The mathematics of the win condition mean that nearly every game
   ends in an "almost," and humans handle "almost" badly under
   conditions of low cost.
4. Sessions show flow-state features (time distortion, narrow focus,
   immediate feedback) when measured by self-report.

What we cannot confidently say:

- That 2048 is "addictive" in any clinical sense.
- That dopamine specifically is the mechanism, beyond it being a
  general-purpose explanation that applies to most rewarding
  activities.
- That the gambling-research findings about near-miss psychology
  transfer cleanly to a free puzzle game.

The honest version of the story is: 2048 is a beautifully-tuned
compulsion machine, and the design choices that make it that way are
well-understood at the design level. The neuroscience layer is
suggestive rather than settled. That is a perfectly interesting story
without overclaiming.

Continue with [sources.md](sources.md) -- or back to
[README.md](README.md).
