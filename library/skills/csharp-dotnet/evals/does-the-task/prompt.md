---
description: Integration test for a minimal API endpoint backed by EF Core and PostgreSQL.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

My ASP.NET Core minimal API targets net10.0 and stores orders with EF Core on PostgreSQL (Npgsql). The test project uses xUnit v3. Write the test class that checks POST /orders with {"sku":"A-1","quantity":2} returns 201 and that the order is really saved. Here is Program.cs:

```csharp
var builder = WebApplication.CreateBuilder(args);
builder.Services.AddDbContext<ShopDb>(o => o.UseNpgsql(builder.Configuration.GetConnectionString("Shop")));
var app = builder.Build();
app.MapPost("/orders", async (NewOrder dto, ShopDb db) => {
    var order = new Order { Sku = dto.Sku, Quantity = dto.Quantity };
    db.Orders.Add(order);
    await db.SaveChangesAsync();
    return Results.Created($"/orders/{order.Id}", order);
});
app.Run();
public partial class Program;
```
