# CI for Radicle

Practically every software development project needs to use some form
of CI, and Radicle should enable that. This document summarizes my
understanding of our consensus of how Radicle should support CI, based
on documents and discussions elsewhere. If my understanding is
incorrect or incomplete, please enlighten me so I can update this
document.

## Meta

"I" in this document refers to Lars Wirzenius. It was easier for me to
write this way, but later, this should be turned into a neutral
architecture document.

This document lives as a separate file for now, but should be merged
to the architecture documentation, once that is merged. For reasons of
personal convenience, I'm not fond of stacking branches that are still
moving. I'll move this to a better location when it's time.

## Source documents

* <https://hackmd.io/bcOvihTwQ16OgqeJo_fktQ>
  - by cloudhead, September 2023
* <https://hackmd.io/@gsaslis/radicle-ci-broker-howto>
  - by yorgos, September 2023
* <https://cryptpad.fr/code/#/2/code/view/xWYTZUwuf6UBJT3vcklAAz7DMSyvXt-Gz0gh1ODhPP4/>
  - by Lars, September 2023

There have additionally been various discussion online and in person,
which are hard to link to.

## Terminology

There is no standard terminology for CI that all systems use. A lot of
people use GitHub terminology, as that is the dominant git platform.
For this document I use the following minimal terminology, for
convenience:

* **artifact**---a file produced when CI runs
* **build log**---the stdout and stderr output of a run
* **project**---a Radicle repository
* **run**---building a project and running its automated test suite

These terms are meant to be as generic and neutral as possible, and
meant to avoid confusion for people used to one or another CI system.

# Goal of CI support in Radicle

Context: The user is a software developer working on a project that
uses Radicle for version control. The project has an automated test
suite, and in-repository configuration for how to build the project
and run the test suite, in a format suitable for the CI engine being
used.

In the long run, the goal for CI in Radicle is "anything that makes it
easier, more fun, and faster to produce working software", but that's
not a concrete goal.

At this stage in the development of Radicle, CI support has two
concrete goals:

* When I create a patch to propose a change, I am automatically told
  if the project branch with my committed changes fails to build or
  pass its test suite. I can also manually check what the status of
  that process ("CI run") is, and find out what the build log is, to
  investigate any problems.

  - This is "build and test the patch branch".

* When a project delegate merges my patch, both they and I are
  automatically told if the merge fails due to a merge conflict, or
  if, after the merge the project no longer builds or its test suite
  fails.

  - This is "build and test the master branch after the merge". This
    is useful, because sometimes a merged change breaks the build or
    the test suite, even when there are no merge conflicts.

It is not yet clear how notifications will work.

# Status quo

We are currently aiming to support CI in two ways:

* by integrating with external CI systems

  - an integration module co-ordinates with, and controls, the
    external system, so trigger builds when Radicle sees changes, and
    to track the state of the CI run
  - Radicle will provide an interface to integrate with any reasonable
    CI engine or service
  - work is under way to support Concourse, initially
  - yorgos and Nikolas are working on this and have a proof of
    concept, see
    <https://community.radworks.org/t/radicle-ci-integrations/3394>

* by providing a native CI system

  - we don't want users of Radicle to have to set up a third party CI
    engine, or have to have access to a third-party service
  - the interface towards native CI will be as similar as possible to
    the one for integrating with external CI - having two users of the
    interface helps ensure it's not too tied to a specific CI engine
  - native CI will initially be a proof of concept
  - Lars is working on this

In other words, one or more nodes will run CI for at least some
repositories, for new and updated patches, and for changes to the
branch with the main line of development (master, main). Any node in
the network can see what CI runs happen, and can see if they succeed
or fail.

Architecturally:

* a node can choose to run external or native CI
  - no node is required to have CI, but every node is allowed to have
    CI
* a project can choose to treat a specific node's CI as authoritative,
  but this is not enforced technically, for now
  - such enforcement may be implemented later
  - e.g., a merge might require the authoritative CI to succeed on the
    merged code before the merge is published to the network
* build logs are stored with the CI instance
* a new type of COB is used to share information to nodes on the
  network about what CI runs have been started, and what their current
  state is
* a CI run is triggered by a `RefsFetched` event on a patch
  - later, we can add a new event to trigger a run manually, or by
    other things, such as via something like cron, and on arbitrary
    commits, but this will do to start with

## The job COB

Lars has sketched an initial "job COB". It intends to capture the CI
runs that have been created and their current state. The job COB
records information about CI runs that happen and does not itself
trigger or control the CI runs. The job COB can be used to record
other automated processing of the repository as well.

The job COB is to be created and maintained by either the integration
with an external CI or the native CI. The exact details of this are to
be determined.

For now, the job COB is intended to be the minimal COB that both
external CI integration and native CI can be build on:

* git commit hash of the source being built
* latest known state of the run
  - created
  - running
  - finished, successfully
  - finished, failed
* external id for the job, such as one assigned by a remote CI system
* optional URL to information about the job

Later, additional fields can and will be added, based on our and our
users' actual use cases. These will probably be things like the
information about where the build log is, where build artifacts are,
why the run was triggered, etc. However, as a matter of software
development philosophy, Lars doesn't want to add anything that doesn't
have an actual, concrete need.

However, this COB needs to be suitable, even initially, for
integrating external CI and implementing at least the most rudimentary
native CI.

## Event handling

A "CI broker" is provided in a separate repository to listed on ref
change events from the node and to invoke either native CI or an
integration with external CI.
