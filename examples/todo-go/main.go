// Command todo-go: the tri-language Todo spec (examples/TODO_SPEC.md) in Go.
package main

import (
	"fmt"
	"time"

	"github.com/monstercameron/gowebbrowser/sdk/gwb"
)

const atomTextDecoration uint32 = 1024

type todo struct {
	label uint32 // label span
	text  uint32 // label's text node
	done  bool
}

var (
	todos      = map[uint32]*todo{} // keyed by item row id
	byLabel    = map[uint32]uint32{}
	byXButton  = map[uint32]uint32{}
	listID     uint32
	statusText uint32
	inputID    uint32
	addBtn     uint32
	hundredBtn uint32
	inputValue string
	created    int
	doneCount  int
)

func addTodo(text string) {
	created++
	if text == "" {
		text = fmt.Sprintf("Item %d", created)
	}
	row := gwb.NewID()
	gwb.CreateElement(row, gwb.Div)
	gwb.SetStyle(row, gwb.StyleDisplay, "flex")
	gwb.SetStyle(row, gwb.StyleGap, "8px")
	gwb.SetStyle(row, gwb.StyleMargin, "0 0 6px 0")

	label := gwb.NewID()
	gwb.CreateElement(label, gwb.Span)
	gwb.SetStyle(label, gwb.StyleCursor, "pointer")
	labelText := gwb.NewID()
	gwb.CreateText(labelText, text)
	gwb.AppendChild(label, labelText)
	gwb.AppendChild(row, label)

	x := gwb.NewID()
	gwb.CreateElement(x, gwb.Button)
	xText := gwb.NewID()
	gwb.CreateText(xText, "x")
	gwb.AppendChild(x, xText)
	gwb.AppendChild(row, x)

	gwb.AppendChild(listID, row)
	gwb.Listen(label, gwb.EvClick)
	gwb.Listen(x, gwb.EvClick)

	todos[row] = &todo{label: label, text: labelText}
	byLabel[label] = row
	byXButton[x] = row
}

func updateStatus() {
	gwb.SetText(statusText, fmt.Sprintf("%d items, %d done", len(todos), doneCount))
}

func newTextElement(tag uint32, text string) uint32 {
	el := gwb.NewID()
	gwb.CreateElement(el, tag)
	t := gwb.NewID()
	gwb.CreateText(t, text)
	gwb.AppendChild(el, t)
	return el
}

func init() {
	gwb.OnStart = func(w, h, scale float32, flags uint32) {
		gwb.DefineAtom(atomTextDecoration, "text-decoration")

		card := gwb.NewID()
		gwb.CreateElement(card, gwb.Div)
		gwb.SetStyle(card, gwb.StylePadding, "20px")
		gwb.SetStyle(card, gwb.StyleBackground, "#26282c")
		gwb.SetStyle(card, gwb.StyleBorderRadius, "10px")
		gwb.SetStyle(card, gwb.StyleWidth, "420px")
		gwb.AppendChild(gwb.Root, card)

		heading := newTextElement(gwb.H2, "Todos — Go")
		gwb.SetStyle(heading, gwb.StyleMargin, "0 0 12px 0")
		gwb.AppendChild(card, heading)

		row := gwb.NewID()
		gwb.CreateElement(row, gwb.Div)
		gwb.SetStyle(row, gwb.StyleDisplay, "flex")
		gwb.SetStyle(row, gwb.StyleGap, "8px")
		gwb.SetStyle(row, gwb.StyleMargin, "0 0 14px 0")
		gwb.AppendChild(card, row)

		inputID = gwb.NewID()
		gwb.CreateElement(inputID, gwb.Input)
		gwb.SetAttr(inputID, gwb.AttrType, "text")
		gwb.SetAttr(inputID, gwb.AttrPlaceholder, "What needs doing?")
		gwb.SetStyle(inputID, gwb.StyleWidth, "240px")
		gwb.AppendChild(row, inputID)
		gwb.Listen(inputID, gwb.EvInput)

		addBtn = newTextElement(gwb.Button, "Add")
		gwb.AppendChild(row, addBtn)
		gwb.Listen(addBtn, gwb.EvClick)

		hundredBtn = newTextElement(gwb.Button, "+100")
		gwb.AppendChild(row, hundredBtn)
		gwb.Listen(hundredBtn, gwb.EvClick)

		listID = gwb.NewID()
		gwb.CreateElement(listID, gwb.Div)
		gwb.AppendChild(card, listID)

		status := gwb.NewID()
		gwb.CreateElement(status, gwb.P)
		statusText = gwb.NewID()
		gwb.CreateText(statusText, "0 items, 0 done")
		gwb.AppendChild(status, statusText)
		gwb.SetStyle(status, gwb.StyleMargin, "10px 0 0 0")
		gwb.SetStyle(status, gwb.StyleFontSize, "12px")
		gwb.SetStyle(status, gwb.StyleColor, "#9a9fa6")
		gwb.AppendChild(card, status)
	}

	gwb.OnEvent = func(e *gwb.Event) uint32 {
		switch e.Kind {
		case gwb.EvInput:
			if e.Listener == inputID {
				inputValue = e.Str
			}
		case gwb.EvClick:
			switch {
			case e.Listener == addBtn:
				addTodo(inputValue)
				inputValue = ""
				gwb.SetAttr(inputID, gwb.AttrValue, "")
				updateStatus()
			case e.Listener == hundredBtn:
				start := time.Now()
				for range 100 {
					addTodo("")
				}
				updateStatus()
				gwb.Log(gwb.LogInfo, fmt.Sprintf("+100 encoded in %.2fms (guest)", float64(time.Since(start).Microseconds())/1000.0))
			default:
				if row, ok := byLabel[e.Listener]; ok {
					t := todos[row]
					t.done = !t.done
					if t.done {
						doneCount++
						gwb.SetStyle(t.label, gwb.StyleColor, "#9a9fa6")
						gwb.SetStyle(t.label, atomTextDecoration, "line-through")
					} else {
						doneCount--
						gwb.SetStyle(t.label, gwb.StyleColor, "#e8e8e8")
						gwb.RemoveStyle(t.label, atomTextDecoration)
					}
					updateStatus()
				} else if row, ok := byXButton[e.Listener]; ok {
					if t := todos[row]; t != nil && t.done {
						doneCount--
					}
					delete(byLabel, todos[row].label)
					delete(todos, row)
					delete(byXButton, e.Listener)
					gwb.Remove(row)
					updateStatus()
				}
			}
		}
		return 0
	}
}

func main() {} // wasip1 reactor
