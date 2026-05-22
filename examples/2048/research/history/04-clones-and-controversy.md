# Clones, the "broken game" critique, and the Threemails (March-April 2014)

By late March 2014, *2048* and its derivatives had taken over the App
Store charts. *TechCrunch* counted **over two dozen** distinct "2048"
titles on iOS and noted "additional games incorporated the '2048' term
into titles/descriptions for search visibility." *Pocket Gamer* later
reported a peak rate of **roughly fifteen new Threes!/2048 clones per
day** on the App Store.

Most of these clones were not Cirulli's. He had explicitly declined to
ship a mobile version, in part to avoid being one more entrant in that
flood. The clones were third parties wrapping his open-source code, or
reimplementing the same rules independently, often with paid removal of
ads.

## The Threes! team's public response

Asher Vollmer and Greg Wohlwend put out a long post titled
**"Threemails"**, hosted at `asherv.com/threes/threemails/`. It is
roughly **45,000 words**, assembled from over 570 development emails
spanning December 2012 to December 2013, threaded together with
commentary. Wohlwend later said the editing alone took four days and
left him "drained," and credited the act of publishing with "very
potent healing powers."

The thesis: *Threes!* was the result of fourteen months of difficult
design iteration. *2048* was a weekend project that inherited a much
looser ruleset and got the credit. They are not the same game, and the
distinction matters.

### "A broken game"

The most-quoted part of Threemails is their direct claim that *2048* is
not just simpler than *Threes!*, but mechanically broken:

> When an automated script ... can beat the game, then well, that's
> broken.

This is a real fact about 2048's rules: a script that alternates two
perpendicular swipes -- for example, up/right/up/right -- will, with
high probability, accumulate large tiles and reach 2048. The looser
merge rules of *1024* / *2048* make this strategy work; the constrained
merge rules of *Threes!* make it not work.

Their other formulations in the same post:

> a simpler, easier form of Threes

> [the sting] especially when people called Threes, a game we poured
> over for nearly a year and a half, a clone of 2048.

> We do believe imitation is the greatest form of flattery ... [but]
> ideally the imitation happens after we've had time to descend slowly
> from the peak -- not the moment we plant the flag.

The last line is the heart of it. They were not arguing for some
absolute originality principle. They were arguing for **time**: the
year-plus of original work deserved a window before the iterations took
over the narrative.

### Their metric for design quality

The team contrasted reachability:

- In *2048*, the win condition (the 2048 tile) is reached by many
  players within their first sessions; "winning" is routine.
- In *Threes!* at the time of Threemails, **only six players globally**
  had reached the 6144 tile, and **nobody** had exhausted the game.

For Vollmer and Wohlwend this was the relevant evidence: a designed
game has depth that resists trivial strategies, and *Threes!* did,
where *2048* did not.

## Press framing

The coverage that picked Threemails up was sympathetic but did not
pretend the cat could be put back. Representative samples:

- *Gamezebo*, "2048 is why we can't have nice things"
- Leigh Alexander (then at *Gamasutra*) called it a "unique tragedy."
- *HuffPost* ran a piece titled "The Most Popular iPhone Game Is Just A
  Shameless Ripoff."
- *TechCrunch* and *The Verge* covered the clone flood without strongly
  taking sides on whether *2048* itself was the problem.

The conversation it kicked off -- about App Store curation, about what
"cloning" even means for a derivative form like puzzle games, about
whether platforms should reject titles that openly copy successful ones
the way Apple and Google had begun rejecting "Flappy"-named apps -- was
one of the more substantive debates the indie scene had that year.

## Where it landed

Vollmer's eventual stance, as paraphrased in later interviews: cloning
is going to happen, and the only useful response is to protect future
original games more aggressively at design time -- not by chasing
clones after the fact. The team went on to other projects (*Tumbleseed*,
*TouchTone*, others); *Threes!* itself got a Steam release on its tenth
anniversary in February 2024.

Cirulli, for his part, has always credited *Threes!* in interviews,
acknowledged the awkwardness of the situation, and has not tried to
claim originality he did not have. His own retrospective Medium piece
("2048, success and me") is the cleanest statement of that view.

Continue with [05-legacy.md](05-legacy.md).
