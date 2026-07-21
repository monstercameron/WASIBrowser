// Command retailer-go: a small electronics storefront frontend proving a
// real (non-toy), RPC-backed wasm guest authored entirely in Go — the
// sdk/gwb analogue of examples/shop-c, exercising the RPC/fetch bindings
// added to sdk/gwb for this demo (see sdk/gwb/gwb.go).
//
// Styled via an injected <style> element (same technique shop-c uses) — a
// real class-based stylesheet rather than per-element inline styles, so
// hover states and a considered visual identity are actually possible.
package main

import (
	"encoding/json"
	"fmt"

	"github.com/monstercameron/gowebbrowser/sdk/gwb"
)

// "style" isn't a well-known atom (document-level tags are deliberately
// absent from the well-known table) — define it once, like todo-go defines
// its own custom atom for a property outside the base set.
const atomStyle uint32 = 1030

const css = `
.ct{width:100%;min-height:100%;background:#101216;color:#e7e9ec;
    font-family:-apple-system,'Segoe UI',sans-serif;padding:40px 46px}
.ct-wrap{max-width:1160px;margin:0 auto}
.ct-head{display:flex;justify-content:space-between;align-items:flex-start;margin:0 0 4px 0}
.ct-brand{font-size:25px;font-weight:800;letter-spacing:-.02em;color:#f2f4f7;margin:0}
.ct-dot{color:#4f8cff}
.ct-cart{font-family:ui-monospace,'Cascadia Mono',Consolas,monospace;font-size:12.5px;
         color:#4f8cff;font-variant-numeric:tabular-nums;padding-top:5px}
.ct-sub{color:#8890a0;font-size:12.5px;margin:6px 0 24px 0;max-width:560px;line-height:1.6}
.ct-cats{display:flex;gap:6px;flex-wrap:wrap;margin:0 0 10px 0}
.ct-chip{font-size:12.5px;padding:8px 16px;border-radius:999px;border:1px solid #262b33;
         background:transparent;color:#8890a0;cursor:pointer;font-weight:600}
.ct-chip:hover{border-color:#3a4150;color:#e7e9ec}
.ct-chip.active{border-color:#4f8cff;color:#4f8cff;background:rgba(79,140,255,.08)}
.ct-status{color:#565d6b;font-size:11px;margin:14px 0 16px 0;letter-spacing:.03em;
           text-transform:uppercase}
.ct-grid{display:flex;flex-wrap:wrap;gap:16px}
.ct-card{width:216px;background:#181b21;border:1px solid #262b33;border-radius:12px;
         padding:16px}
.ct-icon{font-family:Georgia,'Times New Roman',serif;font-size:38px;font-weight:700;
         width:56px;height:56px;border-radius:10px;display:flex;align-items:center;
         justify-content:center;margin:0 0 12px 0}
.ct-icon-laptops{background:#1c2b45;color:#6ba3ff}
.ct-icon-phones{background:#2a2035;color:#b98bff}
.ct-icon-audio{background:#2a2418;color:#e0b45c}
.ct-icon-monitors{background:#1a2b28;color:#5cd9b8}
.ct-icon-accessories{background:#2b2020;color:#e08a6b}
.ct-name{font-size:14px;font-weight:700;margin:0 0 4px 0;color:#f2f4f7;line-height:1.3}
.ct-blurb{font-size:11.5px;color:#8890a0;line-height:1.5;margin:0 0 12px 0}
.ct-featured{display:inline-block;background:#ffb020;color:#161104;font-size:9px;
             font-weight:800;letter-spacing:.08em;padding:2px 6px;border-radius:5px;
             text-transform:uppercase;margin:0 0 8px 0}
.ct-price-row{display:flex;justify-content:space-between;align-items:baseline;
              font-family:ui-monospace,'Cascadia Mono',Consolas,monospace;margin:0 0 12px 0}
.ct-price{font-size:15px;font-weight:700;color:#4f8cff;font-variant-numeric:tabular-nums}
.ct-rating{font-size:10.5px;color:#565d6b}
.ct-add{width:100%;background:#4f8cff;color:#0b1220;border:none;font-weight:700;
        font-size:12.5px;padding:9px 0;border-radius:8px;cursor:pointer}
.ct-add:hover{background:#6d9fff}
.ct-links{display:flex;gap:20px;margin:32px 0 0 0;padding:16px 0 0 0;border-top:1px solid #262b33}
.ct-link{background:transparent;border:none;color:#5b7ab0;font-family:inherit;font-size:12px;
         cursor:pointer;letter-spacing:.01em;padding:0}
.ct-link:hover{color:#6d9fff}
`

type product struct {
	ID       string  `json:"id"`
	Name     string  `json:"name"`
	Category string  `json:"category"`
	Price    int     `json:"price"`
	Image    string  `json:"image"`
	Blurb    string  `json:"blurb"`
	Rating   float64 `json:"rating"`
	Featured bool    `json:"featured"`
}

type catalogResp struct {
	Products []product `json:"products"`
}

type cartResp struct {
	Subtotal int `json:"subtotal"`
	Count    int `json:"count"`
}

var (
	grid       uint32
	statusText uint32
	cartText   uint32
	chipByCat  = map[uint32]string{}
	catChips   = map[string]uint32{} // category id -> chip element id
	activeCat  = "all"

	pendingCatalogReq uint32
	pendingCartReq    uint32

	linkSearch uint32
	linkShop   uint32

	addButtons = map[uint32]string{}
)

// el creates an element and (if non-empty) sets its class attribute.
func el(tag uint32, class string) uint32 {
	id := gwb.NewID()
	gwb.CreateElement(id, tag)
	if class != "" {
		gwb.SetAttr(id, gwb.AttrClass, class)
	}
	return id
}

// textEl is el but with a text child; returns (element, text-node) — SetText
// must target the text node, not the wrapping element (calling it on the
// element silently no-ops instead of replacing the child's content).
func textEl(tag uint32, class, text string) (uint32, uint32) {
	id := el(tag, class)
	t := gwb.NewID()
	gwb.CreateText(t, text)
	gwb.AppendChild(id, t)
	return id, t
}

func money(cents int) string {
	return fmt.Sprintf("$%d.%02d", cents/100, cents%100)
}

// categoryIcon returns a monogram letter + category-tinted class for a
// product tile — proven-safe glyph coverage (matches shop-c's own large-
// letter tile pattern) rather than gambling on emoji the font may not have
// (💻/🖥️ render as tofu boxes in this build; plain Latin letters never do).
func categoryIcon(category string) (letter, class string) {
	switch category {
	case "laptops":
		return "L", "ct-icon ct-icon-laptops"
	case "phones":
		return "P", "ct-icon ct-icon-phones"
	case "audio":
		return "A", "ct-icon ct-icon-audio"
	case "monitors":
		return "M", "ct-icon ct-icon-monitors"
	default:
		return "X", "ct-icon ct-icon-accessories"
	}
}

func setActiveCategory(cat string) {
	if prev, ok := catChips[activeCat]; ok {
		gwb.SetAttr(prev, gwb.AttrClass, "ct-chip")
	}
	activeCat = cat
	if next, ok := catChips[cat]; ok {
		gwb.SetAttr(next, gwb.AttrClass, "ct-chip active")
	}
}

func loadCatalog(category string) {
	setActiveCategory(category)
	gwb.SetText(statusText, fmt.Sprintf("loading %s...", category))
	payload, _ := json.Marshal(map[string]string{"category": category})
	id := gwb.Rpc("catalog", "retailer.catalog.v1", "list", payload, 0)
	if id == 0 {
		gwb.SetText(statusText, "catalog unavailable (service undeclared)")
		return
	}
	pendingCatalogReq = id
}

func refreshCart() {
	id := gwb.Rpc("cart", "retailer.cart.v1", "get", []byte("{}"), 0)
	if id != 0 {
		pendingCartReq = id
	}
}

func addToCart(id string) {
	payload, _ := json.Marshal(map[string]any{"id": id, "qty": 1})
	reqID := gwb.Rpc("cart", "retailer.cart.v1", "add", payload, 0)
	if reqID != 0 {
		pendingCartReq = reqID
	}
}

func renderCatalog(resp catalogResp) {
	gwb.Clear(grid)
	gwb.SetText(statusText, fmt.Sprintf("%d products", len(resp.Products)))
	for _, p := range resp.Products {
		card := el(gwb.Div, "ct-card")
		gwb.AppendChild(grid, card)

		if p.Featured {
			badge, _ := textEl(gwb.Span, "ct-featured", "featured")
			gwb.AppendChild(card, badge)
		}

		letter, iconClass := categoryIcon(p.Category)
		icon, _ := textEl(gwb.Div, iconClass, letter)
		gwb.AppendChild(card, icon)

		name, _ := textEl(gwb.H3, "ct-name", p.Name)
		gwb.AppendChild(card, name)

		blurb, _ := textEl(gwb.P, "ct-blurb", p.Blurb)
		gwb.AppendChild(card, blurb)

		priceRow := el(gwb.Div, "ct-price-row")
		gwb.AppendChild(card, priceRow)
		price, _ := textEl(gwb.Span, "ct-price", money(p.Price))
		gwb.AppendChild(priceRow, price)
		rating, _ := textEl(gwb.Span, "ct-rating", fmt.Sprintf("%.1f★", p.Rating))
		gwb.AppendChild(priceRow, rating)

		addBtn, _ := textEl(gwb.Button, "ct-add", "Add to cart")
		gwb.SetAttr(addBtn, gwb.AttrID, "add-"+p.ID)
		gwb.AppendChild(card, addBtn)
		gwb.Listen(addBtn, gwb.EvClick)
		addButtons[addBtn] = p.ID
	}
}

func init() {
	gwb.OnStart = func(w, h, scale float32, flags uint32) {
		gwb.DefineAtom(atomStyle, "style")
		style := el(atomStyle, "")
		gwb.SetInnerHTML(style, css)
		gwb.AppendChild(gwb.Root, style)

		card := el(gwb.Div, "ct")
		gwb.AppendChild(gwb.Root, card)

		wrap := el(gwb.Div, "ct-wrap")
		gwb.AppendChild(card, wrap)

		headRow := el(gwb.Div, "ct-head")
		gwb.AppendChild(wrap, headRow)

		brand := el(gwb.H1, "ct-brand")
		gwb.AppendChild(headRow, brand)
		brandText, _ := textEl(gwb.Span, "", "circuit")
		gwb.AppendChild(brand, brandText)
		dot, _ := textEl(gwb.Span, "ct-dot", ".")
		gwb.AppendChild(brand, dot)

		cartEl, cartTextNode := textEl(gwb.P, "ct-cart", "cart: 0 items · $0.00")
		cartText = cartTextNode
		gwb.AppendChild(headRow, cartEl)

		sub, _ := textEl(gwb.P, "ct-sub", "An electronics storefront's frontend, written entirely in Go, talking to a Go RPC backend on :8789.")
		gwb.AppendChild(wrap, sub)

		catsRow := el(gwb.Div, "ct-cats")
		gwb.AppendChild(wrap, catsRow)

		categories := []struct{ id, label string }{
			{"all", "All"}, {"laptops", "Laptops"}, {"phones", "Phones"},
			{"audio", "Audio"}, {"monitors", "Monitors"}, {"accessories", "Accessories"},
		}
		for _, cat := range categories {
			class := "ct-chip"
			if cat.id == activeCat {
				class = "ct-chip active"
			}
			chip, _ := textEl(gwb.Button, class, cat.label)
			gwb.SetAttr(chip, gwb.AttrID, "cat-"+cat.id)
			gwb.AppendChild(catsRow, chip)
			gwb.Listen(chip, gwb.EvClick)
			chipByCat[chip] = cat.id
			catChips[cat.id] = chip
		}

		statusEl, statusTextNode := textEl(gwb.P, "ct-status", "loading...")
		statusText = statusTextNode
		gwb.AppendChild(wrap, statusEl)

		grid = el(gwb.Div, "ct-grid")
		gwb.AppendChild(wrap, grid)

		// Cross-site links: gwb.Navigate only succeeds if "search.local"/
		// "shop.local" are in this app's manifest "links" array — see
		// manifests/retailer.local.json.
		links := el(gwb.Div, "ct-links")
		gwb.AppendChild(wrap, links)
		linkSearch, _ = textEl(gwb.Button, "ct-link", "→ compare prices on wasm-search")
		gwb.SetAttr(linkSearch, gwb.AttrID, "link-search")
		gwb.AppendChild(links, linkSearch)
		gwb.Listen(linkSearch, gwb.EvClick)
		linkShop, _ = textEl(gwb.Button, "ct-link", "→ Aurelia clothing store (new tab)")
		gwb.SetAttr(linkShop, gwb.AttrID, "link-shop")
		gwb.AppendChild(links, linkShop)
		gwb.Listen(linkShop, gwb.EvClick)

		loadCatalog("all")
		refreshCart()
	}

	gwb.OnEvent = func(e *gwb.Event) uint32 {
		switch e.Kind {
		case gwb.EvClick:
			if cat, ok := chipByCat[e.Listener]; ok {
				loadCatalog(cat)
				return 0
			}
			if id, ok := addButtons[e.Listener]; ok {
				addToCart(id)
				return 0
			}
			if e.Listener == linkSearch {
				if code := gwb.Navigate("web://search.local", 0); code != 0 {
					gwb.SetText(statusText, fmt.Sprintf("navigate blocked (code %d)", code))
				}
				return 0
			}
			if e.Listener == linkShop {
				if code := gwb.Navigate("web://shop.local", gwb.NavFNewTab); code != 0 {
					gwb.SetText(statusText, fmt.Sprintf("navigate blocked (code %d)", code))
				}
				return 0
			}
		case gwb.EvRpcResult:
			switch e.RpcReqID {
			case pendingCatalogReq:
				pendingCatalogReq = 0
				if !e.RpcOk {
					gwb.SetText(statusText, fmt.Sprintf("catalog failed (err_class=%d)", e.RpcErrClass))
					return 0
				}
				var resp catalogResp
				if err := json.Unmarshal([]byte(e.Str), &resp); err != nil {
					gwb.SetText(statusText, "bad catalog response: "+err.Error())
					return 0
				}
				renderCatalog(resp)
			case pendingCartReq:
				pendingCartReq = 0
				if !e.RpcOk {
					return 0
				}
				var resp cartResp
				if err := json.Unmarshal([]byte(e.Str), &resp); err == nil {
					plural := "s"
					if resp.Count == 1 {
						plural = ""
					}
					gwb.SetText(cartText, fmt.Sprintf("cart: %d item%s · %s", resp.Count, plural, money(resp.Subtotal)))
				}
			}
		}
		return 0
	}
}

func main() {} // wasip1 reactor
