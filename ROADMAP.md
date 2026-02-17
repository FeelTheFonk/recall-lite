# roadmap

- ~~**MCP server**~~ done -- `recall-mcp` binary exposes search as tools over stdio. any MCP client (cursor, claude desktop, copilot) can use it out of the box
- **file watcher** -- the fact that you have to manually re-index after editing files is embarrassing. `notify` crate, watch for changes, auto re-embed only the diff. should've been there from day 1. also `reindex_all` nukes the entire table and rebuilds from scratch which is insane, needs to do delta like `index_folder` does
- **agentic search** -- local LLM that can grep --> read --> reason --> answer in a loop. notebooklm but private
- **linux / mac** -- need cross-platform alternatives for OCR and mica backdrop
- **more file types** -- always

want something? open an issue.
