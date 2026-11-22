---
description: A widget test for an acceptance criterion on a cart screen that uses provider.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Acceptance criterion: "When the cart is empty, the cart screen says 'Your cart is empty' and the Checkout button cannot be pressed." Our app runs Flutter 3.50 and uses provider. The screen is:

```dart
class CartScreen extends StatelessWidget {
  const CartScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final cart = context.watch<CartModel>();
    return Scaffold(
      body: cart.items.isEmpty ? const Center(child: Text('Your cart is empty')) : CartList(items: cart.items),
      bottomNavigationBar: FilledButton(
        onPressed: cart.items.isEmpty ? null : () => Navigator.of(context).pushNamed('/checkout'),
        child: const Text('Checkout'),
      ),
    );
  }
}
```

`CartModel` is a `ChangeNotifier` with `List<CartItem> items`. Write the test.
