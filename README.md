# CodeAtlas2
Turns a codebase into a structured, queryable knowledge graph.
Instead of searching text, you query symbols, relationships, and dependencies.

## API
- get_symbol
- list_function
- calls
- find <symbol> [-r repo | -d dir] [-l]

## Output
- every command run still appends a `finished with duration = ...` line after the JSON output

### find output
- default `find` output is summary JSON for each matched symbol: `kind`, `file_name`, `path`, `span`
- `find -l` outputs the full symbol JSON details


## Todos
- long term: add support for C#