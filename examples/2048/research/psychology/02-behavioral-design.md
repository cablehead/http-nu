# Behavioral design: variable-ratio reinforcement and near-miss

The dopamine story in [01-the-dopamine-account.md](01-the-dopamine-account.md)
is a neuroscience layer on top of an older and more specific
behavioural-psychology story. The older story names the *design choice*
that makes 2048 hard to put down, where the dopamine story only names
the *consequence*.

## Variable-ratio reinforcement

In **B. F. Skinner's** schedule-of-reinforcement work in the 1950s,
four basic schedules were identified for how often a behavior gets
rewarded:

- **Fixed-ratio.** Reward every Nth action. Predictable. Easy to stop
  once the reward arrives.
- **Fixed-interval.** Reward after every N seconds. Easy to wait out.
- **Variable-interval.** Reward at random *times*. Moderately sticky.
- **Variable-ratio.** Reward after a random number of *actions*.
  Hugely sticky -- this is what slot machines run on, and it produces
  behavior that resists extinction more strongly than any other
  schedule Skinner tested.

2048's reward stream is variable-ratio. The merges themselves are
deterministic (same-value tiles combine), but the *value* of those
merges is gated by:

- where the next tile spawns (random, uniform over empty cells),
- what value the next tile is (90% a 2, 10% a 4).

A swipe that "should" have set up a four-into-eight might be
sabotaged by a 4 spawning in the corner you needed. Or it might be
made trivially good by a 2 landing exactly where you wanted one. The
player cannot predict the rate of reward; they can only keep playing
to find out.

This is the structural property the dopamine account is downstream of.
"My brain releases dopamine because I am making a prediction" is a
true sentence; "the *reason* I keep making predictions is that the
reward schedule is variable-ratio" is the underlying mechanism.

## The compulsion loop

The contemporary game-design term for the per-action cycle is the
**compulsion loop**:

1. **Anticipation.** Look at the board, plan a move.
2. **Action.** Swipe.
3. **Feedback.** Tiles slide, maybe merge, score increments, a new
   tile spawns.
4. **Evaluation.** Did the board get better or worse? Plan the next
   move.

2048's loop is tight -- under a second per cycle in active play. The
shorter the loop, the more reinforcement events per session, and the
faster the conditioning. Slot machines deliberately optimize for this;
mobile match-three games do too. 2048 inherited it from *Threes!* and
*1024* and tightened it further by removing the one-cell slide
constraint.

## Near-miss

The third behavioral lever is the **near-miss effect**: under
fMRI, near-misses in gambling tasks activate reward circuitry in
patterns similar to actual wins, not to losses (Clark et al., 2009,
in *Neuron* -- the canonical reference). Near-misses extend persistence
on losing tasks more than clear losses do.

2048's geometry guarantees near-misses. From Lê Nguyên Hoang's
mathematical analysis at *Science4All*: reaching the actual end-state
of the game (the largest possible tile, 131072) requires roughly
**sixteen specific lucky spawns at the right moments**, with a joint
probability of at most about 1 in 10^16. The "win" tile -- 2048 -- is
reachable but, as *BuzzFeed News* reported in April 2014, only **about
347,000 of the first 42 million games played** actually hit it. Less
than 1%.

The result is that virtually every game ends in an "almost." You had
the 1024 tile, you had the 512 ready to feed it, one bad spawn locked
the corner, game over. The near-miss is not a marketing trick -- it is
emergent from the rules. But it is the behavioral effect that the
near-miss research warns is the most persistent driver of "one more
game."

## Putting these together

These three -- variable-ratio reward, tight compulsion loop, structural
near-miss -- form a coherent design analysis that doesn't depend on
neuroscience at all. The dopamine story explains *what your brain does
in response*. This is the story about *what 2048 does to you*.

Continue with [03-flow-and-friction.md](03-flow-and-friction.md).
