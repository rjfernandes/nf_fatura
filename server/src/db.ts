import { PrismaBetterSqlite3 } from "@prisma/adapter-better-sqlite3";
import { PrismaClient } from "./generated/prisma/client.js";

export const createPrisma = (url: string) =>
  new PrismaClient({ adapter: new PrismaBetterSqlite3({ url }) });

export const prisma = createPrisma(
  process.env.DATABASE_URL ?? "file:./prisma/dev.db",
);
