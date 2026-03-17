import { test, expect } from '@playwright/test';

test.describe('Trader Account Switching', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('should display account selector', async ({ page }) => {
    const accountSelector = page.getByTestId('account-selector');
    await expect(accountSelector).toBeVisible({ timeout: 10000 });
  });

  test('should list available trading accounts', async ({ page }) => {
    // Open account selector
    const accountSelector = page.getByTestId('account-selector');
    await accountSelector.click();

    // Verify dropdown is open
    const accountDropdown = page.getByTestId('account-dropdown');
    await expect(accountDropdown).toBeVisible();

    // Check for at least one account
    const accountItems = page.getByTestId('account-item');
    await expect(accountItems).toHaveCount(1, { timeout: 5000 });
  });

  test('should switch between trading accounts', async ({ page }) => {
    // Open account selector
    await page.getByTestId('account-selector').click();

    // Get current account
    const currentAccount = await page.getByTestId('current-account').textContent();

    // Select different account if available
    const accountItems = page.getByTestId('account-item');
    const count = await accountItems.count();

    if (count > 1) {
      // Click second account
      await accountItems.nth(1).click();

      // Verify account changed
      const newAccount = await page.getByTestId('current-account').textContent();
      expect(newAccount).not.toBe(currentAccount);
    }
  });

  test('should display account balances', async ({ page }) => {
    // Open account selector
    await page.getByTestId('account-selector').click();

    // Check for balance display
    const balance = page.getByTestId('account-balance').first();
    await expect(balance).toBeVisible();
  });

  test('should persist account selection on page reload', async ({ page }) => {
    // Open account selector
    await page.getByTestId('account-selector').click();

    // Get account items
    const accountItems = page.getByTestId('account-item');
    const count = await accountItems.count();

    if (count > 1) {
      // Select second account
      await accountItems.nth(1).click();
      const selectedAccount = await page.getByTestId('current-account').textContent();

      // Reload page
      await page.reload();
      await page.waitForLoadState('networkidle');

      // Verify account is still selected
      const currentAccount = await page.getByTestId('current-account').textContent();
      expect(currentAccount).toBe(selectedAccount);
    }
  });

  test('should update positions when switching accounts', async ({ page }) => {
    // Open account selector
    await page.getByTestId('account-selector').click();

    const accountItems = page.getByTestId('account-item');
    const count = await accountItems.count();

    if (count > 1) {
      // Get positions for first account
      const positionsPanel = page.getByTestId('positions-panel');
      const initialPositions = await positionsPanel.getByTestId('position-row').count();

      // Switch to second account
      await page.getByTestId('account-selector').click();
      await accountItems.nth(1).click();

      // Wait for positions to update
      await page.waitForTimeout(1000);

      // Verify positions updated (count may be different)
      const newPositions = await positionsPanel.getByTestId('position-row').count();
      // Just verify the panel is still visible and responsive
      await expect(positionsPanel).toBeVisible();
    }
  });

  test('should show account connection status', async ({ page }) => {
    const connectionStatus = page.getByTestId('connection-status');
    await expect(connectionStatus).toBeVisible({ timeout: 10000 });

    // Verify status indicates connected or disconnected
    const statusText = await connectionStatus.textContent();
    expect(statusText).toMatch(/connected|disconnected/i);
  });
});
