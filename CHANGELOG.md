# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Bug Fixes
- forgot one instance by @Karrq in
https://github.com/chainbound/prometric/pull/61
### Bug Fixes
### Features
- option to export process metrics at an interval by @merklefruit in
https://github.com/chainbound/prometric/pull/52
### Bug Fixes
- reintroduce basic (smaller) example by @Karrq in
https://github.com/chainbound/prometric/pull/47
- use appropriate parse_quote macro by @Karrq in
https://github.com/chainbound/prometric/pull/47
- error with invalid partitions config by @Karrq in
https://github.com/chainbound/prometric/pull/47
- enforce metric help string by @Karrq in
https://github.com/chainbound/prometric/pull/47
- pr review by @Karrq in
https://github.com/chainbound/prometric/pull/47
- clippy by @Karrq in
https://github.com/chainbound/prometric/pull/47
- commit deadlock by @Karrq in
https://github.com/chainbound/prometric/pull/47
- account for outstanding loads by @Karrq in
https://github.com/chainbound/prometric/pull/47
- BatchOpts, unnecessary const-hack by @Karrq in
https://github.com/chainbound/prometric/pull/47
- clippy by @Karrq in
https://github.com/chainbound/prometric/pull/47
### Documentation
- more comments in prometric-derive by @Karrq in
https://github.com/chainbound/prometric/pull/47
- ensure generated docs link to items by @Karrq in
https://github.com/chainbound/prometric/pull/47
- clarify comment by @Karrq in
https://github.com/chainbound/prometric/pull/47
### Features
- GenericSummaryMetric (cheap clone) by @Karrq in
https://github.com/chainbound/prometric/pull/47
- expand metrics with user path by @Karrq in
https://github.com/chainbound/prometric/pull/47
- Summary metric support by @Karrq in
https://github.com/chainbound/prometric/pull/47
- `ArcCell` by @Karrq in
https://github.com/chainbound/prometric/pull/47
- batching summary by @Karrq in
https://github.com/chainbound/prometric/pull/47
- summary metric by @Karrq in
https://github.com/chainbound/prometric/pull/47
### Refactor
- move examples under examples/ by @Karrq in
https://github.com/chainbound/prometric/pull/47
- quote fully qualified imports by @Karrq in
https://github.com/chainbound/prometric/pull/47
- use existing `arc-cell` by @Karrq in
https://github.com/chainbound/prometric/pull/47
- split into more modules by @Karrq in
https://github.com/chainbound/prometric/pull/47
- export 2 more items by @Karrq in
https://github.com/chainbound/prometric/pull/47
- split metrics in modules by @Karrq in
https://github.com/chainbound/prometric/pull/47

## `v0.1.4`
### Bug Fixes

- fix thread usage by @mempirate in
https://github.com/chainbound/prometric/pull/39
- don't enable process feature by default by @mempirate in
https://github.com/chainbound/prometric/pull/33

### Features
- support expressions that evalute into a Vec<f64> for buckets by @thedevbirb in
https://github.com/chainbound/prometric/pull/37
- add thread busyness stats by @mempirate in
https://github.com/chainbound/prometric/pull/38
- add collection time metric, more system stats by @mempirate in
https://github.com/chainbound/prometric/pull/33

## `v0.1.3`
### Bug Fixes
- fix CPU usage, no default feature by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>
- default scrape path /metrics by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>
- blocking issue by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>

### Documentation
- fix docs by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>
- document process metrics by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>
- update README doc order by @mempirate in
  <https://github.com/chainbound/prometric/pull/26>
- add exporter docs by @mempirate in
  <https://github.com/chainbound/prometric/pull/26>
- add metric constructor documentation by @mempirate in
  <https://github.com/chainbound/prometric/pull/25>
- more metric types documentation by @mempirate in
  <https://github.com/chainbound/prometric/pull/21>

### Features
- add exporter example by @mempirate in
  <https://github.com/chainbound/prometric/pull/26>
- add HTTP exporter utilities by @mempirate in
  <https://github.com/chainbound/prometric/pull/26>
- don't collect system swap by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>
- add some system metrics by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>
- add default impl by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>
- add process metrics by @mempirate in
  <https://github.com/chainbound/prometric/pull/30>
- add expression support for buckets by @thedevbirb in
  <https://github.com/chainbound/prometric/pull/37>
