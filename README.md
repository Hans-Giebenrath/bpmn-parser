# BPMD - *BPM*N *D*SL

Create BPMN diagrams from text files. Similar to Mermaid/PlantUML/... but for BPMN.

*This project is in an early development stage and is missing features which are required for serious usage.*

BPMD is a tool which takes as input a text file in the BPMD format (`.bpmd`) and outputs a layouted
BPMN. The goal is to enable the author of a diagram to focus on the semantics of a diagram. However,
when creating a diagram by moving boxes and arrows, the author needs to focus at the same time on
the presentation. Simply put, this tool determines the `x`, `y`, `width` and `height` values of all
the elements in your diagram, you just describe your process.

The *hope* is that this allows for:

* Consistent layouts (e.g. equal margins within a diagram and across diagrams).
* Faster diagram sketching, and faster modification of an existing diagram (like inserting a box in
  the middle of the diagram).
* Creation of diagrams which are *good enough* for regular purposes, so you never need to
  post-process any of the diagrams except for exceptional situations with higher aesthetic
  requirements.
* An overall pleasant diagram creation experience for fans of Mermaid/PlantUML/...
* Simple enough language that can be mastered also by non-programmers.

```
= This is a Pool
== This is a Lane

# Start Event
X I am an exclusive Gateway ->one-branch"Let's Go here" ->the-other-branch"No, here!"

G <-one-branch
- Wonderful Task
G ->we-are-done

G <-the-other-branch
- Inspiring Task
G ->we-are-done

X <-we-are-done
. Have Some Rest
```

## Why could this be useful?

This tool tries to fall into the same niche as PlantUML and similar tools. The target group is (probably) primarily users which are home in text files. A couple of (subjective) benefits:

* Author can focus on content rather than Layout - an okayish layout is calculated automatically
* Later changes to the process, like adding a task activity somewhere in the middle, don't require a full (manual) rework of the surrounding layout.
* Better integration with version control systems:
  * The syntax aims to split the process description from layout tweaks. Meaning that a diffs are more meaningful (one can rather concentrate on the changes of the process than on layout peculiarities)
* Any advantage which text files bring over binary files (like one can stay within one's beloved text editor / IDE to code, document and create diagrams without mouse interruptions)

## Missing Pieces

* Bug fixes
* Boundary Events
* Message Flow Orthogonal Routing
* Marking a pool as black box
* Cycles
* Nested states
* Separate Interrupt States
* Layout-Instructions
* SVG Export (in addition to XML export)
* SVG-Based Image Export (in addition to XML export)
* Interactive SVG Export

## Goals

* Layouts should be *good enough* for everyday use to not warrant manual corrections in like 90% of
  the cases.

## Non-Goals

* Make 100% aesthetically perfect diagrams. This is subjective and depends on the human who looks at
  the diagram. In that regard I deem it impossible to achieve a *perfect* diagram layout.
* Support of *grouping*. This is just not compatible with the layout algorithm being employed.
  Groups can stretch across lanes and pools, but the algorithm is primarily lane-focused with just
  some inter-lane and inter-pool extensions. That is to say, I think one can achieve grouping for
  simple use cases, but better just use colors to denote that some nodes are semantically related.

## DSL Syntax Overview

The syntax is explained in [doc/doc.html](doc/doc.html).

## Dependencies

* TODO: Write about the cbc solver dependency. Also try to replace this with a pure Rust solver at some point.
* `bpmn-to-image`: The compiler within this repository only creates the `.xml` representation. If you want a `.png` file, you can use `bpmn-to-image` for that. In the `./doc` directory, run `npm install` and look into the `doc/build.sh` script to see how to use it.

## Usage

Use `cargo run --release -- --help` to see how to use the tool.

Basic usage: `cargo run --release -- -i file.bpmd -o file.bpmn`
