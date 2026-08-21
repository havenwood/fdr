# Fdr

`Fdr` is a fast file-search gem for Ruby. Its Rust extension is derived from [fd](https://github.com/sharkdp/fd) and uses ripgrep's [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore), [globset](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset), [grep-regex](https://github.com/BurntSushi/ripgrep/tree/master/crates/regex) and [grep-searcher](https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher) crates, plus [regex](https://github.com/rust-lang/regex).

`Fdr.search` covers most of `fd` and `Fdr.grep` just a small part of [ripgrep](https://github.com/BurntSushi/ripgrep). Use `Fdr` from Ruby and `fd` or `rg` from shell. `Fdr` doesn't have a CLI executable.

## Installation

Install the gem:

```bash
gem install fdr
```

Or add it to your `Gemfile`:

```ruby
gem 'fdr'
```

### Requirements

- Ruby 3.2+
- Rust 1.88+
- A C compiler and libclang

## Usage

`Fdr.search` returns a path-sorted `Array` of matching paths. Like `fd`, hidden files and ignore-file entries (`.gitignore` and friends) are skipped unless `hidden: true` or `no_ignore: true` is passed. Patterns are [Rust regex](https://docs.rs/regex) Strings, or globs with `glob: true`; matching is case-insensitive by default. Invalid regexes raise `RegexpError`, while invalid globs and option values raise `ArgumentError`. `type` takes `'f'`/`'file'`, `'d'`/`'dir'`/`'directory'` or `'l'`/`'symlink'`. `min_size` and `max_size` are bytes, while `changed_within` and `changed_before` are seconds before now. Nonexistent `paths` and unreadable entries are silently skipped, so a search returns whatever was reachable. With `full_path: true`, patterns match against absolute paths, so globs need a `**/` prefix to match subpaths.

```ruby
require 'fdr'

Fdr.search(extension: 'rb')

Fdr.search(
  pattern: 'test',
  paths: %w[lib spec],
  type: 'f'
)

Fdr.search(
  pattern: 'config',
  paths: %w[app config],
  extension: 'yml',
  type: 'f',
  max_depth: 3,
  hidden: true
)

Fdr.search(
  pattern: '\.test\.js$',
  paths: %w[src test],
  exclude: %w[node_modules vendor],
  case_sensitive: true
)

Fdr.search(pattern: '*.{rb,rake}', glob: true)

Fdr.search(
  extension: 'log',
  min_size: 1024 * 1024,
  changed_within: 86400,
  paths: %w[logs]
)

Fdr.search(
  pattern: 'thought.*snow|garret.*auction|foul.*thing',
  paths: [File.expand_path('~/garret'), File.expand_path('~/vault')],
  extension: 'txt',
  type: 'f',
  hidden: true,
  no_ignore: true,
  case_sensitive: false,
  glob: false,
  full_path: true,
  max_depth: 7,
  min_depth: 1,
  exclude: %w[publication creator],
  follow: true,
  min_size: 1,
  max_size: 1_048_576,
  changed_within: 31_536_000,
  changed_before: 604_800
)

# Aliases for `Fdr.search`:
Fdr.entries(extension: 'rb')
Fdr.scan(extension: 'rb')
```

### Grep

`Fdr.grep` returns a path-sorted `Hash` whose values map one-based line numbers to matching text. A line appears once regardless of how many times it matches.

```ruby
Fdr.grep(pattern: 'module Fdr', paths: %w[lib])
#=> {"lib/fdr.rb" => {13 => "module Fdr"}, "lib/fdr/version.rb" => {3 => "module Fdr"}}

Fdr.grep(pattern: 'TODO').keys                     # files that match
Fdr.grep(pattern: 'TODO').transform_values(&:size) # match counts per file
```

`pattern` searches file contents. Use `name` and the usual `Fdr.search` options to narrow down the files.

```ruby
Fdr.grep(
  pattern: 'TODO',
  name: '_spec\.rb$',
  paths: %w[spec],
  extension: 'rb',
  max_depth: 3,
  hidden: true
)
```

`Fdr.grep` is case-sensitive by default (`case_sensitive` applies to both `pattern` and `name`) and works one line at a time, so patterns can't span lines. Binary files are skipped.

### Gaps

Missing: `fd`'s owner filters, executable/empty/socket/pipe/device types, smart case and `.fdignore`, plus `rg`'s context lines, fixed-string and multiline matching, inverted matches and replacements. No `--crlf` either, so `needle$` won't match before a CRLF.

Filenames that aren't valid UTF-8 come back with `U+FFFD` replacements, like `fd`'s output. Input `paths` take any byte sequence, including `Pathname` objects and non-UTF-8 strings. Colliding replacement paths in `Fdr.grep` share one result with merged line numbers.

## Releasing

1. Bump `lib/fdr/version.rb` and commit.
2. `bundle exec rake release`, which verifies the packaged gem, tags the version, pushes the source and publishes to RubyGems.

## Attribution

`Fdr` incorporates code from `fd` under the MIT License.
