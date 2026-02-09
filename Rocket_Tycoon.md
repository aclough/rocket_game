# Scenario

The player is a dot com millionaire who wants to start a rocket company.  They’re somewhere between Elon Musk, Richard Branson, John Carmack, and Jeff Bezos.  They’re running their own company and competing with other new rocket companies and a few legacy “dinosaur” established players which make minimal technological investments.

Initially income comes from contracts to launch satellites to particular orbits or other activities in space or potentially taking out loans.  Fame from accomplishing missions or other achievements can unlock new missions and make loans cheaper.

As the game progresses the player will make space stations and surface outposts for research, mining, manufacturing, tourism, propellant depots, and laser energy transfer sites.

The player will research new technologies, use them to create new parts such as engines, and incorporate them in new designs.

# Simulation

**Dynamic World**
Part of the game is finding out more about the solar system they find themselves in.
We don’t really know how easy it is to get iron on the moon, say.  Each game you’re going to have to send out probes to figure this out and it will often be different from game to game.  Sometimes there might be interesting stuff like primordial black holes in the outer asteroids, alien passive probes, or such.  These aspects of the world are determined randomly for a given game so that you can’t just look them up in strategy guides and to preserve the experience of actually exploring the world.  The Earth itself will evolve as well and things like GDP and computer technology will follow different paths that might affect the player’s game, as well as events like wars or environmental disasters.

**Technology**
You have teams conducting research on various technologies like full flow staged combustion engines, gas core nuclear rockets, composite payload fairings, space 3D printers, or greenhouses.

I’d also like to have a somewhat random technological progression, both in terms of timing and in terms of what happens over the course of the game.  Different games will have different times to develop different sorts of fusion power which will have implications both for drives and for demand for He3 on Earth.  For the long run the players will have to invest in a number of different technologies only some of which will pan out depending on whether they turn out to be a good idea in this playthrough.  The player will also be able to see which technologies work out well for their competitors and so copy those approaches.

**Parts**
With a given technology you can build parts.  Many parts will require a tech level N manufactory which will always be available on Earth but which might not on outposts elsewhere in the solar system.  Parts have mass and size.

When first designed parts might have flaws that can be either revealed through testing or use.  Parts can be revised to remove flaws or to improve them.

Large production runs of parts let them be made more efficiently.  A revision will partially reset this.  This isn't discrete, however, there are both learning and forgetting curves.

Some parts will be available for sale from suppliers to the dinosaurs, or from the former Soviet Union like the
Antares's NK-33s

Flaws in third party parts can't neccesarily be fixed.

**Designs**
Parts can be assembled into rocket, spaceship, or space station designs.  The same production run and flaws dynamics that affect parts affect designs.  Most new rocket designs will explode on the first launch.

**Resources**
Things like aluminium, hydrogen, water, deuterium, etc that might be found in space.  Also things like "electronics"
that will come from the Earth even after you can get aluminium from the Moon.

**People**
Design teams.  Assembly teams.  Crews.  Marketing.

**Routes**
The player will have to worry about the tyranny of the rocket equation in how they get their ships from point A to B.
At some point they'll want to set up regular routes instead of planning things mission by mission.

**Time**
Generally rather than spending money in a lump to do something the player will engineering teams, manufacturing lines, etc which cost a certain amount to spin up and then have a continuous cost.  There are a certain amount of raw materials for a part of ship that do act more like a discrete cost.

**Seed**
We want every playthough to be in a different plausible world.  How much demand is there for space tourism?  How
dangrous are nuclear lightbulb rockets?  How much water is on the Moon?  These will need to be discovered in each
playthrough and won't be the same from one to the next.  For that reason a seed has to be generated for a new game and
stored, and then used deterministically for certain "rolls" about the world later so they aren't succeptable to save
scumming.  Other contingent things like if a rocket explodes will have a more conventional source of randomness.

# Interface

I’m thinking of a 2D pixel art approach with zooming.  Bodies like the Moon or Asteroids shouldn’t shrink as fast as zooming would make them to keep them visible as the user zooms out though at some point minor planets have to disappear to keep things from being too cluttered.  When zoomed in enough orbital bands and maybe things like the Van Allen belts can appear.

Tools to help the user figure out what the delta-V cost of a trip is and let them make designs to hit that target are going to be important for avoiding frustration and something to iterate on.

Financial planning and making projections would also be important for whether, e.g., launching a space station makes sense.  It would be good if the player can see prices accounted for with just things purchased, purchased plus the wages of employees who worked on it direclty, and price inclusive of R&D prorated over the number purchased so far.

# Income

For existing markets like launching comsats there will be a number of public orders you can rely on.  However, other things like tourism have no existing market and need to be discovered.  Also, drastic changes in the price of launches should affect how people want some service.  We need some sort of demand curve that needs to be discovered.

## Launch Provider

There will be contracts providing payment for sending various payloads to various orbits.

## Tourism

Sending people on suborbital hops, to orbit, or up to a space station.  The presence of demand for this will be strongly
tied to prestige / safety track record of the particular rocket.

## Research

Facilities on a space station for third parties to do research.

## Manufacturing

Things that can ony be made in space.  Might be gated on general technology, research done in stations, or the player's
own research.

## Comms

Like Iridium or Starlink.

## Power

Space based solar power.

# Events

Some that might be determined by the seed and wait for the conditions to trigger

- Someone wants a joint venture with your for a commsat constellation
- NASA does COTS and will fund you making a rocket and spacecraft
- Old Soviet engines available, or some other third party part.
- Or the same idea but a space station
- Aliens are discovered and NASA really, really needs a mission to Jupiter ASAP to investiage the monolith
- Helium 3 is suddenly really valuable
- Technologies being unlocked

Others might be unseeded

- Financial downturn/upturn and demand in general goes up/down for a period
- Solar flare

# Inspirations

* The High Frontier board game
* The Atomic Rockets website
* Selenium Boondocks blog

# Things to maybe add later

* Dynamic Earth events like wars affecting which missions are available
    * Which also shifts the demand curve or adds new sorts of missions
* Winged vehicles
* Communication limits giving you reasons to build comm sats on the Moon or Mars.
* Part wear
* Crew morale
* Competitors
* Trading with other companies
  * Parts
  * Resources like fuel
  * Use of facilities like lasers
* Have reputation limits on things like enriched uranium, being able to put big mass drivers on the
  Moon.
* Move your facilities up to space and declare independence?

