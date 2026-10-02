// Package pkgs defines the data types and interfaces shared by the tooln
// package-manager backends (pip and apt) and the TUI layer.
package pkgs

import "context"

// Package is a single installed package shown in a list view.
type Package struct {
	Name    string
	Version string
	// Summary is the short description (PyPI summary or apt short description).
	Summary string
	// Manager is the backend that produced this entry ("pip" or "apt").
	Manager string
}

// PackageInfo is the detailed information shown in the details pane.
type PackageInfo struct {
	Name          string
	Version       string
	Summary       string
	HomePage      string
	Author        string
	License       string
	Location      string
	InstalledSize string
	Status        string
	// Depends lists direct dependency package names (pip "Requires" or apt
	// "Depends", normalised to bare package names).
	Depends []string
	// RequiredBy lists packages that directly depend on this one.
	RequiredBy []string
	// Description is the long-form description when available.
	Description string
}

// SearchResult is one candidate package returned from a repository search.
type SearchResult struct {
	Name    string
	Version string // latest available version ("" when unknown)
	Summary string
}

// OpResult describes the outcome of a mutating operation (install, uninstall,
// upgrade). Output is the captured command output and Removed lists any
// additional packages that were removed (e.g. pip autoremove).
type OpResult struct {
	Output  string
	Removed []string
}

// Manager is implemented by every supported package manager backend.
type Manager interface {
	// Name returns a short identifier, "pip" or "apt".
	Name() string

	// List returns the installed packages managed by this backend.
	List(ctx context.Context) ([]Package, error)

	// Info returns detailed information for one installed package.
	Info(ctx context.Context, name string) (*PackageInfo, error)

	// Search queries the backend's repository for packages matching query.
	Search(ctx context.Context, query string) ([]SearchResult, error)

	// Install installs a package by name/specifier.
	Install(ctx context.Context, name string) (OpResult, error)

	// Uninstall removes a package. The returned OpResult may report additional
	// packages that were removed because they were no longer needed.
	Uninstall(ctx context.Context, name string) (OpResult, error)

	// Upgrade upgrades a package to the latest available version.
	Upgrade(ctx context.Context, name string) (OpResult, error)

	// SupportsUpgrade reports whether Upgrade is meaningful for this backend.
	SupportsUpgrade() bool
}
