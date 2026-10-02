package apt

import (
	"bufio"
	"fmt"
	"regexp"
	"strings"
)

var versionConstraint = regexp.MustCompile(`\s*\([^)]*\)`)
var architectureRestriction = regexp.MustCompile(`\s*\[[^]]*\]`)
var profileRestriction = regexp.MustCompile(`\s*<[^>]*>`)

func parseInstalled(output string) map[string]string {
	result := make(map[string]string)
	scanner := bufio.NewScanner(strings.NewReader(output))
	for scanner.Scan() {
		fields := strings.SplitN(scanner.Text(), "\t", 3)
		if len(fields) != 3 || len(fields[2]) < 2 || fields[2][1] != 'i' {
			continue
		}
		name := normalizePackageName(fields[0])
		if name != "" {
			result[name] = strings.TrimSpace(fields[1])
		}
	}
	return result
}

func parseSearch(output string) map[string]string {
	result := make(map[string]string)
	scanner := bufio.NewScanner(strings.NewReader(output))
	for scanner.Scan() {
		line := scanner.Text()
		separator := strings.Index(line, " - ")
		if separator <= 0 {
			continue
		}
		name := normalizePackageName(line[:separator])
		if name != "" {
			result[name] = strings.TrimSpace(line[separator+3:])
		}
	}
	return result
}

func parseUpgradable(output string) map[string]string {
	result := make(map[string]string)
	scanner := bufio.NewScanner(strings.NewReader(output))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "Listing...") || strings.HasPrefix(line, "WARNING:") {
			continue
		}
		slash := strings.IndexByte(line, '/')
		if slash <= 0 {
			continue
		}
		name := normalizePackageName(line[:slash])
		fields := strings.Fields(line[slash+1:])
		if name != "" && len(fields) >= 2 {
			result[name] = fields[1]
		}
	}
	return result
}

func parsePolicy(output string) (installed, candidate string) {
	scanner := bufio.NewScanner(strings.NewReader(output))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		switch {
		case strings.HasPrefix(line, "Installed:"):
			installed = strings.TrimSpace(strings.TrimPrefix(line, "Installed:"))
		case strings.HasPrefix(line, "Candidate:"):
			candidate = strings.TrimSpace(strings.TrimPrefix(line, "Candidate:"))
		}
	}
	return installed, candidate
}

func parseDetails(output string) (Details, error) {
	paragraph := firstParagraph(output)
	if strings.TrimSpace(paragraph) == "" {
		return Details{}, errorsForMissingDetails()
	}
	fields := parseDeb822(paragraph)
	details := Details{
		Name:         fields["Package"],
		Version:      fields["Version"],
		Architecture: fields["Architecture"],
		Section:      fields["Section"],
		Maintainer:   fields["Maintainer"],
		Homepage:     fields["Homepage"],
		Description:  formatDescription(fields["Description"]),
	}
	for _, kind := range []string{"Pre-Depends", "Depends"} {
		details.Dependencies = append(details.Dependencies, parseDependencies(kind, fields[kind])...)
	}
	return details, nil
}

func firstParagraph(output string) string {
	lines := strings.Split(strings.ReplaceAll(output, "\r\n", "\n"), "\n")
	var paragraph []string
	started := false
	for _, line := range lines {
		if strings.TrimSpace(line) == "" {
			if started {
				break
			}
			continue
		}
		started = true
		paragraph = append(paragraph, line)
	}
	return strings.Join(paragraph, "\n")
}

func parseDeb822(paragraph string) map[string]string {
	fields := make(map[string]string)
	var current string
	for _, line := range strings.Split(paragraph, "\n") {
		if strings.HasPrefix(line, " ") || strings.HasPrefix(line, "\t") {
			if current != "" {
				fields[current] += "\n" + strings.TrimPrefix(strings.TrimPrefix(line, " "), "\t")
			}
			continue
		}
		colon := strings.IndexByte(line, ':')
		if colon <= 0 {
			continue
		}
		current = line[:colon]
		fields[current] = strings.TrimSpace(line[colon+1:])
	}
	return fields
}

func formatDescription(value string) string {
	if value == "" {
		return "No description available."
	}
	lines := strings.Split(value, "\n")
	for i, line := range lines {
		if line == "." {
			lines[i] = ""
		}
	}
	return strings.Join(lines, "\n")
}

func parseDependencies(kind, value string) []Dependency {
	if strings.TrimSpace(value) == "" {
		return nil
	}
	var dependencies []Dependency
	for _, group := range strings.Split(strings.ReplaceAll(value, "\n", " "), ",") {
		for _, alternative := range strings.Split(group, "|") {
			name := strings.TrimSpace(alternative)
			name = versionConstraint.ReplaceAllString(name, "")
			name = architectureRestriction.ReplaceAllString(name, "")
			name = profileRestriction.ReplaceAllString(name, "")
			name = strings.TrimSpace(name)
			if name != "" {
				dependencies = append(dependencies, Dependency{Kind: kind, Name: name})
			}
		}
	}
	return dependencies
}

func errorsForMissingDetails() error {
	return fmt.Errorf("package details were empty")
}
