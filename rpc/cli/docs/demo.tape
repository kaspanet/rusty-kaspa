# Demo recording for kaspa-rpc.
#
# Render with: vhs demo.tape  (then optimize: gifsicle -O3 --batch demo.gif)
#
# The endpoint is read from ~/.config/kaspa-rpc/config.toml so that no node
# URL or address ever appears on screen. Set that file up before recording,
# e.g. `url = "ws://<NODE>:17210"`, and ensure `kaspa-rpc` is on PATH.

Output demo.gif

Set Shell "bash"
Set Theme "Catppuccin Mocha"
Set FontSize 20
Set Padding 18
Set Width 1100
Set Height 700

# Clean, portable prompt and active shell completion; none of this on camera.
Hide
Type "export PS1='$ '"
Enter
Type "source <(kaspa-rpc completion bash)"
Enter
Type "clear"
Enter
Show

Sleep 1s

# Tab-complete a partial command: `get-i` expands to `get-info`, then run it.
Type "kaspa-rpc get-i"
Sleep 800ms
Tab
Sleep 1.1s
Enter
Sleep 2.5s

# Same call, machine-readable JSON for scripting.
Type "kaspa-rpc --json get-coin-supply"
Sleep 600ms
Enter
Sleep 2.5s

# Tab-complete the subcommand (`subscr` -> `subscribe`), then stream live
# notifications until Ctrl-C.
Type "kaspa-rpc subscr"
Sleep 700ms
Tab
Sleep 1s
Type "virtual-daa-score-changed"
Sleep 500ms
Enter
Sleep 5s
Ctrl+C
Sleep 1.2s
