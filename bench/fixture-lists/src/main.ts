import { createServer } from "./app.ts";

const port = Number(process.env.PORT ?? 3000);
createServer(process.env.DB_PATH ?? "lists.db", { jobs: true }).listen(port, () => {
  console.log(`Lists on http://localhost:${port}`);
});
