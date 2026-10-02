package ui

// min and max are defined here rather than relying on the predeclared builtins,
// which arrived in Go 1.21. Debian 12 — the target platform — ships Go 1.19, and
// tooln should build with the distribution's own toolchain.
func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
