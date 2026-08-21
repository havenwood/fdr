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

`Fdr.search` gives you back a path-sorted `Array` of matching paths, rooted at the `paths` you pass, so the default `['.']` gets you `./`-prefixed strings. Options mirror `fd`'s flags: patterns are [Rust regex](https://docs.rs/regex) unless you pass `glob: true`, matching is case-insensitive by default, `exclude` is always globs, sizes are bytes and times are seconds ago.

```ruby
require 'fdr'

Fdr.search
Fdr.search(extension: 'rb')
Fdr.search(pattern: '**/*.{rb,rake}', glob: true)

Fdr.search(
  pattern: '\.test\.js$',
  paths: %w[src test],
  type: 'f',
  exclude: %w[vendor],
  case_sensitive: true
)

Fdr.search(
  pattern: 'thought.*snow|garret.*auction|foul.*thing',
  paths: [File.expand_path('~/boo'), File.expand_path('~/vault')],
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
```

### Grep

`Fdr.grep` returns a path-sorted `Hash` of files and their one-based matching line numbers. Each line appears at most once.

```ruby
Fdr.grep(pattern: 'TODO|MARK', paths: %w[lib spec])
# => {"lib/example.rb" => [7, 22], "spec/example_spec.rb" => [3]}
```

`pattern` searches file contents. Use `name` and the file-selection options from `Fdr.search` to narrow down the files. `Fdr.grep` only scans regular files, so it does not take `type`.

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

Content matching is case-sensitive by default, unlike `name`, which follows `Fdr.search`; pass `content_case_sensitive: false` to flip it.

### Gaps

Missing `fd` features: owner filters, the executable/empty/socket/pipe/device types, smart case and `.fdignore`. `Fdr` isn't Ractor-safe, so a non-main Ractor raises `Ractor::UnsafeError`.

Paths come back as raw bytes tagged with the filesystem encoding, like `Dir.glob`, so a non-UTF-8 name still opens. Input `paths` take any bytes, including `Pathname`.

## Attribution

`Fdr` directly borrows code from `fd` under an MIT license.
