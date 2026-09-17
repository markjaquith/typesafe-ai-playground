type Item = { sku: string; quantity: number };
type Product = { sku: string; priceCents: number; available: number };
type Customer = { id: string; email: string; active: boolean };
type Order = {
  id: string;
  customerId: string;
  items: Item[];
  totalCents: number;
  paymentId: string;
};

interface Transaction {
  findOrder(key: string): Promise<Order | undefined>;
  lockProducts(skus: string[]): Promise<Product[]>;
  reserve(sku: string, quantity: number): Promise<void>;
  saveOrder(key: string, order: Order): Promise<void>;
  enqueueReceipt(orderId: string, email: string): Promise<void>;
}

interface Services {
  transaction<T>(work: (tx: Transaction) => Promise<T>): Promise<T>;
  customer(id: string): Promise<Customer | undefined>;
  charge(key: string, cents: number): Promise<{ id: string }>;
  log(event: string, fields: Record<string, unknown>): void;
}

class CheckoutError extends Error {
  constructor(public readonly code: string, message: string) {
    super(message);
    this.name = 'CheckoutError';
  }
}

// Money stays in integer cents throughout checkout.
function checkedTotal(products: Product[], items: Item[]): number {
  const catalog = new Map(products.map(product => [product.sku, product]));
  let total = 0;
  for (const item of items) {
    const product = catalog.get(item.sku);
    if (!product) {
      throw new CheckoutError('UNKNOWN_SKU', `Unknown product: ${item.sku}`);
    }
    if (product.available < item.quantity) {
      throw new CheckoutError('OUT_OF_STOCK', `Not enough stock: ${item.sku}`);
    }
    total += product.priceCents * item.quantity;
    if (!Number.isSafeInteger(total) || total < 0) {
      throw new CheckoutError('INVALID_TOTAL', 'Order total is out of range');
    }
  }
  return total;
}

export async function checkout(
  services: Services,
  customerId: string,
  items: Item[],
  idempotencyKey: string,
): Promise<Order> {
  const startedAt = Date.now();
  const requestKey = `${customerId}:${idempotencyKey}`;
  if (!idempotencyKey.trim() || items.length === 0) {
    throw new CheckoutError('INVALID_REQUEST', 'A key and items are required');
  }
  const seen = new Set<string>();
  for (const item of items) {
    if (!Number.isSafeInteger(item.quantity) || item.quantity <= 0) {
      throw new CheckoutError('INVALID_QUANTITY', 'Quantity must be positive');
    }
    if (seen.has(item.sku)) {
      throw new CheckoutError('DUPLICATE_SKU', 'Combine duplicate items first');
    }
    seen.add(item.sku);
  }

  const customer = await services.customer(customerId);
  if (!customer?.active) {
    throw new CheckoutError('INACTIVE_CUSTOMER', 'An active account is required');
  }
  services.log('checkout.started', { customerId, itemCount: items.length });

  // The adapter serializes identical keys; payment retries reuse the same key.
  const order = await services.transaction(async tx => {
    const existing = await tx.findOrder(requestKey);
    if (existing) return existing;
    // Stable lock ordering avoids deadlocks between overlapping carts.
    const skus = items.map(item => item.sku).sort();
    const products = await tx.lockProducts(skus);
    const totalCents = checkedTotal(products, items);
    const payment = await services.charge(requestKey, totalCents);
    for (const item of items) {
      await tx.reserve(item.sku, item.quantity);
    }
    const created: Order = {
      id: crypto.randomUUID(), customerId, items, totalCents, paymentId: payment.id,
    };
    await tx.saveOrder(requestKey, created);
    // An outbox receipt commits atomically with the order, then sends separately.
    await tx.enqueueReceipt(created.id, customer.email);
    return created;
  });

  services.log('checkout.completed', {
    orderId: order.id,
    elapsedMs: Date.now() - startedAt,
  });
  return order;
}
