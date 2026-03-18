import { test, expect } from '@playwright/test';

test.describe('Trading Mode and Status (Account Switching)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('should display trading mode in status bar', async ({ page }) => {
    // The status bar shows "Paper" indicating paper trading mode
    const statusBar = page.getByText('Paper', { exact: true });
    await expect(statusBar).toBeVisible({ timeout: 10000 });
  });

  test('should display session P&L', async ({ page }) => {
    // The status bar shows Session P&L value
    const pnlLabel = page.locator('text=Session P&L:');
    await expect(pnlLabel).toBeVisible({ timeout: 5000 });
  });

  test('should display max daily loss limit', async ({ page }) => {
    // The status bar shows max daily loss
    const maxLoss = page.locator('text=Max Loss:');
    await expect(maxLoss).toBeVisible({ timeout: 5000 });
  });

  test('should display positions panel', async ({ page }) => {
    // Positions panel should be visible
    const positionsPanel = page.getByTestId('positions-panel');
    await expect(positionsPanel).toBeVisible({ timeout: 10000 });
  });

  test('should show paper trading indicator in command bar', async ({ page }) => {
    // The command bar shows "Paper Trading"
    const paperTrading = page.locator('text=Paper Trading');
    await expect(paperTrading).toBeVisible({ timeout: 5000 });
  });

  test('should display workspace panel', async ({ page }) => {
    // Workspace panel should be visible and contain layout
    const workspace = page.getByTestId('workspace-panel');
    await expect(workspace).toBeVisible({ timeout: 10000 });
  });

  test('should show current time in status bar', async ({ page }) => {
    // The status bar should contain time (HH:MM:SS format)
    const statusBar = page.locator('div').filter({ hasText: /\d{1,2}:\d{2}:\d{2}/ }).first();
    await expect(statusBar).toBeVisible({ timeout: 5000 });
  });
});
