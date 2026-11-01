CREATE TABLE IF NOT EXISTS "Customer" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "name" TEXT NOT NULL,
    "cnpj" TEXT NOT NULL,
    "address" TEXT NOT NULL,
    "addressNumber" TEXT NOT NULL,
    "addressComplement" TEXT,
    "neighborhood" TEXT NOT NULL,
    "city" TEXT NOT NULL,
    "cityIbgeCode" TEXT,
    "state" TEXT NOT NULL,
    "zipcode" TEXT NOT NULL,
    "recurringValue" INTEGER NOT NULL,
    "hasBankSlip" BOOLEAN NOT NULL DEFAULT false,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    "nickName" TEXT,
    "integration" TEXT,
    "station" TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS "Customer_cnpj_key" ON "Customer"("cnpj");

CREATE TABLE IF NOT EXISTS "Billing" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "customerId" TEXT NOT NULL,
    "competence" TEXT NOT NULL,
    "amountCents" INTEGER NOT NULL,
    "dueDate" TEXT NOT NULL,
    "issueNfse" BOOLEAN NOT NULL DEFAULT true,
    "issueSlip" BOOLEAN NOT NULL DEFAULT false,
    "status" TEXT NOT NULL DEFAULT 'PENDING',
    "error" TEXT,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    CONSTRAINT "Billing_customerId_fkey" FOREIGN KEY ("customerId") REFERENCES "Customer" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "Invoice" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "billingId" TEXT NOT NULL,
    "number" TEXT,
    "accessKey" TEXT,
    "xml" TEXT,
    "pdf" BLOB,
    "raw" TEXT,
    "source" TEXT NOT NULL DEFAULT 'API',
    CONSTRAINT "Invoice_billingId_fkey" FOREIGN KEY ("billingId") REFERENCES "Billing" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS "Invoice_billingId_key" ON "Invoice"("billingId");

CREATE TABLE IF NOT EXISTS "BankSlip" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "billingId" TEXT NOT NULL,
    "codigoSolicitacao" TEXT NOT NULL,
    "nossoNumero" TEXT,
    "linhaDigitavel" TEXT,
    "barcode" TEXT,
    "pdf" BLOB,
    "status" TEXT,
    CONSTRAINT "BankSlip_billingId_fkey" FOREIGN KEY ("billingId") REFERENCES "Billing" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS "BankSlip_billingId_key" ON "BankSlip"("billingId");

CREATE TABLE IF NOT EXISTS "Delivery" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "billingId" TEXT NOT NULL,
    "integration" TEXT NOT NULL,
    "parts" TEXT NOT NULL DEFAULT 'BOTH',
    "status" TEXT NOT NULL,
    "error" TEXT,
    "response" TEXT,
    "sentAt" DATETIME,
    "updatedAt" DATETIME NOT NULL,
    CONSTRAINT "Delivery_billingId_fkey" FOREIGN KEY ("billingId") REFERENCES "Billing" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS "Delivery_billingId_key" ON "Delivery"("billingId");

CREATE TABLE IF NOT EXISTS "DpsSequence" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "billingId" TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS "DpsSequence_billingId_key" ON "DpsSequence"("billingId");
