package main

import (
	"sort"
	"sync"
)

// store is the in-memory retailer data: an electronics catalog + guest carts
// keyed by the verified channel app-key (no user accounts in this demo — see
// auth.go). mu guards concurrent requests the same way server/store.go does.
type store struct {
	mu       sync.Mutex
	products map[string]*Product
	carts    map[string]*Cart // by app-key identity
}

// Product — an electronics catalog item.
type Product struct {
	ID       string  `json:"id"`
	Name     string  `json:"name"`
	Category string  `json:"category"` // "laptops","phones","audio","monitors","accessories"
	Price    int     `json:"price"`    // cents
	Currency string  `json:"currency"`
	Image    string  `json:"image"` // emoji stand-in (no external assets)
	Blurb    string  `json:"blurb"`
	Stock    int     `json:"stock"`
	Featured bool    `json:"featured"`
	Rating   float64 `json:"rating"`
}

type CartItem struct {
	ProductID string `json:"productId"`
	Qty       int    `json:"qty"`
}
type Cart struct {
	Items []CartItem `json:"items"`
}

func newStore() *store {
	s := &store{products: map[string]*Product{}, carts: map[string]*Cart{}}
	s.seedCatalog()
	return s
}

func (s *store) listProducts(category string) []*Product {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []*Product
	for _, p := range s.products {
		if category == "" || category == "all" || p.Category == category {
			out = append(out, p)
		}
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Featured != out[j].Featured {
			return out[i].Featured
		}
		return out[i].Name < out[j].Name
	})
	return out
}

func (s *store) getProduct(id string) *Product {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.products[id]
}

// cartFor must be called with s.mu already held (matches server/store.go's
// cartFor convention — callers lock around the read-modify-write).
func (s *store) cartFor(identity string) *Cart {
	if s.carts[identity] == nil {
		s.carts[identity] = &Cart{}
	}
	return s.carts[identity]
}

func (s *store) seedCatalog() {
	items := []*Product{
		{ID: "workstation-14", Name: "Workstation Laptop 14\"", Category: "laptops", Price: 189900, Image: "💻", Blurb: "14-core CPU, 32GB unified memory, all-day battery.", Stock: 18, Featured: true, Rating: 4.7},
		{ID: "ultrabook-13", Name: "Ultrabook 13\"", Category: "laptops", Price: 109900, Image: "💻", Blurb: "2.4lb magnesium chassis, 1200-nit display, fanless.", Stock: 30, Rating: 4.5},
		{ID: "dev-tower", Name: "Developer Tower PC", Category: "laptops", Price: 249900, Image: "🖥️", Blurb: "24-core desktop, 64GB RAM, dual NVMe, whisper-quiet.", Stock: 10, Rating: 4.6},
		{ID: "flagship-phone", Name: "Flagship Phone", Category: "phones", Price: 99900, Image: "📱", Blurb: "6.7\" OLED, three-lens camera, titanium frame.", Stock: 40, Featured: true, Rating: 4.6},
		{ID: "compact-phone", Name: "Compact Phone", Category: "phones", Price: 69900, Image: "📱", Blurb: "5.4\" one-hand size, all-day battery, IP68.", Stock: 35, Rating: 4.4},
		{ID: "anc-headphones", Name: "ANC Over-Ear Headphones", Category: "audio", Price: 34900, Image: "🎧", Blurb: "Adaptive noise cancellation, 40hr battery, lossless codec.", Stock: 50, Featured: true, Rating: 4.8},
		{ID: "wireless-earbuds", Name: "Wireless Earbuds", Category: "audio", Price: 17900, Image: "🎧", Blurb: "Spatial audio, sweat-resistant, wireless charging case.", Stock: 60, Rating: 4.5},
		{ID: "smart-speaker", Name: "Smart Speaker", Category: "audio", Price: 8900, Image: "🔊", Blurb: "360-degree sound, room-filling bass, voice assistant built in.", Stock: 45, Rating: 4.3},
		{ID: "ultrawide-monitor", Name: "34\" Ultrawide Monitor", Category: "monitors", Price: 64900, Image: "🖥️", Blurb: "3440x1440 curved panel, 144Hz, USB-C 90W passthrough.", Stock: 22, Featured: true, Rating: 4.7},
		{ID: "4k-monitor", Name: "27\" 4K Monitor", Category: "monitors", Price: 42900, Image: "🖥️", Blurb: "4K IPS, factory color-calibrated, height-adjustable stand.", Stock: 28, Rating: 4.6},
		{ID: "mech-keyboard", Name: "Mechanical Keyboard", Category: "accessories", Price: 15900, Image: "⌨️", Blurb: "Hot-swappable switches, aluminum frame, per-key RGB.", Stock: 40, Rating: 4.6},
		{ID: "wireless-mouse", Name: "Wireless Mouse", Category: "accessories", Price: 7900, Image: "🖱️", Blurb: "8000 DPI sensor, silent clicks, 70-day battery.", Stock: 55, Rating: 4.5},
		{ID: "usbc-dock", Name: "USB-C Docking Station", Category: "accessories", Price: 12900, Image: "🔌", Blurb: "Triple 4K output, 100W passthrough, gigabit ethernet.", Stock: 32, Rating: 4.4},
		{ID: "portable-ssd", Name: "2TB Portable SSD", Category: "accessories", Price: 15900, Image: "💾", Blurb: "2000MB/s read, aluminum enclosure, USB-C.", Stock: 38, Rating: 4.7},
	}
	for _, p := range items {
		p.Currency = "USD"
		s.products[p.ID] = p
	}
}
