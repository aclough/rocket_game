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

Large production runs of parts let them be made more efficiently.  A revision will partially reset this.

Some parts will be available for sale from suppliers to the dinosaurs.  

**Designs**  
Parts can be assembled into rocket, spaceship, or space station designs.  The same production run and flaws dynamics that affect parts affect designs.  Most new rocket designs will explode on the first launch.

**Resources**  
Things like aluminium, hydrogen, water, deuterium, etc that might be found in space.

**Routes**  
The player will have to worry about the tyranny of the rocket equation in how they get their ships from point A to B.

# Interface

I’m thinking of a 2D pixel art approach with zooming.  Bodies like the Moon or Asteroids shouldn’t shrink as fast as zooming would make them to keep them visible as the user zooms out though at some point minor planets have to disappear to keep things from being too cluttered.  When zoomed in enough orbital bands and maybe things like the Van Allen belts can appear.

Tools to help the user figure out what the delta-V cost of a trip is and let them make designs to hit that target are going to be important for avoiding frustration and something to iterate on.

Financial planning and making projections would also be important for whether, e.g., launching a space station makes sense.

# Inspirations

* The High Frontier board game  
* The Atomic Rockets website  
* Selenium Boondocks blog

# Things to maybe add later

* Dynamic Earth events like wars affecting which missions are available  
* Communication limits giving you reasons to build comm sats on the Moon or Mars.  
* Part wear  
* Crew morale  
* Trading with other companies  
  * Parts  
  * Resources like fuel  
  * Use of facilities like lasers  
* Have reputation limits on things like enriched uranium, being able to put lasers in orbit  
* Move your facilities up to space and declare independence?

