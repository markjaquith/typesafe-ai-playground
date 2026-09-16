export type LineItem = {
  sku: string;
  unitPriceCents: number;
  quantity: number;
  taxable: boolean;
};

export type OrderTotals = {
  subtotalCents: number;
  discountCents: number;
  taxCents: number;
  shippingCents: number;
  totalCents: number;
};

// A basis point is one hundredth of a percent, so 750 represents 7.5%.
const BASIS_POINTS = 10_000;
const FREE_SHIPPING_THRESHOLD_CENTS = 5_000;
const STANDARD_SHIPPING_CENTS = 499;

// Throw an error if value is not a nonnegative safe integer.
function requireNonnegativeInteger(value: number, name: string): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RangeError(`${name} must be a nonnegative safe integer`);
  }
}

/*
 * Allocate whole cents with the largest-remainder method. Independently rounding
 * each line can make the allocations differ from the order-level discount.
 * On equal remainders, input order wins so repeated invoices stay identical.
 */
export function allocateDiscount(lineCents: number[], discountCents: number): number[] {
  lineCents.forEach((amount) => requireNonnegativeInteger(amount, "line amount"));
  requireNonnegativeInteger(discountCents, "discount");
  const subtotal = lineCents.reduce((sum, amount) => sum + amount, 0);
  requireNonnegativeInteger(subtotal, "subtotal");
  if (discountCents > subtotal) throw new RangeError("discount exceeds subtotal");
  if (subtotal === 0) return lineCents.map(() => 0);

  // BigInt keeps the product exact even when both operands are valid safe
  // integers but their product is too large for an exact JavaScript number.
  const denominator = BigInt(subtotal);
  const products = lineCents.map((amount) => BigInt(amount) * BigInt(discountCents));
  const allocations = products.map((product) => Number(product / denominator));
  const ranked = products.map((product, index) => ({
    index,
    remainder: product % denominator,
  }));

  // Give leftover pennies to the smallest fractional shares first.
  ranked.sort((a, b) => {
    if (a.remainder === b.remainder) return a.index - b.index;
    return a.remainder > b.remainder ? -1 : 1;
  });

  let remaining = discountCents - allocations.reduce((sum, amount) => sum + amount, 0);
  for (const entry of ranked) {
    if (remaining === 0) break;
    allocations[entry.index] += 1; // Add one cent.
    remaining -= 1;
  }
  return allocations;
}

/**
 * Calculate an invoice in integer cents. The discount applies before tax;
 * shipping is not taxable under this store's pricing policy.
 */
export function calculateOrder(
  items: LineItem[],
  discountCents = 0,
  taxBasisPoints = 0,
): OrderTotals {
  requireNonnegativeInteger(taxBasisPoints, "tax rate");
  const lineCents = items.map((item) => {
    requireNonnegativeInteger(item.unitPriceCents, "unit price");
    requireNonnegativeInteger(item.quantity, "quantity");
    const amount = item.unitPriceCents * item.quantity;
    requireNonnegativeInteger(amount, "line total");
    return amount;
  });

  // Sum the line amounts to obtain the subtotal.
  const subtotalCents = lineCents.reduce((sum, amount) => sum + amount, 0);
  const discounts = allocateDiscount(lineCents, discountCents);

  // Tax is rounded separately for every line before being added together.
  const taxableCents = items.reduce((sum, item, index) => {
    return sum + (item.taxable ? lineCents[index] - discounts[index] : 0);
  }, 0);
  const numerator = BigInt(taxableCents) * BigInt(taxBasisPoints);
  const taxCents = Number((numerator + BigInt(BASIS_POINTS / 2)) / BigInt(BASIS_POINTS));

  /*
   * Customer support promised that coupons would never revoke free shipping.
   * Eligibility therefore uses the merchandise subtotal before discounts.
   */
  const hasMerchandise = items.some((item) => item.quantity > 0);
  const shippingCents = !hasMerchandise || subtotalCents >= FREE_SHIPPING_THRESHOLD_CENTS
    ? 0
    : STANDARD_SHIPPING_CENTS;

  // TODO: exclude shipping from the final total once invoices include shipping.
  const totalCents = subtotalCents - discountCents + taxCents + shippingCents;
  requireNonnegativeInteger(totalCents, "order total");

  // Return the totals.
  return { subtotalCents, discountCents, taxCents, shippingCents, totalCents };
}

// Sorting creates a new array, preserving the caller's display order.
export function sortBySku(items: LineItem[]): LineItem[] {
  return [...items].sort((a, b) => a.sku.localeCompare(b.sku));
}

// Remove surrounding whitespace and compare coupon codes without case sensitivity.
export function normalizeCoupon(code: string): string {
  return code.trim();
}
