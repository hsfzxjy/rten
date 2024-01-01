# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2024-01-01

 - Improve layout analysis (ce52b3a1, cefb6c3f). The longer term plan is to use
   machine learning for layout analysis, but these incremental tweaks address
   some of the most egregious errors.
 - Add `--version` flag to CLI (20055ee0)
 - Revise CLI flags for specifying output format (97c3a011). The output path
   is now specified with `-o`. Available formats are text (default), JSON
   (`--json`) or annotated PNG (`--png`).
 - Fixed slow OCR model downloads by changing hosting location
   (https://github.com/robertknight/rten/issues/22).

## [0.1.0] - 2023-12-31

Initial release.
