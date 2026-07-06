// Command click is the GWB ABI v0 demo: a counter that increments on real
// mouse clicks in the Blitz window — DOM built and mutated from Go wasm,
// zero JavaScript.
package main

import (
	"fmt"

	"github.com/monstercameron/gowebbrowser/sdk/gwb"
)

var (
	count     int
	labelText uint32 // text node inside the <h2>
)

func init() {
	gwb.OnStart = func(w, h, scale float32, flags uint32) {
		gwb.Log(gwb.LogInfo, "click demo: building DOM")

		card := gwb.NewID()
		gwb.CreateElement(card, gwb.Div)
		gwb.SetStyle(card, gwb.StylePadding, "20px")
		gwb.SetStyle(card, gwb.StyleBackground, "#26282c")
		gwb.SetStyle(card, gwb.StyleBorderRadius, "10px")
		gwb.SetStyle(card, gwb.StyleWidth, "320px")
		gwb.AppendChild(gwb.Root, card)

		heading := gwb.NewID()
		gwb.CreateElement(heading, gwb.H2)
		labelText = gwb.NewID()
		gwb.CreateText(labelText, "Count: 0")
		gwb.AppendChild(heading, labelText)
		gwb.SetStyle(heading, gwb.StyleMargin, "0 0 12px 0")
		gwb.AppendChild(card, heading)

		btn := gwb.NewID()
		gwb.CreateElement(btn, gwb.Button)
		btnText := gwb.NewID()
		gwb.CreateText(btnText, "Increment")
		gwb.AppendChild(btn, btnText)
		gwb.AppendChild(card, btn)

		gwb.Listen(btn, gwb.EvClick)
	}

	gwb.OnEvent = func(e *gwb.Event) uint32 {
		if e.Kind == gwb.EvClick {
			count++
			gwb.SetText(labelText, fmt.Sprintf("Count: %d", count))
			gwb.Log(gwb.LogInfo, fmt.Sprintf("clicked! count=%d", count))
		}
		return 0
	}
}

func main() {} // wasip1 reactor: all setup happens in init()
