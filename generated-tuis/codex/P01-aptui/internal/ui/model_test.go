package ui

import (
	"context"
	"strings"
	"testing"

	"toola/internal/apt"
)

type fakeBackend struct{}

func (fakeBackend) Packages(context.Context) ([]apt.Package, error) { return nil, nil }
func (fakeBackend) Details(context.Context, string) (apt.Details, error) {
	return apt.Details{}, nil
}
func (fakeBackend) Install(context.Context, string) (string, error) { return "", nil }
func (fakeBackend) Remove(context.Context, string) (string, error)  { return "", nil }
func (fakeBackend) Upgrade(context.Context, string) (string, error) { return "", nil }
func (fakeBackend) UpgradeAll(context.Context) (string, error)      { return "", nil }
func (fakeBackend) Update(context.Context) (string, error)          { return "", nil }

func TestApplyFilterSearchesNameAndSummary(t *testing.T) {
	model := New(fakeBackend{})
	model.packages = []apt.Package{
		{Name: "curl", Summary: "command line URL transfer tool"},
		{Name: "jq", Summary: "lightweight JSON processor"},
		{Name: "wget", Summary: "retrieves files from the web"},
	}

	model.search.SetValue("json")
	model.applyFilter("")
	if len(model.filtered) != 1 || model.filtered[0].Name != "jq" {
		t.Fatalf("keyword filter returned %#v", model.filtered)
	}

	model.search.SetValue("cur")
	model.applyFilter("")
	if len(model.filtered) != 1 || model.filtered[0].Name != "curl" {
		t.Fatalf("name filter returned %#v", model.filtered)
	}
}

func TestRenderDetailsListsEveryDependency(t *testing.T) {
	details := apt.Details{
		Name:             "example",
		Installed:        true,
		InstalledVersion: "1.0",
		CandidateVersion: "1.1",
		Description:      "A complete package description.",
		Dependencies: []apt.Dependency{
			{Kind: "Pre-Depends", Name: "init-system-helpers"},
			{Kind: "Depends", Name: "libc6"},
			{Kind: "Depends", Name: "libssl3"},
		},
	}
	rendered := renderDetails(details, 80)
	for _, expected := range []string{
		"init-system-helpers",
		"libc6",
		"libssl3",
		"A complete package description.",
		"Installed version",
		"Candidate version",
	} {
		if !strings.Contains(rendered, expected) {
			t.Errorf("details panel does not contain %q", expected)
		}
	}
}
