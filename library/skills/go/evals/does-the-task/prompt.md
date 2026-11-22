---
description: A table-driven HTTP test from acceptance criteria in a Go service.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Go 1.27 service has this handler, registered on `mux.HandleFunc("POST /refunds", h.CreateRefund)`:

```go
func (h *Handler) CreateRefund(w http.ResponseWriter, r *http.Request) {
    var in struct{ OrderID string `json:"order_id"`; Amount int `json:"amount"` }
    if err := json.NewDecoder(r.Body).Decode(&in); err != nil { http.Error(w, "bad json", 400); return }
    order, err := h.store.Order(r.Context(), in.OrderID)
    if errors.Is(err, ErrNotFound) { http.Error(w, "no such order", 404); return }
    if in.Amount > order.Paid { http.Error(w, "refund exceeds payment", 422); return }
    // ... saves the refund, returns 201
}
```

Criteria: a refund up to the amount paid is created (201); a refund above it is refused (422); an unknown order gives 404. Write the tests.
