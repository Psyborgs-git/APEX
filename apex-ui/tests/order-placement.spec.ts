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
    // Switch to Limit order type first so price input is visible
    await page.getByTestId('order-type-select').selectOption('LIMIT');

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
    // The submit button is disabled when symbol/quantity are empty (disabled attribute).
    // Verify it is disabled initially.
    const submitButton = page.getByTestId('order-submit-button');
    await expect(submitButton).toBeDisabled();
  });

  test('should place a limit buy order', async ({ page }) => {
    // Select order type first to show price input
    await page.getByTestId('order-type-select').selectOption('LIMIT');

    // Fill in order details
    await page.getByTestId('order-symbol-input').fill('BTCUSD');
    await page.getByTestId('order-quantity-input').fill('10');
    await page.getByTestId('order-price-input').fill('50000');

    // Select buy side
    await page.getByTestId('order-side-buy').click();

    // Submit order
    await page.getByTestId('order-submit-button').click();

    // Verify order confirmation or wait for status change
    const confirmation = page.getByTestId('order-confirmation');
    await expect(confirmation).toBeVisible({ timeout: 5000 });
  });

  test('should place a market sell order', async ({ page }) => {
    // Fill in order details for market order (no price needed)
    await page.getByTestId('order-type-select').selectOption('MARKET');
    await page.getByTestId('order-symbol-input').fill('ETHUSD');
    await page.getByTestId('order-quantity-input').fill('1');

    // Select sell side
    await page.getByTestId('order-side-sell').click();

    // Submit order
    await page.getByTestId('order-submit-button').click();

    // Verify order confirmation
    const confirmation = page.getByTestId('order-confirmation');
    await expect(confirmation).toBeVisible({ timeout: 5000 });
  });

  test('should display positions panel', async ({ page }) => {
    // Check positions panel exists
    const positionsPanel = page.getByTestId('positions-panel');
    await expect(positionsPanel).toBeVisible({ timeout: 10000 });
  });

  test('should show buy and sell buttons', async ({ page }) => {
    // Verify buy and sell side buttons exist and are interactive
    const buyButton = page.getByTestId('order-side-buy');
    const sellButton = page.getByTestId('order-side-sell');
    await expect(buyButton).toBeVisible();
    await expect(sellButton).toBeVisible();

    // Click sell to change side
    await sellButton.click();

    // Verify submit button text changes
    const submitButton = page.getByTestId('order-submit-button');
    await expect(submitButton).toContainText('SELL');
  });
});
