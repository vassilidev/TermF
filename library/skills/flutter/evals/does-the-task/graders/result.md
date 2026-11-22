---
type: llm
---

PASS if the answer writes a widget test (`testWidgets`) in `test/` that pumps CartScreen inside a `ChangeNotifierProvider` (or the provider setup the app uses) with an empty CartModel and a MaterialApp, finds the text 'Your cart is empty', checks that the Checkout button is disabled (its `onPressed` is null, or tapping it does not navigate), and runs it with `flutter test`.
FAIL if it only tests CartModel without the screen, puts this check in `integration_test/` or a device run, or asserts on private state instead of what the screen shows.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
