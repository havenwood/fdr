# Fdr

`Fdr` is a Ruby gem for `fd`-style file search and `rg`-style content search without shelling out. It cuts startup overhead and returns lazy structured results with no CLI output to parse.

## Installation

```bash
gem install fdr
```

### Requirements

- Ruby 3.2+
- Rust 1.88+
- A C compiler and libclang

## Usage

### Search

`Fdr.search` returns an `Enumerator` of matching paths, in no particular order. `pattern` is a [Rust regex](https://docs.rs/regex) matched case-insensitively against the file name. `glob: true` makes it a glob, `case_sensitive: true` matches case, `full_path: true` matches the whole path.

```ruby
require "fdr"

Fdr.search
#=> #<Enumerator: ...>
```

Keyword options are based on `fd` flags

```ruby
Fdr.search(extension: %w[rb rake]).to_a
Fdr.search(pattern: "*.{rb,rake}", glob: true).to_a

Fdr.search(
  pattern: '\.test\.js$',
  paths: %w[src test],
  type: "f",
  exclude: %w[vendor],
  case_sensitive: true
)
```

### Grep

`Fdr.grep` returns an `Enumerator` of path, one-based line number and matching line, in no particular order. A line comes back once however many times it matches. Returned text strings are frozen in every result shape.

```ruby
require "fdr"

Fdr.grep(pattern: "module Fdr", paths: %w[lib]) do |path, line_number, text|
  puts "#{path}:#{line_number}:#{text}"
end

Fdr.grep(pattern: "module Fdr", paths: %w[lib]).to_a.sort
#=> [["lib/fdr.rb", 13, "module Fdr"], ["lib/fdr/version.rb", 3, "module Fdr"]]

Fdr.grep(pattern: "module Fdr", paths: %w[lib]).group_by { |path,| path }
```

`pattern` searches file contents. Filter files with `name` and the file-selection options from `Fdr.search`. `Fdr.grep` scans only regular files and does not accept `type`.

`pattern` is case-sensitive by default, unlike `name`, which follows `Fdr.search`. Pass `content_case_sensitive: false` for case-insensitive matching.

Binary files are skipped at the first NUL. Pass `text: true` to scan them anyway, as `rg -a` does.

`column: true` yields `path, line_number, column, text` instead, one entry per occurrence rather than per line, in the shape of `rg --vimgrep`. The column is a 1-based byte offset.

`byte_range: true` yields `path, line_number, range, text` instead, where `range` is a zero-based byte `Range`, so `text.byteslice(range)` returns the match. It cannot be combined with `column: true`. Both occurrence modes keep a CR before LF in `text` so their offsets index it exactly.

### Ignore files

By default, both `search` and `grep` respect `.ignore` and, inside a git repository, `.gitignore`, along with higher-precedence `.fdignore` for `Fdr.search` and `.rgignore` for `Fdr.grep`. `Fdr.search` also reads fd's global ignore file from `$XDG_CONFIG_HOME/fd/ignore` or `~/.config/fd/ignore`.

`ignore_file:` adds your own gitignore-format files at the lowest precedence. They still apply under `no_ignore: true`, like `fd` and `rg`.

### Behavior

- **Grep result shapes:** Occurrence modes include a zero-width match at the end of a final line without LF. This measured difference from `rg --vimgrep` is deliberate: omitting it would leave the matching line without an occurrence tuple, while keeping it matches Ruby's regex engine.

Paths come back under the root you gave, so `paths: ["lib"]` gives `lib/fdr.rb` and `paths: ["./lib"]` gives `./lib/fdr.rb`. The default root adds no prefix.

Hidden files and directories are skipped unless `hidden: true` is passed.

Missing or unreadable entries are skipped. Pass `ignore_error: false` to raise `Fdr::IOError` on the first one instead. A bare `"-"` names the file `./-` rather than standard input.

`Fdr.search` and `Fdr.grep` release the GVL, support `Timeout` and `Ctrl-C`, and run independently in threads, Ractors and forks. Wrap Enumerator consumption when setting a timeout:

```ruby
require "timeout"

Timeout.timeout(5) { Fdr.grep(pattern: "TODO", paths: %w[lib]).to_a }
```

Nothing is walked until the `Enumerator` is consumed, so a bad `pattern` or `exclude` raises then, not at the call. Take what you need with `first`, `take`, `filter` or `to_a`.

`Fdr` leaves out `fd`'s owner and exotic type filters and smart case, and `rg`'s context lines, literal and multiline matching, inverted matches and replacements.

## Attribution

`Fdr` incorporates code from `fd` under the MIT License.
