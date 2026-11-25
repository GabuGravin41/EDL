# Antigravity Plugin: EDL Engineering (`edl-engineering`)

Bundles the `edl` MCP server (`edl mcp`), the `edl-design` workflow skill, and engineering verification guardrails into a single installable Antigravity plugin.

## Prerequisites
Install `edl` onto your `PATH`:
```bash
cargo install --path /path/to/EDL
# or copy target/release/edl into ~/.local/bin/edl
```

## Local Installation in Antigravity
Copy or symlink this folder into your Antigravity plugins directory:
```bash
mkdir -p ~/.gemini/config/plugins
ln -s /ABS/PATH/TO/EDL/plugins/antigravity-edl ~/.gemini/config/plugins/edl-engineering
```
Then enable **EDL — Engineering Description Language** in Antigravity's Customizations page.
