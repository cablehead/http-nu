# Flow, time distortion, and the cost of restarting

The reinforcement-schedule story in
[02-behavioral-design.md](02-behavioral-design.md) explains why a 2048
*session* is sticky. It does not fully explain why *the next session*
is, too. That part is about flow and about how cheap restarting is.

## Flow

Mihály Csikszentmihalyi's **flow state** (1975, formalised in *Flow:
The Psychology of Optimal Experience*, 1990) is the experience of
total absorption in an activity. The conditions he identified:

- Clear immediate goals.
- Immediate, unambiguous feedback.
- A challenge level matched to the player's skill -- neither boring
  nor overwhelming.
- Concentration on a narrow, defined task.
- An action loop with no inserted friction.

A symptom of flow is **time distortion**: subjective time compresses.
"I'll play one more game" turns into forty minutes that felt like
five. Players of 2048 report this routinely, and it shows up in every
casual-coverage piece on the game.

Crucially, flow is not the same thing as addiction. Flow is the
positive state; the addictive *risk* comes from a game's flow being
combined with the reinforcement structures from the previous chapter.
A long jog produces flow too, but you can't do it for ten hours
straight in five-minute increments while waiting for the bus. The
combination is the dangerous part.

## The friction story

The single most under-discussed property of 2048 is how *cheap*
restarting is. Compare:

- A round of *Civilization* costs hours. The cost of starting a new
  game is enormous.
- A round of *Threes!* costs minutes, but every round you do not save
  loses your high score, which feels like sunk cost.
- A round of 2048 costs five-to-twenty minutes. There is no score
  saved between sessions on the original implementation. Restarting
  costs literally nothing -- no progress lost, no apology owed.

The "one more game" heuristic only works if "one more" is genuinely
cheap. 2048 made it as cheap as it could possibly be:

- Single keystroke or swipe to start.
- No login, no account, no payment.
- Web URL, opens in a tab, no install.
- No animation forcing you through a "would you like to try again?"
  modal -- press `r`, board resets, the next game has already begun.

This is design, not accident. Cirulli built the game to be the
opposite of frictioned mobile games of the era, which interrupted
every loss with an ad. The friction floor of 2048 is below any of its
predecessors, which is part of why the variant that went viral was
this one and not the others.

## "Just one more" as a probability error

The interaction between cheap restarting and near-miss math is the
specific trap. Each game ends in "almost." The marginal cost of one
more attempt feels trivial. But each attempt is independent: your
chance of reaching 2048 on the next game is not informed by your
near-miss on this one. You are not "due."

The brain handles this poorly. Gambler's-fallacy-style reasoning ("I
came so close, surely this time") is the same family of error that
slot machines exploit. 2048's design does not actively push this --
nobody is whispering "you're due" -- but the *math* puts you in
exactly the position where the error feels rational.

## The Zeigarnik effect, and why "almost" stays in your head

The pull between sessions has a name. In 1927, the Lithuanian-Soviet
psychologist **Bluma Zeigarnik**, working under Kurt Lewin in Berlin,
published an experiment showing that subjects remembered interrupted
tasks roughly twice as often as completed ones. The finding -- now
called the **Zeigarnik effect** -- is that the mind keeps an
incomplete task active and accessible until it is either finished or
deliberately dismissed.

Primary citation: Zeigarnik, B. (1927). *Uber das Behalten von
erledigten und unerledigten Handlungen*. *Psychologische Forschung*,
9, 1-85.

The effect is well-replicated in the memory literature and has been
explicitly applied to game design by **Scott Rigby and Richard Ryan**
in *Glued to Games: How Video Games Draw Us In and Hold Us Spellbound*
(2011), where they argue MMO quest logs are engineered to keep at
least one incomplete task visible at all times -- closing a quest
opens two new ones, so the Zeigarnik tension never resolves.

2048's version of this is implicit rather than designed: every game
ends in a partly-built tile that never reached its merge. The 1024
sitting on the board with no 1024 to pair against. The plan that
would have worked if the spawn had landed two cells over. These are
not unfinished *quests*, but they are unfinished *cognitive tasks* in
exactly the Zeigarnik sense -- they have a known successor state, you
can mentally see it, you didn't get there.

This is the *between-session* mechanism. The variable-ratio
reinforcement in [02-behavioral-design.md](02-behavioral-design.md)
explains why you don't stop mid-session; the cheap-restart story
above explains why "one more" is structurally tempting; Zeigarnik
explains why, an hour later, the unfinished board is still quietly
in your head. The three together are how a session-bound game
becomes an across-the-day presence.

Note (consistent with the caveat in
[04-where-the-evidence-thins.md](04-where-the-evidence-thins.md)):
the Zeigarnik effect itself is well-established. The application of
it to 2048 specifically is plausible inference, not a measured study.
The Rigby and Ryan extension to games is a real published argument
but is mostly about MMO quest design, not puzzle games.

Continue with [04-where-the-evidence-thins.md](04-where-the-evidence-thins.md).
