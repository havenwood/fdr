# Fdr

`Fdr` is a fast file search gem for Ruby, implemented with a Rust native extension directly derived from [fd](https://github.com/sharkdp/fd). Rust deps include ripgrep's [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore), [globset](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset), [grep-regex](https://github.com/BurntSushi/ripgrep/tree/master/crates/regex) and [grep-searcher](https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher) crates, plus [regex](https://github.com/rust-lang/regex) and [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam).

`Fdr` doesn't ship with an `fdr` executable. Use `Fdr` from Ruby and the real `fd` from the command line.

## Installation

Install `Fdr`:

```bash
gem install fdr
```

Or add it to your app and `bundle install`:

```ruby
gem 'fdr'
```

### Requirements

- Ruby 3.2+
- Rust 1.88+
- A C compiler and libclang for the build (Xcode Command Line Tools on macOS, `clang`/`libclang-dev` on Linux)

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

Fdr.search(pattern: '**/*.{rb,rake}', glob: true)

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

`Fdr.grep` returns a path-sorted `Hash` of files and their one-based matching line numbers. Each line appears at most once.

```ruby
Fdr.grep(pattern: 'TODO|MARK', paths: %w[lib spec])
# => {"lib/example.rb" => [7, 22], "spec/example_spec.rb" => [3]}
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

Some non-CLI `fd` features `Fdr` lacks: owner filters, the executable/empty/socket/pipe/device file types, smart case switching and `.fdignore` support.

Filenames that aren't valid UTF-8 come back with `U+FFFD` replacements, like `fd`'s output. Input `paths` take any byte sequence, including `Pathname` objects and non-UTF-8 strings. Colliding replacement paths in `Fdr.grep` share one result with merged line numbers.

## Releasing

1. Bump `lib/fdr/version.rb` and commit.
2. `bundle exec rake release`, which verifies the packaged gem, tags the version, pushes the source and publishes to RubyGems.

## Attribution

`Fdr` directly borrows code from `fd` under an MIT license.
