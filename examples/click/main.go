// Command click is the GWB ABI v1 demo: counter on click, hover feedback via
// pointer enter/leave, layout observation, window-resize awareness, and a
// request_frame pulse animation — all from Go wasm, zero JavaScript.
package main

import (
	"fmt"

	"github.com/monstercameron/gowebbrowser/sdk/gwb"
)

var (
	count     int
	card      uint32
	labelText uint32 // text node inside the <h2>
	statText  uint32 // status line text node
	pulse     uint32 // animated bar
	pulseT    float32
	animating bool
)

func init() {
	gwb.OnStart = func(w, h, scale float32, flags uint32) {
		gwb.Log(gwb.LogInfo, fmt.Sprintf("v1 demo: start viewport=%.0fx%.0f scale=%.1f", w, h, scale))

		card = gwb.NewID()
		gwb.CreateElement(card, gwb.Div)
		gwb.SetStyle(card, gwb.StylePadding, "20px")
		gwb.SetStyle(card, gwb.StyleBackground, "#26282c")
		gwb.SetStyle(card, gwb.StyleBorderRadius, "10px")
		gwb.SetStyle(card, gwb.StyleWidth, "340px")
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

		// Animated pulse bar (driven by request_frame).
		pulse = gwb.NewID()
		gwb.CreateElement(pulse, gwb.Div)
		gwb.SetStyle(pulse, gwb.StyleHeight, "6px")
		gwb.SetStyle(pulse, gwb.StyleWidth, "0%")
		gwb.SetStyle(pulse, gwb.StyleBackground, "#2f6feb")
		gwb.SetStyle(pulse, gwb.StyleBorderRadius, "3px")
		gwb.SetStyle(pulse, gwb.StyleMargin, "14px 0 0 0")
		gwb.AppendChild(card, pulse)

		status := gwb.NewID()
		gwb.CreateElement(status, gwb.P)
		statText = gwb.NewID()
		gwb.CreateText(statText, "hover the card; resize the window")
		gwb.AppendChild(status, statText)
		gwb.SetStyle(status, gwb.StyleMargin, "10px 0 0 0")
		gwb.SetStyle(status, gwb.StyleFontSize, "12px")
		gwb.SetStyle(status, gwb.StyleColor, "#9a9fa6")
		gwb.AppendChild(card, status)

		// v1 event surface.
		gwb.Listen(btn, gwb.EvClick)
		gwb.Listen(card, gwb.EvPointerEnter)
		gwb.Listen(card, gwb.EvPointerLeave)
		gwb.Listen(gwb.Root, gwb.EvWindowResize)
		gwb.Observe(card, gwb.ObserveLayout)
	}

	gwb.OnEvent = func(e *gwb.Event) uint32 {
		switch e.Kind {
		case gwb.EvClick:
			count++
			gwb.SetText(labelText, fmt.Sprintf("Count: %d", count))
			// Kick off (or restart) the pulse animation.
			pulseT = 0
			if !animating {
				animating = true
				gwb.RequestFrame()
			}
		case gwb.EvPointerEnter:
			gwb.SetStyle(card, gwb.StyleBackground, "#2c2f34")
			gwb.SetText(statText, "pointer entered the card")
		case gwb.EvPointerLeave:
			gwb.SetStyle(card, gwb.StyleBackground, "#26282c")
			gwb.SetText(statText, "pointer left the card")
		case gwb.EvWindowResize:
			gwb.SetText(statText, fmt.Sprintf("window resized to %.0fx%.0f", e.W, e.H))
		case gwb.EvObservedLayout:
			gwb.Log(gwb.LogInfo, fmt.Sprintf("card layout: %.0f,%.0f %.0fx%.0f", e.X, e.Y, e.W, e.H))
		}
		return 0
	}

	gwb.OnFrame = func(dtMS float32) {
		pulseT += dtMS
		const duration = 900.0
		if pulseT >= duration {
			gwb.SetStyle(pulse, gwb.StyleWidth, "0%")
			animating = false
			return
		}
		frac := pulseT / duration
		gwb.SetStyle(pulse, gwb.StyleWidth, fmt.Sprintf("%.1f%%", frac*100))
		gwb.RequestFrame()
	}
}

func main() {} // wasip1 reactor: all setup happens in init()
