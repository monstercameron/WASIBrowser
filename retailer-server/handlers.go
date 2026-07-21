package main

// Retailer RPC handlers. Every method is public (see main.go's routes) —
// c.prin is still the verified channel app-key (or "dev:unsigned" when
// channel auth is disabled), used only as the cart's identity key, not for
// any authorization decision.

// --- retailer.catalog.v1 ---

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
		{"id": "laptops", "label": "Laptops"},
		{"id": "phones", "label": "Phones"},
		{"id": "audio", "label": "Audio"},
		{"id": "monitors", "label": "Monitors"},
		{"id": "accessories", "label": "Accessories"},
	}}, nil
}

// --- retailer.cart.v1 (identity = c.prin, the verified channel app-key) ---

func (s *server) cartView(identity string) map[string]any {
	s.store.mu.Lock()
	defer s.store.mu.Unlock()
	cart := s.store.cartFor(identity)
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
	return s.cartView(c.prin), nil
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
	cart := s.store.cartFor(c.prin)
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
	return s.cartView(c.prin), nil
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
	cart := s.store.cartFor(c.prin)
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
	return s.cartView(c.prin), nil
}

func (s *server) cartRemove(c *rpcCtx) (any, *rpcError) {
	var req struct{ ID string }
	if err := c.bind(&req); err != nil {
		return nil, err
	}
	s.store.mu.Lock()
	cart := s.store.cartFor(c.prin)
	next := cart.Items[:0]
	for _, it := range cart.Items {
		if it.ProductID != req.ID {
			next = append(next, it)
		}
	}
	cart.Items = next
	s.store.mu.Unlock()
	return s.cartView(c.prin), nil
}
