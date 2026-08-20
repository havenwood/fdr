# Fdr

`Fdr` is a fast file search gem for Ruby, implemented with a Rust native extension based on [fd](https://github.com/sharkdp/fd). Its Rust dependencies include ripgrep's [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore), [globset](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset), [grep-regex](https://github.com/BurntSushi/ripgrep/tree/master/crates/regex) and [grep-searcher](https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher) crates, plus [regex](https://github.com/rust-lang/regex) and [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam).

`Fdr` intentionally lacks an `fdr` executable, since `fd` is perfect for that job. If you need fast file searching in a CLI, use `fd`. If you need it from your Ruby code, use `Fdr`.

## Installation

Install `Fdr` from RubyGems:

```bash
gem install fdr
```

Or add it to your application's `Gemfile`, then run `bundle install`:

```ruby
gem 'fdr'
```

Precompiled gems are provided for x86-64 and ARM64 macOS, and for x86-64 and ARM64 Linux with glibc or musl. Other platforms build the extension from source.

### Requirements

- Ruby 3.2+
- Rust 1.88+ for source builds

## Usage

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

Search is case-sensitive by default and works one line at a time, so patterns can't span lines. Binary files are skipped.

### Gaps

Some non-CLI `fd` features `Fdr` lacks: owner filters, nonfile types, smart case switching and `.fdignore` support.

## Releasing

Precompiled gems come from the `Package gems` workflow. `scripts/fetch-gems` downloads and verifies them into `gems/`.

1. Bump `lib/fdr/version.rb`, commit and tag.
2. `git push && git push --tags`
3. `gh workflow run "Package gems" && gh run watch`
4. `scripts/fetch-gems`
5. `for gem in gems/*.gem ; do gem push "$gem" ; done`

## Attribution

`Fdr` directly borrows code from `fd`, under an MIT license.
