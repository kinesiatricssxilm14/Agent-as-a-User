package main

import "testing"

// TestStreamingProcess exercises the real process-streaming pipeline used to
// run apt-get, using a trivial shell command instead of apt.
func TestStreamingProcess(t *testing.T) {
	msg := startAptProcess("sh", "-c", "echo one; echo two; exit 3")()
	start, ok := msg.(startedMsg)
	if !ok {
		t.Fatalf("expected startedMsg, got %T: %v", msg, msg)
	}

	var lines []string
	var done *doneMsg
	proc := start.proc
	for done == nil {
		m := proc.nextCmd()()
		switch mm := m.(type) {
		case lineMsg:
			lines = append(lines, string(mm))
		case doneMsg:
			done = &mm
		}
	}

	if len(lines) != 2 || lines[0] != "one" || lines[1] != "two" {
		t.Errorf("unexpected lines: %v", lines)
	}
	if done.err == nil {
		t.Errorf("expected non-nil exit error for exit code 3")
	}
}
