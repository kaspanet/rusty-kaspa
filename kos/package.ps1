
if ($args.Contains("--dev")) {
    & "cargo nw build --sdk innosetup"
} else {
    & "cargo nw build innosetup"
}
