#!/bin/sh
# Test-only adapter used by the MCP protocol smoke test. Claude's local
# adapter prepends MCP flags, so this wrapper deliberately ignores argv while
# keeping the heartbeat run alive long enough to exercise the gateway.
sleep "${MCP_TEST_SLEEP_SECONDS:-30}"
