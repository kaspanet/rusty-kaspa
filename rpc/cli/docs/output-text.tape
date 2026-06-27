# Still: default human-readable text output.
#
# Render with: vhs output-text.tape
#
# Endpoint is taken from ~/.config/kaspa-rpc/config.toml so no node URL is
# shown. Ensure `kaspa-rpc` is on PATH before recording.


Set Theme "Catppuccin Mocha"
Set FontSize 22
Set Padding 20
Set Width 900
Set Height 360

Hide
Type "export PS1='$ '"
Enter
Type "clear"
Enter
Show

Sleep 800ms
Type "kaspa-rpc get-info"
Sleep 500ms
Enter
Sleep 2.5s
Screenshot output-text.png
