package main

import "fmt"

// Storefront RPC handlers. Each is a method on *server; authz is enforced by the
// route's role in main.go before the handler runs, so a handler that reaches
// here already has a valid principal (c.prin) for its declared role.

// --- shop.auth.v1 ---

func (s *server) login(c *rpcCtx) (any, *rpcError) {
	var req struct{ Email, Password string }
	if err := c.bind(&req); err != nil {
		return nil, err
	}
	u := s.store.checkPassword(req.Email, req.Password)
	if u == nil {
		return nil, errUnauthorized("invalid email or password")
	}
	token := s.auth.issueSession(u)
	return map[string]any{"token": token, "role": u.Role, "name": u.Name, "sub": u.Sub}, nil
}

func (s *server) logout(c *rpcCtx) (any, *rpcError) {
	// Stateless tokens: logout is a client-side clear. Ack for symmetry.
	return map[string]any{"ok": true}, nil
}

func (s *server) me(c *rpcCtx) (any, *rpcError) {
	s.store.mu.Lock()
	u := s.store.users[c.prin.Sub]
	s.store.mu.Unlock()
	if u == nil {
		return nil, errUnauthorized("no such user")
	}
	return map[string]any{"sub": u.Sub, "name": u.Name, "role": u.Role}, nil
}

// --- shop.catalog.v1 ---

func (s *server) catalogList(c *rpcCtx) (any, *rpcError) {
	var req struct{ Category string }
	_ = c.bind(&req)
	return map[string]any{"products": s.store.listProducts(req.Category)}, nil
}

func (s *server) catalogGet(c *rpcCtx) (any, *rpcError) {
	var req struct{ ID string }
	if err := c.bind(&req); err != nil {
		return nil, err
	}
	p := s.store.getProduct(req.ID)
	if p == nil {
		return nil, errNotFound("no product " + req.ID)
	}
	return p, nil
}

func (s *server) catalogCategories(c *rpcCtx) (any, *rpcError) {
	return map[string]any{"categories": []map[string]string{
		{"id": "all", "label": "All"},
		{"id": "tops", "label": "Tops"},
		{"id": "bottoms", "label": "Bottoms"},
		{"id": "outerwear", "label": "Outerwear"},
		{"id": "footwear", "label": "Footwear"},
		{"id": "accessories", "label": "Accessories"},
	}}, nil
}

// --- shop.cart.v1 (all require a logged-in user) ---

func (s *server) cartView(sub string) map[string]any {
	s.store.mu.Lock()
	defer s.store.mu.Unlock()
	cart := s.store.cartFor(sub)
	subtotal := 0
	var lines []map[string]any
	for _, it := range cart.Items {
		p := s.store.products[it.ProductID]
		if p == nil {
			continue
		}
		line := p.Price * it.Qty
		subtotal += line
		lines = append(lines, map[string]any{"product": p, "qty": it.Qty, "lineTotal": line})
	}
	return map[string]any{"items": lines, "subtotal": subtotal, "count": len(cart.Items)}
}

func (s *server) cartGet(c *rpcCtx) (any, *rpcError) {
	return s.cartView(c.prin.Sub), nil
}

func (s *server) cartAdd(c *rpcCtx) (any, *rpcError) {
	var req struct {
		ID  string
		Qty int
	}
	if err := c.bind(&req); err != nil {
		return nil, err
	}
	if req.Qty <= 0 {
		req.Qty = 1
	}
	if s.store.getProduct(req.ID) == nil {
		return nil, errNotFound("no product " + req.ID)
	}
	s.store.mu.Lock()
	cart := s.store.cartFor(c.prin.Sub)
	found := false
	for i := range cart.Items {
		if cart.Items[i].ProductID == req.ID {
			cart.Items[i].Qty += req.Qty
			found = true
			break
		}
	}
	if !found {
		cart.Items = append(cart.Items, CartItem{ProductID: req.ID, Qty: req.Qty})
	}
	s.store.mu.Unlock()
	return s.cartView(c.prin.Sub), nil
}

func (s *server) cartSetQty(c *rpcCtx) (any, *rpcError) {
	var req struct {
		ID  string
		Qty int
	}
	if err := c.bind(&req); err != nil {
		return nil, err
	}
	s.store.mu.Lock()
	cart := s.store.cartFor(c.prin.Sub)
	next := cart.Items[:0]
	for _, it := range cart.Items {
		if it.ProductID == req.ID {
			if req.Qty <= 0 {
				continue // drop
			}
			it.Qty = req.Qty
		}
		next = append(next, it)
	}
	cart.Items = next
	s.store.mu.Unlock()
	return s.cartView(c.prin.Sub), nil
}

func (s *server) cartRemove(c *rpcCtx) (any, *rpcError) {
	var req struct{ ID string }
	if err := c.bind(&req); err != nil {
		return nil, err
	}
	s.store.mu.Lock()
	cart := s.store.cartFor(c.prin.Sub)
	next := cart.Items[:0]
	for _, it := range cart.Items {
		if it.ProductID != req.ID {
			next = append(next, it)
		}
	}
	cart.Items = next
	s.store.mu.Unlock()
	return s.cartView(c.prin.Sub), nil
}

// --- shop.orders.v1 ---

func (s *server) ordersPlace(c *rpcCtx) (any, *rpcError) {
	var req struct{ Shipping ShipInfo }
	if err := c.bind(&req); err != nil {
		return nil, err
	}
	s.store.mu.Lock()
	defer s.store.mu.Unlock()
	cart := s.store.cartFor(c.prin.Sub)
	if len(cart.Items) == 0 {
		return nil, errBadRequest("cart is empty")
	}
	subtotal := 0
	for _, it := range cart.Items {
		if p := s.store.products[it.ProductID]; p != nil {
			subtotal += p.Price * it.Qty
		}
	}
	s.store.orderSeq++
	id := fmt.Sprintf("AUR-%05d", s.store.orderSeq)
	order := &Order{
		ID: id, Sub: c.prin.Sub, Items: cart.Items, Subtotal: subtotal,
		Shipping: req.Shipping, Status: "confirmed", Created: nowMs(),
	}
	s.store.orders[id] = order
	s.store.carts[c.prin.Sub] = &Cart{} // clear
	return order, nil
}

func (s *server) ordersMine(c *rpcCtx) (any, *rpcError) {
	s.store.mu.Lock()
	defer s.store.mu.Unlock()
	var mine []*Order
	for _, o := range s.store.orders {
		if o.Sub == c.prin.Sub {
			mine = append(mine, o)
		}
	}
	return map[string]any{"orders": mine}, nil
}

// --- shop.admin.v1 (admin role) ---

func (s *server) adminUpsert(c *rpcCtx) (any, *rpcError) {
	var p Product
	if err := c.bind(&p); err != nil {
		return nil, err
	}
	if p.ID == "" || p.Name == "" {
		return nil, errBadRequest("product needs id and name")
	}
	if p.Currency == "" {
		p.Currency = "USD"
	}
	s.store.mu.Lock()
	s.store.products[p.ID] = &p
	s.store.mu.Unlock()
	return &p, nil
}

func (s *server) adminDelete(c *rpcCtx) (any, *rpcError) {
	var req struct{ ID string }
	if err := c.bind(&req); err != nil {
		return nil, err
	}
	s.store.mu.Lock()
	_, ok := s.store.products[req.ID]
	delete(s.store.products, req.ID)
	s.store.mu.Unlock()
	if !ok {
		return nil, errNotFound("no product " + req.ID)
	}
	return map[string]any{"ok": true}, nil
}

func (s *server) adminOrders(c *rpcCtx) (any, *rpcError) {
	s.store.mu.Lock()
	defer s.store.mu.Unlock()
	var all []*Order
	for _, o := range s.store.orders {
		all = append(all, o)
	}
	return map[string]any{"orders": all}, nil
}
