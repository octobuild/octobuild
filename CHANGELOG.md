# Changes

## 0.1.5

 * Add show some cache statistics after build finish.
 * Fix partically saved files from cache on IO-errors (like out-disk-space).
 * Clang: Don't use octobuild on --analyze
 * Clang: Add support cache for cross-compiler

## 0.1.4

 * Join i686 and x86_64 builds to single .nupkg Chocolatey package (fix #4).
 * Don't require reboot for apply PATH environment variable (fix #9).

## 0.1.3

 * Fix panicked at 'called `Result::unwrap()` on an `Err` value: "SendError(..)"' (fix #8).
 * Minor performance improvement.

## 0.1.2

 * Remove comments from clang preprocessed output for more cache hits.

## 0.1.1

 * Rewrite .deb packaging.

## 0.1.0

 * First release.
