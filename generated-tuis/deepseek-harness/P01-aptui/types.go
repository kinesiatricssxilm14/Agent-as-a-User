package main

import "fmt"

// Status describes the relationship between a package and the system.
type Status int

const (
	StatusInstalled Status = iota
	StatusAvailable
	StatusUpgradable
)

func (s Status) String() string {
	switch s {
	case StatusInstalled:
		return "installed"
	case StatusAvailable:
		return "available"
	case StatusUpgradable:
		return "upgradable"
	}
	return "unknown"
}

// Glyph returns the compact marker used in the package list.
func (s Status) Glyph() string {
	switch s {
	case StatusInstalled:
		return "●"
	case StatusAvailable:
		return "○"
	case StatusUpgradable:
		return "▲"
	}
	return "?"
}

// Package is a single entry in the package list. It implements the
// bubbles/list Item interface (Title/Description/FilterValue) in addition to
// carrying the data the UI needs to render rows and details.
type Package struct {
	Name         string
	InstalledVer string
	CandidateVer string
	Arch         string
	Status       Status
	Summary      string
}

func (p Package) Title() string       { return p.Name }
func (p Package) Description() string { return p.Summary }
func (p Package) FilterValue() string { return p.Name + " " + p.Summary }

// versionForList returns the version string shown in the package list.
func (p Package) versionForList() string {
	switch p.Status {
	case StatusUpgradable:
		if p.InstalledVer != "" && p.CandidateVer != "" {
			return fmt.Sprintf("%s → %s", p.InstalledVer, p.CandidateVer)
		}
		if p.CandidateVer != "" {
			return p.CandidateVer
		}
		return p.InstalledVer
	case StatusInstalled:
		return p.InstalledVer
	default:
		return p.CandidateVer
	}
}

// PackageDetails holds everything shown in the fixed details panel.
type PackageDetails struct {
	Name          string
	Status        Status
	InstalledVer  string
	CandidateVer  string
	Arch          string
	Maintainer    string
	Homepage      string
	InstalledSize string
	Summary       string
	Description   string
	Depends       []string
	PreDepends    []string
	Recommends    []string
	Suggests      []string
	Conflicts     []string
	Breaks        []string
	Replaces      []string
	Provides      []string
}
