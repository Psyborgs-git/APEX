import { test, expect } from '@playwright/test';

test.describe('Order Placement and Execution Flow', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('should display order entry panel', async ({ page }) => {
    // Check if order entry panel is visible
    const orderEntry = page.getByTestId('order-entry-panel');
    await expect(orderEntry).toBeVisible({ timeout: 10000 });
  });

  test('should allow entering order details', async ({ page }) => {
    // Fill in order details
    const symbolInput = page.getByTestId('order-symbol-input');
    await symbolInput.fill('BTCUSD');

    const quantityInput = page.getByTestId('order-quantity-input');
    await quantityInput.fill('0.1');

    const priceInput = page.getByTestId('order-price-input');
    await priceInput.fill('50000');

    // Verify values are entered
    await expect(symbolInput).toHaveValue('BTCUSD');
    await expect(quantityInput).toHaveValue('0.1');
    await expect(priceInput).toHaveValue('50000');
  });

  test('should validate order inputs', async ({ page }) => {
    // Try to submit empty order
    const submitButton = page.getByTestId('order-submit-button');
    await submitButton.click();

    // Check for validation errors
    const errorMessage = page.getByTestId('order-error-message');
    await expect(errorMessage).toBeVisible();
  });

  test('should place a limit buy order', async ({ page }) => {
    // Fill in order details
    await page.getByTestId('order-symbol-input').fill('BTCUSD');
    await page.getByTestId('order-quantity-input').fill('0.1');
    await page.getByTestId('order-price-input').fill('50000');

    // Select order type
    await page.getByTestId('order-type-select').selectOption('limit');
    await page.getByTestId('order-side-buy').click();

    // Submit order
    await page.getByTestId('order-submit-button').click();

    // Verify order confirmation
    const confirmation = page.getByTestId('order-confirmation');
    await expect(confirmation).toBeVisible({ timeout: 5000 });
  });

  test('should place a market sell order', async ({ page }) => {
    // Fill in order details
    await page.getByTestId('order-symbol-input').fill('ETHUSD');
    await page.getByTestId('order-quantity-input').fill('1.0');

    // Select order type
    await page.getByTestId('order-type-select').selectOption('market');
    await page.getByTestId('order-side-sell').click();

    // Submit order
    await page.getByTestId('order-submit-button').click();

    // Verify order confirmation
    const confirmation = page.getByTestId('order-confirmation');
    await expect(confirmation).toBeVisible({ timeout: 5000 });
  });

  test('should display order in positions panel after execution', async ({ page }) => {
    // Place an order
    await page.getByTestId('order-symbol-input').fill('BTCUSD');
    await page.getByTestId('order-quantity-input').fill('0.1');
    await page.getByTestId('order-type-select').selectOption('market');
    await page.getByTestId('order-side-buy').click();
    await page.getByTestId('order-submit-button').click();

    // Wait for order to execute
    await page.waitForTimeout(2000);

    // Check positions panel
    const positionsPanel = page.getByTestId('positions-panel');
    await expect(positionsPanel).toBeVisible();

    // Verify position is displayed
    const position = page.getByTestId('position-BTCUSD');
    await expect(position).toBeVisible({ timeout: 5000 });
  });

  test('should cancel pending order', async ({ page }) => {
    // Place a limit order
    await page.getByTestId('order-symbol-input').fill('BTCUSD');
    await page.getByTestId('order-quantity-input').fill('0.1');
    await page.getByTestId('order-price-input').fill('100000'); // High price to avoid execution
    await page.getByTestId('order-type-select').selectOption('limit');
    await page.getByTestId('order-side-buy').click();
    await page.getByTestId('order-submit-button').click();

    // Find the order in orders list
    const order = page.getByTestId('pending-order').first();
    await expect(order).toBeVisible({ timeout: 5000 });

    // Cancel the order
    const cancelButton = order.getByTestId('cancel-order-button');
    await cancelButton.click();

    // Verify order is cancelled
    await expect(order).not.toBeVisible({ timeout: 3000 });
  });
});
