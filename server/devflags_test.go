package main

import "testing"

func TestDevFlagsLoopbackOnly(t *testing.T) {
	for _, addr := range []string{"127.0.0.1:8787", "localhost:8787", "[::1]:8787", "127.5.6.7:1"} {
		if err := checkDevFlags(addr, true, true); err != nil {
			t.Errorf("%s: loopback should allow dev flags: %v", addr, err)
		}
	}
	for _, addr := range []string{"0.0.0.0:8787", ":8787", "192.168.1.5:8787", "[::]:8787", "example.com:8787", "garbage"} {
		if err := checkDevFlags(addr, true, false); err == nil {
			t.Errorf("%s: -dev-insecure must be refused", addr)
		}
		if err := checkDevFlags(addr, false, true); err == nil {
			t.Errorf("%s: demo accounts must be refused", addr)
		}
		if err := checkDevFlags(addr, false, false); err != nil {
			t.Errorf("%s: production flags should be allowed: %v", addr, err)
		}
	}
}

func TestNoDemoUsersMeansNoKnownPasswords(t *testing.T) {
	s := newServerWith(true, false)
	if u := s.store.checkPassword("admin@aurelia.dev", "admin1234"); u != nil {
		t.Fatal("demo admin exists with demo users disabled")
	}
	if u := newServerWith(true, true).store.checkPassword("shopper@aurelia.dev", "shop1234"); u == nil {
		t.Fatal("demo shopper missing with demo users enabled")
	}
}
