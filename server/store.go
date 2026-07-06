package main

import (
	"crypto/pbkdf2"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"sort"
	"sync"
)

const pbkdf2Iters = 120_000 // OWASP-ish floor for PBKDF2-HMAC-SHA256

// store is the in-memory storefront data: products, users, carts, orders.
// A real service would back this with a database; the RPC surface is identical.
type store struct {
	mu       sync.Mutex
	products map[string]*Product
	users    map[string]*User    // by email
	carts    map[string]*Cart    // by user sub (email)
	orders   map[string]*Order   // by order id
	orderSeq int
}

// Product — a catalog item (clothing or accessory).
type Product struct {
	ID        string   `json:"id"`
	Name      string   `json:"name"`
	Category  string   `json:"category"` // "tops","bottoms","outerwear","footwear","accessories"
	Price     int      `json:"price"`    // cents
	Currency  string   `json:"currency"`
	Colors    []string `json:"colors"`
	Sizes     []string `json:"sizes"`
	Image     string   `json:"image"` // emoji/glyph stand-in (no external assets)
	Blurb     string   `json:"blurb"`
	Stock     int      `json:"stock"`
	Featured  bool     `json:"featured"`
	Rating    float64  `json:"rating"`
}

// User — a shopper or admin. Password stored salted-hashed (demo-grade).
type User struct {
	Sub   string `json:"sub"`  // email
	Name  string `json:"name"`
	Role  string `json:"role"` // "user" | "admin"
	salt  []byte
	pwKey []byte
}

// CartItem / Cart — the shopper's basket.
type CartItem struct {
	ProductID string `json:"productId"`
	Qty       int    `json:"qty"`
}
type Cart struct {
	Items []CartItem `json:"items"`
}

// Order — a placed order.
type Order struct {
	ID       string     `json:"id"`
	Sub      string     `json:"sub"`
	Items    []CartItem `json:"items"`
	Subtotal int        `json:"subtotal"`
	Shipping ShipInfo   `json:"shipping"`
	Status   string     `json:"status"`
	Created  int64      `json:"created"`
}
type ShipInfo struct {
	Name    string `json:"name"`
	Address string `json:"address"`
	City    string `json:"city"`
	Zip     string `json:"zip"`
}

// pwHash derives a password hash with PBKDF2-HMAC-SHA256 (stdlib, Go 1.24+).
// A real deployment would use argon2id; PBKDF2 is the strongest stdlib KDF and
// far better than a bare salted hash.
func pwHash(salt []byte, pw string) []byte {
	dk, err := pbkdf2.Key(sha256.New, pw, salt, pbkdf2Iters, 32)
	if err != nil {
		panic(err) // only errors on absurd params
	}
	return dk
}

func randSalt() []byte {
	b := make([]byte, 16)
	_, _ = rand.Read(b)
	return b
}

func newStore() *store {
	s := &store{
		products: map[string]*Product{},
		users:    map[string]*User{},
		carts:    map[string]*Cart{},
		orders:   map[string]*Order{},
	}
	s.seedUsers()
	s.seedCatalog()
	return s
}

func (s *store) addUser(sub, name, role, pw string) {
	salt := randSalt()
	s.users[sub] = &User{Sub: sub, Name: name, Role: role, salt: salt, pwKey: pwHash(salt, pw)}
}

func (s *store) seedUsers() {
	s.addUser("shopper@aurelia.dev", "Ava Shopper", "user", "shop1234")
	s.addUser("admin@aurelia.dev", "Aurelia Admin", "admin", "admin1234")
}

func (s *store) checkPassword(email, pw string) *User {
	u := s.users[email]
	if u == nil {
		return nil
	}
	if subtle.ConstantTimeCompare(pwHash(u.salt, pw), u.pwKey) != 1 {
		return nil
	}
	return u
}

// listProducts returns products filtered by category (""=all), sorted featured-first.
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

func (s *store) cartFor(sub string) *Cart {
	if s.carts[sub] == nil {
		s.carts[sub] = &Cart{}
	}
	return s.carts[sub]
}

func (s *store) seedCatalog() {
	items := []*Product{
		{ID: "linen-shirt", Name: "Linen Camp Shirt", Category: "tops", Price: 8900, Colors: []string{"Sand", "Sage", "White"}, Sizes: []string{"S", "M", "L", "XL"}, Image: "👔", Blurb: "Breathable European linen with a relaxed camp collar.", Stock: 40, Featured: true, Rating: 4.7},
		{ID: "merino-tee", Name: "Merino Wool Tee", Category: "tops", Price: 6500, Colors: []string{"Charcoal", "Navy", "Oat"}, Sizes: []string{"XS", "S", "M", "L", "XL"}, Image: "👕", Blurb: "Ultrafine 17.5-micron merino — soft, odor-resistant, all-season.", Stock: 60, Rating: 4.6},
		{ID: "oxford-shirt", Name: "Brushed Oxford Shirt", Category: "tops", Price: 9800, Colors: []string{"Blue", "White", "Pink"}, Sizes: []string{"S", "M", "L", "XL"}, Image: "🧥", Blurb: "Heavyweight brushed cotton oxford with mother-of-pearl buttons.", Stock: 25, Rating: 4.5},
		{ID: "tailored-chino", Name: "Tailored Stretch Chino", Category: "bottoms", Price: 11800, Colors: []string{"Stone", "Olive", "Navy"}, Sizes: []string{"28", "30", "32", "34", "36"}, Image: "👖", Blurb: "Comfort-stretch twill with a clean tapered leg.", Stock: 35, Featured: true, Rating: 4.8},
		{ID: "selvedge-denim", Name: "Selvedge Denim Jeans", Category: "bottoms", Price: 16500, Colors: []string{"Indigo", "Washed Black"}, Sizes: []string{"28", "30", "32", "34", "36"}, Image: "👖", Blurb: "14oz Japanese selvedge denim, sanforized, button fly.", Stock: 20, Rating: 4.9},
		{ID: "wool-trouser", Name: "Pleated Wool Trouser", Category: "bottoms", Price: 14200, Colors: []string{"Grey", "Charcoal"}, Sizes: []string{"28", "30", "32", "34"}, Image: "👖", Blurb: "Single-pleat tropical wool with a tailored drape.", Stock: 18, Rating: 4.4},
		{ID: "field-jacket", Name: "Waxed Field Jacket", Category: "outerwear", Price: 24500, Colors: []string{"Olive", "Black"}, Sizes: []string{"S", "M", "L", "XL"}, Image: "🧥", Blurb: "British waxed cotton, corduroy collar, four bellows pockets.", Stock: 15, Featured: true, Rating: 4.8},
		{ID: "puffer-vest", Name: "Recycled Puffer Vest", Category: "outerwear", Price: 13900, Colors: []string{"Black", "Rust", "Slate"}, Sizes: []string{"S", "M", "L", "XL"}, Image: "🦺", Blurb: "800-fill recycled down, packs into its own pocket.", Stock: 30, Rating: 4.5},
		{ID: "chelsea-boot", Name: "Suede Chelsea Boot", Category: "footwear", Price: 19800, Colors: []string{"Tobacco", "Black"}, Sizes: []string{"8", "9", "10", "11", "12"}, Image: "🥾", Blurb: "Italian suede, Goodyear-welted, crepe sole.", Stock: 22, Featured: true, Rating: 4.7},
		{ID: "canvas-sneaker", Name: "Low Canvas Sneaker", Category: "footwear", Price: 7900, Colors: []string{"White", "Navy", "Olive"}, Sizes: []string{"7", "8", "9", "10", "11", "12"}, Image: "👟", Blurb: "Vulcanized canvas court sneaker, cotton laces.", Stock: 50, Rating: 4.3},
		{ID: "leather-belt", Name: "Full-Grain Leather Belt", Category: "accessories", Price: 5900, Colors: []string{"Tan", "Brown", "Black"}, Sizes: []string{"S", "M", "L"}, Image: "🎗️", Blurb: "Vegetable-tanned full-grain leather, solid brass buckle.", Stock: 45, Rating: 4.6},
		{ID: "wool-scarf", Name: "Lambswool Scarf", Category: "accessories", Price: 6800, Colors: []string{"Camel", "Grey", "Forest"}, Sizes: []string{"One Size"}, Image: "🧣", Blurb: "Scottish lambswool, brushed for softness, fringed ends.", Stock: 38, Rating: 4.5},
		{ID: "leather-tote", Name: "Minimal Leather Tote", Category: "accessories", Price: 21500, Colors: []string{"Cognac", "Black"}, Sizes: []string{"One Size"}, Image: "👜", Blurb: "Full-grain leather tote with a laptop sleeve and magnetic closure.", Stock: 16, Featured: true, Rating: 4.8},
		{ID: "aviator-sunglasses", Name: "Aviator Sunglasses", Category: "accessories", Price: 8500, Colors: []string{"Gold/Green", "Silver/Grey"}, Sizes: []string{"One Size"}, Image: "🕶️", Blurb: "Polarized glass lenses, lightweight metal frame, UV400.", Stock: 42, Rating: 4.4},
		{ID: "beanie", Name: "Ribbed Merino Beanie", Category: "accessories", Price: 4200, Colors: []string{"Black", "Oat", "Rust"}, Sizes: []string{"One Size"}, Image: "🧢", Blurb: "Fine-gauge ribbed merino, cuffed, warm without bulk.", Stock: 55, Rating: 4.3},
	}
	for _, p := range items {
		p.Currency = "USD"
		s.products[p.ID] = p
	}
}
