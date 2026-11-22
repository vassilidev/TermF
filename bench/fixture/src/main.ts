import { createServer } from "./app.ts";

const port = Number(process.env.PORT ?? 3000);
createServer(process.env.DB_PATH ?? "tasks.db").listen(port, () => {
  console.log(`Tasks on http://localhost:${port}`);
});
