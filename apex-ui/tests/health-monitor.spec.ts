import { test, expect } from '@playwright/test';

test.describe('Health Monitor', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.getByTestId('tab-health').click();
  });

  test('should display Health Monitor component', async ({ page }) => {
    const monitor = page.getByTestId('health-monitor');
    await expect(monitor).toBeVisible({ timeout: 10000 });
  });

  test('should show system overview metrics', async ({ page }) => {
    const overview = page.getByTestId('health-overview');
    await expect(overview).toBeVisible();

    // Check metric cards are present
    await expect(page.getByTestId('health-uptime')).toBeVisible();
    await expect(page.getByTestId('health-memory')).toBeVisible();
    await expect(page.getByTestId('health-subscriptions')).toBeVisible();
    await expect(page.getByTestId('health-open-orders')).toBeVisible();
    await expect(page.getByTestId('health-active-strategies')).toBeVisible();
  });

  test('should display uptime value', async ({ page }) => {
    const uptime = page.getByTestId('health-uptime');
    await expect(uptime).toBeVisible();
    // Should contain time format like "Xh Xm Xs"
    await expect(uptime).toContainText(/\d+h \d+m \d+s/);
  });

  test('should display memory usage', async ({ page }) => {
    const memory = page.getByTestId('health-memory');
    await expect(memory).toBeVisible();
    await expect(memory).toContainText('MB');
  });

  test('should show adapter status section', async ({ page }) => {
    const adapters = page.getByTestId('health-adapters');
    await expect(adapters).toBeVisible();
  });

  test('should display adapter health entries', async ({ page }) => {
    // Wait for health data to load
    await page.waitForTimeout(1000);
    const adapters = page.getByTestId('health-adapters');
    await expect(adapters).toBeVisible();

    // Should show at least yahoo_finance and paper_trading adapters
    const yahoo = page.getByTestId('adapter-yahoo_finance');
    const paper = page.getByTestId('adapter-paper_trading');
    await expect(yahoo).toBeVisible({ timeout: 10000 });
    await expect(paper).toBeVisible({ timeout: 10000 });
  });

  test('should show healthy status for adapters', async ({ page }) => {
    await page.waitForTimeout(1000);
    const yahoo = page.getByTestId('adapter-yahoo_finance');
    await expect(yahoo).toBeVisible({ timeout: 10000 });
    await expect(yahoo).toContainText('healthy');
  });
});
