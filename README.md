# Seen

`Seen` brings `fd`-style path search and `rg`-style line search to Ruby without shelling out. It returns lazy structured results with no CLI output to parse.

## Installation

```bash
gem install seen
```

### Requirements

- Ruby 3.2+
- Rust 1.88+
- A C compiler and libclang

## Usage

### Paths

`Seen.each_path` returns an `Enumerator` of matching paths, in no particular order. `pattern` is a [Rust regex](https://docs.rs/regex) matched case-insensitively against the file name. `glob: true` makes it a glob, `case_sensitive: true` matches case, `full_path: true` matches the whole path.

```ruby
require "seen"

Seen.each_path
#=> #<Enumerator: ...>
```

Keyword options are based on `fd` flags

```ruby
Seen.each_path(extension: %w[rb rake]).to_a
Seen.each_path(pattern: "*.{rb,rake}", glob: true).to_a

Seen.each_path(
  pattern: '\.test\.js$',
  paths: %w[src test],
  type: "f",
  exclude: %w[vendor],
  case_sensitive: true
)
```

### Lines

`Seen.each_line` returns an `Enumerator` of path, one-based line number and matching line, in no particular order. A line comes back once however many times it matches. Returned text strings are frozen in every result shape.

```ruby
require "seen"

Seen.each_line(pattern: "module Seen", paths: %w[lib]) do |path, line_number, text|
  puts "#{path}:#{line_number}:#{text}"
end

Seen.each_line(pattern: "module Seen", paths: %w[lib]).to_a.sort
#=> [["lib/seen.rb", 13, "module Seen"], ["lib/seen/version.rb", 3, "module Seen"]]

Seen.each_line(pattern: "module Seen", paths: %w[lib]).group_by { |path,| path }
```

`pattern` searches file contents. Filter files with `name` and the file-selection options from `Seen.each_path`. `Seen.each_line` scans only regular files and does not accept `type`.

`pattern` is case-sensitive by default, unlike `name`, which follows `Seen.each_path`. Pass `content_case_sensitive: false` for case-insensitive matching.

Binary files are skipped at the first NUL. Pass `text: true` to scan them anyway, as `rg -a` does.

`max_count:` caps matching lines per file, as `rg -m` does. `heap_limit:` caps memory used to read a line; oversized lines follow `ignore_error:`. Like `rg`, `Seen.each_line` detects UTF-8 and UTF-16 byte order marks by default. `encoding:` accepts `"auto"`, `"none"` or a named encoding such as `"utf-16le"`, as `rg -E` does.

`column: true` yields `path, line_number, column, text` instead, one entry per occurrence rather than per line, in the shape of `rg --vimgrep`. The column is a 1-based byte offset.

`byte_range: true` yields `path, line_number, range, text` instead, where `range` is a zero-based byte `Range`, so `text.byteslice(range)` returns the match. It cannot be combined with `column: true`. Both occurrence modes keep a CR before LF in `text` so their offsets index it exactly.

### Ignore files

By default, both methods respect `.ignore` and, inside a git repository, `.gitignore`, along with higher-precedence `.fdignore` for `Seen.each_path` and `.rgignore` for `Seen.each_line`. `Seen.each_path` also reads fd's global ignore file from `$XDG_CONFIG_HOME/fd/ignore` or `~/.config/fd/ignore`.

`ignore_file:` adds your own gitignore-format files at the lowest precedence. They still apply under `no_ignore: true`, like `fd` and `rg`.

### Behavior

- **Line result shapes:** Occurrence modes include a zero-width match at the end of a final line without LF. This measured difference from `rg --vimgrep` is deliberate: omitting it would leave the matching line without an occurrence tuple, while keeping it matches Ruby's regex engine.

Paths come back under the root you gave, so `paths: ["lib"]` gives `lib/seen.rb` and `paths: ["./lib"]` gives `./lib/seen.rb`. The default root adds no prefix.

Hidden files and directories are skipped unless `hidden: true` is passed.

Missing or unreadable entries are skipped. Pass `ignore_error: false` to raise `Seen::IOError` on the first one instead. A bare `"-"` names the file `./-` rather than standard input.

`Seen.each_path` and `Seen.each_line` release the GVL, support `Timeout` and `Ctrl-C`, and run independently in threads, Ractors and forks. Wrap Enumerator consumption when setting a timeout:

```ruby
require "timeout"

Timeout.timeout(5) { Seen.each_line(pattern: "TODO", paths: %w[lib]).to_a }
```

Nothing is walked until the `Enumerator` is consumed, so a bad `pattern` or `exclude` raises then, not at the call. Take what you need with `first`, `take`, `filter` or `to_a`.

`Seen` leaves out `fd`'s owner and exotic type filters and smart case, and `rg`'s context lines, literal and multiline matching, inverted matches and replacements.

## Attribution

`Seen` incorporates code from `fd` under the MIT License.
