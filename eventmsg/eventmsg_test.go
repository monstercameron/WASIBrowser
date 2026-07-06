package eventmsg_test

import (
	"testing"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/eventmsg"
	"github.com/monstercameron/gowebbrowser/protocol"
)

func TestRoundTrip(t *testing.T) {
	cases := []engine.Event{
		{Kind: engine.EventClick, Target: protocol.NodeID(42), Value: ""},
		{Kind: engine.EventInput, Target: protocol.NodeID(7), Value: "hello"},
		{Kind: engine.EventKey, Target: protocol.NodeID(1), Value: "Enter"},
	}
	for _, want := range cases {
		buf := eventmsg.Encode(want)
		got, err := eventmsg.Decode(buf)
		if err != nil {
			t.Fatalf("Decode error: %v", err)
		}
		if got != want {
			t.Fatalf("got %+v, want %+v", got, want)
		}
	}
}

func TestDecodeShort(t *testing.T) {
	if _, err := eventmsg.Decode([]byte{0, 1}); err == nil {
		t.Fatal("expected error for short buffer")
	}
}
