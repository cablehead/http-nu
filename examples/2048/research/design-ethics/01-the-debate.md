# The debate

"Addictive" is one of those words in game-design vocabulary that
has migrated from compliment to indictment and back, sometimes
within the same sentence. A pull-quote on the App Store calling a
game "addictive" is marketing. A think-piece in *The Atlantic*
calling the same game "addictive" is an accusation. The
disagreement is not semantic. It is about whether the design
techniques the word names are legitimate craft or something the
field should be quietly ashamed of.

## The Blow pole

The clearest, loudest version of "indictment" comes from
**Jonathan Blow** -- designer of *Braid* (2008) and *The Witness*
(2016), both indie titles that succeeded commercially while
deliberately resisting most retention-engineering techniques. Blow
has been on the record about this for over a decade.

The canonical statement is a 2012 *PC Gamer* interview titled "Do
you believe social games are evil? 'Yes. Absolutely.'" The
substance:

> [Social games are] predicated on a kind of player exploitation.

> [...] through Skinnerian reward schedules, designers addict
> players to boring treadmills.

> We convince [players] to pay us money and waste their lives in
> front of our games.

He has elsewhere compared the design of MMO retention systems to
cigarettes and junk-food advertising -- products that work
precisely because they bypass deliberation. The argument is not
that engagement is wrong; it is that engagement *manufactured by
exploiting known cognitive vulnerabilities* is wrong even when the
exploitation is legal, profitable, and well-reviewed.

Blow's position has stayed remarkably consistent. He has not
softened the claim. In his 2010 Creative Mornings PDX talk and
subsequent appearances, the framing is that game design is a
medium with the same kinds of ethical questions any medium has,
and that "optimizing for engagement" is the equivalent of optimizing
for shelf-life in food -- a thing you can do, that the market will
reward, and that nonetheless makes the product worse for the
person who consumes it.

## The Lantz pole

The most thoughtfully-articulated version of "the same machinery
can be used for craft" comes from **Frank Lantz**, director of the
NYU Game Center and designer of *Universal Paperclips* (see
[../incrementals/03-universal-paperclips.md](../incrementals/03-universal-paperclips.md)).
Lantz's book *The Beauty of Games* (MIT Press, 2024) is the
extended argument.

Lantz does not disagree with Blow about the harm. He has been
explicit, repeatedly, that **slot machines have "dark energy and
can ruin people's lives."** The disagreement is about whether the
machinery underneath slot machines is *only* good for slot machines.

His position, restated from interviews and from *The Beauty of
Games*: the same reinforcement structures that produce compulsion
can produce **depth**. The same variable-ratio reward that drives
a slot machine drives chess at a different time-scale -- a
chess player is being rewarded uncertainly for moves whose value
they cannot fully predict, and that uncertainty is part of why
chess is *interesting*, not just why it is compelling. The
mechanism is morally neutral. What matters is what is on the
other side of the reward.

A close paraphrase of the Lantz argument:

> Good game designers know about the dark energy in slot machines.
> They recognise that they could incorporate it. They choose not
> to -- or rather, they incorporate the parts of it that produce
> attention without producing emptiness, and decline the parts
> that produce attention only.

The position has more nuance than "engagement is fine." It is that
"engagement" is the wrong unit of measurement: it does not
distinguish between attention paid to depth and attention paid to
emptiness, and a designer who optimises for engagement without
that distinction is producing slot machines whether they know it
or not. Lantz's *Universal Paperclips* is the design statement of
this position -- the same toolkit, used for an experience that
ends, that means something, and that the player is glad they had
afterwards.

## The Threes! case

The two poles are abstract. The most concrete, well-documented
case in this debate is the one already covered in
[../history/04-clones-and-controversy.md](../history/04-clones-and-controversy.md):
**Asher Vollmer and Greg Wohlwend's design rejections during
Threes! development.**

The documented facts:

- Sirvo spent fourteen months on *Threes!*. The "Threemails"
  retrospective (570+ emails, 45,000 words) traces the iteration.
- They built and explicitly rejected variants closer to what
  became *1024* and *2048* -- versions with looser merge rules,
  faster pacing, all-direction sliding.
- Their stated reason for rejecting these was that the looser
  versions produced a game that was *easier to keep playing* and
  *harder to think about*. They did not call this a virtue.

The published critique of 2048 in Threemails is structural: "When
an automated script ... can beat the game, then well, that's
broken." The mechanical fragility was the indictment, but the
deeper claim was about *what the design was for*. *Threes!* was
designed to be played for years, in short sessions, the way a
favourite album sits on a shelf. *2048* was designed (or rather,
emerged) to be played in long sessions for a few weeks until the
mechanic exhausts itself.

Both games are real outputs of the same conceptual neighbourhood.
The market clearly preferred the second. The Threemails post is
the most articulate defense in the field of having made the first
choice anyway, knowing it would lose.

## The economic context

The argument is sharper in some corners of the industry than in
others, and the reason is structural, not philosophical.

- **Premium indie** (one-time purchase, no in-app purchases, no
  ads): the designer's incentive is *recommendation quality*, not
  session length. A game that respects your time gets recommended.
  *Threes!* at $1.99/$2.99 on the App Store, *Braid* on Steam,
  *Universal Paperclips* free with no monetisation -- these
  business models do not benefit from retention engineering, and
  the designers in them are conspicuously the most likely to
  publicly criticise it.

- **Free-to-play mobile** (ads, IAP, daily login bonuses,
  battle passes): every additional minute of session and every
  additional day of retention has direct revenue value. Retention
  engineering is not a philosophical question for these designers;
  it is the *product*. Studios talk about DAU, MAU, ARPU, and LTV
  the way other industries talk about margins.

- **AAA console / PC games** (one-time purchase plus expansion
  packs and DLC): mixed. The base game often resembles premium
  indie incentives; the live-service overlay (battle passes,
  seasonal content) resembles F2P mobile.

It is no coincidence that Blow, Lantz, Vollmer, Wohlwend, Bogost,
Pipkin, Foddy, and most of the publicly-critical voices in this
debate are working under the first or third structures. The
designers under the second structure rarely engage publicly; their
companies generally do not benefit from a conversation in which
their products are characterised as cigarettes.

## Why this is unresolved, and probably stays that way

The reason the argument does not converge is that **both sides are
right about the things they emphasise**, and the things they
emphasise are not the same things.

- Blow is right that techniques developed for slot machines,
  applied without ethical constraint, produce harm at scale, and
  that "addictive" used as praise launders that harm.
- Lantz is right that the underlying mechanisms are not
  intrinsically harmful, that depth and compulsion share
  machinery, and that condemning the machinery condemns chess and
  rock climbing along with the slot machine.
- Vollmer is right that even well-meaning designers face market
  pressure to lean toward the compulsion end of the dial, and
  that the designer's job partly is to resist that pressure even
  at commercial cost.

There is no synthesis that resolves all three at once. What there
is, instead, is a craft tradition that has internalised the
disagreement and treats "is this game's hold on the player earned
or stolen" as a real, asked-out-loud question during design. That
is more progress than most adjacent industries have made.

This research bundle's own posture is closer to Lantz's than to
Blow's -- the existence of this example, this analysis, this
attempt to understand 2048 rather than just play or just refuse to
play it, is itself a bet that the machinery is worth knowing
rather than worth shunning. But Blow's position is the one to
keep in mind whenever you find yourself saying a game is
"addictive" and meaning it as praise.

Back to [README.md](README.md), or see [sources.md](sources.md).
